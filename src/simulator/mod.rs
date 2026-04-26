//! Local EVM simulation using revm.
//!
//! Instead of making RPC `eth_call` requests (which add network latency and
//! are rate-limited), we run a local EVM fork using `revm`. This gives us:
//! - **Zero network latency** for trade simulation
//! - **State overrides** to test with modified pool reserves
//! - **Gas estimation** without RPC roundtrips
//! - **Optimal loan size search** via binary search on the local fork

pub mod evm;

pub use evm::EvmSimulator;
