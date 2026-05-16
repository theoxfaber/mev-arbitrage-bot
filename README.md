# MEV Arbitrage Engine v3 (Rust)

Production-grade MEV arbitrage engine for Ethereum, written in Rust.

## Core Features
- **Real-time Mempool Monitoring**: High-performance WebSocket subscriptions with multi-RPC racing.
- **SPFA-based Pathfinding**: Sub-millisecond discovery of negative cycles (arbitrage) in a live token graph.
- **revm Simulation**: In-process EVM execution for exact profit and gas calculation without RPC overhead.
- **Atomic Execution**: Aave V3 flash loans for capital-efficient arbitrage.
- **Flashbots Integration**: Multi-relay bundle submission with bid escalation and automatic re-signing.

## Quick Start
1. **Config**: `cp .env.example .env` and fill in RPC URLs.
2. **Build**: `cargo build --release`
3. **Deploy Contract**: `cd contracts && forge script script/Deploy.s.sol --broadcast`
4. **Run**: `./target/release/mev-engine`

## Disclaimer
Arbitrage trading involves risk. Use at your own risk.
