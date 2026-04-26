use alloy_primitives::{Address, Bytes, TxHash};
use mev_arbitrage_bot::scanner::decoder::new_decimals_cache;
use mev_arbitrage_bot::scanner::mempool::PendingTx;
use mev_arbitrage_bot::scanner::MempoolScanner;
use mev_arbitrage_bot::types::SandwichOpportunity;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_mempool_scanner_deduplication_under_chaos() {
    let decimals = new_decimals_cache();
    let urls = vec![
        "wss://rpc1.test".to_string(),
        "wss://rpc2.test".to_string(),
        "wss://rpc3.test".to_string(),
    ];
    let scanner = Arc::new(MempoolScanner::new(urls, decimals));
    let (tx, mut rx) = mpsc::channel::<SandwichOpportunity>(1000);

    // Start scanner background tasks (stats/dedup maintenance)
    scanner.start(tx.clone()).await.unwrap();

    // Create 10,000 "duplicate" transactions across 3 simulated RPCs concurrently
    let tx_count = 10_000;
    let mut handles = vec![];

    for rpc_idx in 0..3 {
        let scanner_clone = Arc::clone(&scanner);
        let tx_clone = tx.clone();

        handles.push(task::spawn(async move {
            for i in 0..tx_count {
                // Same transaction hashes sent by multiple RPCs
                let mut hash_bytes = [0u8; 32];
                let val_bytes = (i as u64).to_be_bytes();
                hash_bytes[24..32].copy_from_slice(&val_bytes);
                let tx_hash = TxHash::from(hash_bytes);

                // Construct a mock exactInputSingle calldata to trigger the decoder occasionally
                // (every 1000th tx is a valid UniswapV3 swap)
                let input = if i % 1000 == 0 {
                    // Valid 4-byte selector + 224 bytes of data
                    let mut data = vec![0x41, 0x4b, 0xf3, 0x89];
                    data.resize(4 + 8 * 32, 0);
                    Bytes::from(data)
                } else {
                    Bytes::from(vec![0x00; 10]) // Invalid / ignored
                };

                let pending_tx = PendingTx {
                    hash: tx_hash,
                    to: Some(Address::ZERO),
                    input,
                };

                // Simulate slight network jitter
                if i % 3 == 0 {
                    tokio::task::yield_now().await;
                }

                scanner_clone.process_pending_tx(pending_tx, rpc_idx, &tx_clone);
            }
        }));
    }

    // Wait for all producers to finish
    for handle in handles {
        handle.await.unwrap();
    }

    drop(tx); // Close the channel

    // Collect all valid opportunities emitted
    let mut opportunities = 0;
    while rx.recv().await.is_some() {
        opportunities += 1;
    }

    // Out of 10,000 unique tx hashes, 10 are valid swaps (0, 1000, 2000... 9000).
    // The decoder will parse them, but with all 0 bytes, slippage is 100%,
    // so `is_actionable` will be true.
    // Because of deduplication, we should get exactly 10 opportunities out,
    // not 30 (since 3 RPCs raced to deliver them).
    assert_eq!(
        opportunities, 10,
        "Deduplication failed to prevent duplicate opportunities"
    );
}
