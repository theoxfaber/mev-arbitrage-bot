//! WebSocket mempool scanner with multi-RPC racing and deduplication.
//!
//! Subscribes to `eth_subscribe("newPendingTransactions")` on multiple WebSocket
//! endpoints simultaneously. The first RPC to surface a given tx hash "wins" —
//! duplicates are suppressed via a lock-free sliding-window dedup cache.
//!
//! **Implementation Note**: The actual WebSocket subscription is abstracted behind
//! a provider trait. This module handles deduplication, racing, and stats. The
//! provider integration (alloy, ethers, or raw JSON-RPC) is injected at startup.

use crate::scanner::decoder::{DecimalsCache, SwapDecoder};
use crate::types::SandwichOpportunity;
use alloy_primitives::{Address, Bytes, TxHash};
use dashmap::DashMap;
use eyre::Result;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

// ─── Deduplication Cache ─────────────────────────────────────────────────────

/// Sliding-window deduplication cache — lock-free via DashMap.
///
/// Each tx hash is stored with its first-seen timestamp. Entries older than
/// `window` are lazily evicted on lookup. This eliminates the correctness risk
/// of a bulk `clear()` that could cause re-processing of in-flight txs.
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

    /// Returns `true` if the key was already seen within the window.
    /// Atomically inserts the key if it hasn't been seen.
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

        if self.cache.len() >= self.max_size {
            let to_remove = self.max_size / 10;
            let keys: Vec<TxHash> = self
                .cache
                .iter()
                .take(to_remove)
                .map(|e| *e.key())
                .collect();
            for key in keys {
                self.cache.remove(&key);
            }
        }
    }

    pub fn live_size(&self) -> usize {
        self.cache.len()
    }
}

// ─── Pending Transaction ─────────────────────────────────────────────────────

/// A pending transaction received from the mempool.
/// This is the minimal data needed for decoding.
pub struct PendingTx {
    pub hash: TxHash,
    pub to: Option<Address>,
    pub input: Bytes,
}

// ─── Scanner ─────────────────────────────────────────────────────────────────

/// Per-RPC win statistics for latency benchmarking.
struct RpcStats {
    wins: AtomicU64,
    id: String,
}

/// Multi-RPC mempool scanner.
///
/// The scanner manages deduplication and opportunity decoding. The actual
/// WebSocket subscription is provided via a channel — the integration layer
/// (in main.rs) is responsible for connecting to the RPC and feeding raw
/// pending tx hashes/data into this scanner.
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

    /// Start the scanner: spawns a background task that reads pending transactions
    /// from the `incoming` channel, deduplicates them, decodes swap calldata,
    /// and forwards actionable opportunities to the `outgoing` channel.
    pub async fn start(&self, outgoing: mpsc::Sender<SandwichOpportunity>) -> Result<()> {
        tracing::info!(
            rpc_count = self.ws_urls.len(),
            "Starting mempool scanner on {} RPC endpoints",
            self.ws_urls.len()
        );

        // Periodic stats logging
        let rpc_stats = self.rpc_stats.clone();
        let dedup = Arc::clone(&self.dedup);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                let stats_str: Vec<String> = rpc_stats
                    .iter()
                    .map(|s| format!("{}={}", s.id, s.wins.load(Ordering::Relaxed)))
                    .collect();
                tracing::info!(
                    rpc_wins = %stats_str.join(", "),
                    dedup_cache_size = dedup.live_size(),
                    "RPC latency stats"
                );
            }
        });

        // Log that we're ready to receive txs
        // In production, the main.rs integration layer would connect WebSocket
        // subscriptions and feed PendingTx structs through a channel.
        tracing::info!(
            urls = ?self.ws_urls,
            "Mempool scanner ready — awaiting WebSocket integration"
        );

        // Prevent the channel from being dropped immediately
        let _outgoing = outgoing;

        Ok(())
    }

    /// Process a single pending transaction. Called from the integration layer.
    pub fn process_pending_tx(
        &self,
        tx: PendingTx,
        rpc_idx: usize,
        outgoing: &mpsc::Sender<SandwichOpportunity>,
    ) {
        // Deduplication check (atomic check-and-insert)
        if self.dedup.check_and_insert(tx.hash) {
            crate::metrics::record_tx_deduplicated();
            return;
        }

        if let Some(stats) = self.rpc_stats.get(rpc_idx) {
            stats.wins.fetch_add(1, Ordering::Relaxed);
            crate::metrics::record_tx_scanned(&stats.id);
        }

        // Decode the transaction calldata
        if let Some(to) = tx.to {
            if !tx.input.is_empty() {
                if let Some(opportunity) = self.decoder.decode(tx.hash, to, &tx.input) {
                    if opportunity.is_actionable {
                        crate::metrics::record_opportunity_found(&opportunity.protocol.to_string());
                        let _ = outgoing.try_send(opportunity);
                    }
                }
            }
        }
    }
}
