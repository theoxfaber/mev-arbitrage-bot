//! Mempool scanning and transaction discovery.
//!
//! This module provides two discovery channels:
//! - **Public mempool** via WebSocket `eth_subscribe("newPendingTransactions")`
//! - **Private order flow** via Flashbots MEV-Share SSE event stream
//!
//! Both channels feed decoded opportunities into a shared async channel.

pub mod decoder;
pub mod event;
pub mod mempool;
pub mod mev_share;

pub use decoder::SwapDecoder;
pub use event::EventScanner;
pub use mempool::MempoolScanner;
pub use mev_share::MevShareScanner;
