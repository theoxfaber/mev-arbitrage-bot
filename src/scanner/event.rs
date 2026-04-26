//! On-chain log monitoring for advanced order flow capture.
//!
//! Monitors:
//! 1. AMM Swap logs (real-time pool updates).
//! 2. Intent protocol events (UniswapX, CoW Protocol).

use crate::types::SandwichOpportunity;
use alloy_primitives::{Address, B256};
use tokio::sync::mpsc;
use tracing::info;

/// Monitor on-chain logs for arbitrage signals.
pub struct EventScanner {
    #[allow(dead_code)]
    opportunity_tx: mpsc::Sender<SandwichOpportunity>,
}

impl EventScanner {
    pub fn new(opportunity_tx: mpsc::Sender<SandwichOpportunity>) -> Self {
        Self { opportunity_tx }
    }

    /// Process a new log entry.
    pub async fn process_log(&self, log_address: Address, topics: Vec<B256>, data: Vec<u8>) {
        // Example: Uniswap V3 Swap(sender, recipient, amount0, amount1, sqrtPriceX96, liquidity, tick)
        // Topic0: 0xc42079f94a6350d7e6235f29174924f928cc2ac818eb64fed8004e115fbcca67
        if topics.is_empty() {
            return;
        }

        let event_signature = topics[0];

        if event_signature
            == B256::from_slice(
                &hex::decode("c42079f94a6350d7e6235f29174924f928cc2ac818eb64fed8004e115fbcca67")
                    .unwrap(),
            )
        {
            self.handle_v3_swap(log_address, &topics, &data).await;
        } else if event_signature
            == B256::from_slice(
                &hex::decode("d78ad95fa46c994b6551d0da85fc275fe613ce37657fb8d5e3d130840159d822")
                    .unwrap(),
            )
        {
            self.handle_v2_swap(log_address, &topics, &data).await;
        }
    }

    async fn handle_v3_swap(&self, address: Address, _topics: &[B256], _data: &[u8]) {
        info!("Detected V3 Swap on-chain at {:?}", address);
        // Trigger a re-calculation of any routes containing this pool
    }

    async fn handle_v2_swap(&self, address: Address, _topics: &[B256], _data: &[u8]) {
        info!("Detected V2 Swap on-chain at {:?}", address);
    }
}
