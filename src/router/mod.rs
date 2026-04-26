//! Multi-hop arbitrage routing via graph-based algorithms.
//!
//! This module maintains a live graph of DEX pools where:
//! - **Nodes** = tokens
//! - **Edges** = pools connecting two tokens, weighted by `-log(exchange_rate)`
//!
//! A negative-weight cycle in this graph corresponds to an arbitrage opportunity.
//! We detect these using a modified **Bellman-Ford algorithm** and extract the
//! optimal route.

pub mod graph;
pub mod multicall;
pub mod pool;

pub use graph::ArbitrageRouter;
