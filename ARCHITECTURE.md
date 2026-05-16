# System Architecture

## 1. Data Pipeline
- **Scanner**: Connects to multiple RPC providers via WebSockets. Decodes `newPendingTransactions` and emits `SandwichOpportunity` (used here as a generic trade trigger).
- **Router**: Maintains a directed weighted graph of pools. Uses the SCC-based SPFA algorithm to detect negative cycles (arbitrage).
- **Simulator**: Forks the current state using `revm` and executes the candidate arbitrage bundle. It provides exact gas used and net profit.

## 2. Execution Pipeline
- **Bidding Engine**: Computes the optimal miner reward based on projected profit and current network gas pressure.
- **Bundle Builder**: Constructs the EIP-1559 transaction for the `ArbitrageExecutor` contract.
- **Relayer**: Parallel submission to Flashbots, Titan, Beaverbuild, and Rsync. Handles bid escalation by rebuilding and re-signing transactions.

## 3. Storage
- **PnL Database**: SQLite (WAL mode) for tracking every submitted bundle and its outcome.
- **Decimals Cache**: In-memory `DashMap` for fast token metadata lookups.
