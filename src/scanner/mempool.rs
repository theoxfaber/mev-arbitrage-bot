//! WebSocket mempool scanner with multi-RPC racing and deduplication.

use crate::scanner::decoder::{DecimalsCache, SwapDecoder};
use crate::types::SandwichOpportunity;
use alloy_primitives::{Address, Bytes, TxHash};
use dashmap::DashMap;
use eyre::Result;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use futures_util::StreamExt;

// ─── Pending Transaction ─────────────────────────────────────────────────────

pub struct PendingTx {
    pub hash: TxHash,
    pub to: Option<Address>,
    pub input: Bytes,
}

// ─── Deduplication Cache ─────────────────────────────────────────────────────

pub struct DedupeCache {
    cache: DashMap<TxHash, Instant>,
    window: Duration,
    max_size: usize,
}

impl DedupeCache {
    pub fn new(window: Duration, max_size: usize) -> Self {
        Self {
            cache: DashMap::with_capacity(max_size / 4),
            window,
            max_size,
        }
    }

    pub fn check_and_insert(&self, key: TxHash) -> bool {
        let is_duplicate = {
            use dashmap::mapref::entry::Entry;
            match self.cache.entry(key) {
                Entry::Occupied(mut entry) => {
                    if entry.get().elapsed() < self.window {
                        true
                    } else {
                        entry.insert(Instant::now());
                        false
                    }
                }
                Entry::Vacant(entry) => {
                    entry.insert(Instant::now());
                    false
                }
            }
        };

        if !is_duplicate && self.cache.len() > self.max_size {
            self.evict_oldest_batch();
        }

        is_duplicate
    }

    fn evict_oldest_batch(&self) {
        let cutoff = Instant::now() - self.window;
        self.cache.retain(|_, v| *v > cutoff);
    }

    pub fn live_size(&self) -> usize {
        self.cache.len()
    }
}

// ─── Scanner ─────────────────────────────────────────────────────────────────

struct RpcStats {
    wins: AtomicU64,
    id: String,
}

pub struct MempoolScanner {
    ws_urls: Vec<String>,
    decoder: Arc<SwapDecoder>,
    dedup: Arc<DedupeCache>,
    rpc_stats: Vec<Arc<RpcStats>>,
}

impl MempoolScanner {
    pub fn new(ws_urls: Vec<String>, decimals: DecimalsCache) -> Self {
        let rpc_stats: Vec<Arc<RpcStats>> = ws_urls
            .iter()
            .enumerate()
            .map(|(i, url)| {
                let host = url::Url::parse(url)
                    .map(|u| u.host_str().unwrap_or("unknown").to_string())
                    .unwrap_or_else(|_| format!("rpc_{i}"));
                Arc::new(RpcStats {
                    wins: AtomicU64::new(0),
                    id: format!("RPC_{i}_{host}"),
                })
            })
            .collect();

        Self {
            ws_urls,
            decoder: Arc::new(SwapDecoder::new(decimals)),
            dedup: Arc::new(DedupeCache::new(Duration::from_secs(60), 150_000)),
            rpc_stats,
        }
    }

    pub async fn start(&self, outgoing: mpsc::Sender<SandwichOpportunity>) -> Result<()> {
        tracing::info!(
            rpc_count = self.ws_urls.len(),
            "Starting mempool scanner on {} RPC endpoints",
            self.ws_urls.len()
        );

        for (i, url) in self.ws_urls.iter().enumerate() {
            let url = url.clone();
            let outgoing = outgoing.clone();
            let decoder = Arc::clone(&self.decoder);
            let dedup = Arc::clone(&self.dedup);
            let stats = Arc::clone(&self.rpc_stats[i]);

            tokio::spawn(async move {
                if let Err(e) = Self::subscribe_and_process(
                    &url,
                    outgoing,
                    decoder,
                    dedup,
                    stats
                ).await {
                    tracing::error!(url = %url, error = %e, "Mempool subscription failed");
                }
            });
        }

        Ok(())
    }

    async fn subscribe_and_process(
        url: &str,
        outgoing: mpsc::Sender<SandwichOpportunity>,
        decoder: Arc<SwapDecoder>,
        dedup: Arc<DedupeCache>,
        stats: Arc<RpcStats>,
    ) -> Result<()> {
        use alloy::providers::{Provider, ProviderBuilder};
        use alloy::consensus::Transaction;
        use tokio::time::{sleep, Duration};

        let mut retry_delay = Duration::from_secs(1);

        loop {
            let provider_res = ProviderBuilder::new()
                .on_ws(alloy::rpc::client::WsConnect::new(url))
                .await;

            let provider = match provider_res {
                Ok(p) => {
                    retry_delay = Duration::from_secs(1); // Reset on success
                    p
                }
                Err(e) => {
                    tracing::error!(rpc = %stats.id, error = %e, "WebSocket connection failed, retrying...");
                    sleep(retry_delay).await;
                    retry_delay = (retry_delay * 2).min(Duration::from_secs(60));
                    continue;
                }
            };

            let sub_res = provider.subscribe_full_pending_transactions().await;
            let sub = match sub_res {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!(rpc = %stats.id, error = %e, "Subscription failed, retrying...");
                    sleep(retry_delay).await;
                    continue;
                }
            };

            let mut stream = sub.into_stream();
            tracing::info!(rpc = %stats.id, "Subscribed to pending transactions");

            while let Some(tx) = stream.next().await {
                let hash = *tx.inner.tx_hash();
                if dedup.check_and_insert(hash) {
                    continue;
                }

                stats.wins.fetch_add(1, Ordering::Relaxed);
                crate::metrics::record_tx_scanned(&stats.id);

                if let Some(to) = tx.inner.to() {
                    let input = tx.inner.input();
                    if let Some(opportunity) = decoder.decode(hash, to, input) {
                        if opportunity.is_actionable {
                            let _ = outgoing.try_send(opportunity);
                        }
                    }
                }
            }

            tracing::warn!(rpc = %stats.id, "Stream ended, reconnecting...");
            sleep(Duration::from_secs(1)).await;
        }
    }

    pub fn process_pending_tx(&self, tx: PendingTx, rpc_idx: usize, outgoing: &mpsc::Sender<SandwichOpportunity>) {
        if self.dedup.check_and_insert(tx.hash) {
            return;
        }

        if let Some(stats) = self.rpc_stats.get(rpc_idx) {
            stats.wins.fetch_add(1, Ordering::Relaxed);
        }

        if let Some(to) = tx.to {
            if let Some(opportunity) = self.decoder.decode(tx.hash, to, &tx.input) {
                if opportunity.is_actionable {
                    let _ = outgoing.try_send(opportunity);
                }
            }
        }
    }
}
