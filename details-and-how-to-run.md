# MEV Arbitrage Bot: Architecture & Operational Guide

## Overview
This repository contains a Rust-based MEV arbitrage engine designed for Ethereum mainnet. It implements a high-performance pipeline from mempool scanning to bundle submission via Flashbots.

## System Architecture

### 1. Mempool Scanner (`src/scanner/mempool.rs`)
- **Multi-RPC Racing**: Connects to multiple WebSocket endpoints simultaneously.
- **Deduplication**: Uses a `DashMap` based cache to ensure the same transaction from different RPCs is only processed once.
- **Decoding**: Extracts swap parameters (amountIn, minAmountOut, tokens) from UniswapV2 and UniswapV3 calldata.

### 2. Arbitrage Router (`src/router/graph.rs`)
- **Graph Representation**: Tokens as nodes, pools as edges.
- **SPFA Algorithm**: Faster than Bellman-Ford for finding negative cycles (arbitrage opportunities).
- **Parallelization**: SCC-based parallel cycle detection using `rayon`.

### 3. Simulation Engine (`src/simulator/evm.rs`)
- **REVM Integration**: In-process EVM simulation (currently a skeleton, needs full state pre-fetching).
- **Profitability Analysis**: Estimates gross profit and gas consumption.

### 4. Bidding & Execution (`src/executor/`)
- **Wallet Pool**: Thread-safe management of multiple executor wallets.
- **Bundle Builder**: Constructs EIP-1559 transactions and signs them using `alloy-signer`.
- **Flashbots Relayer**: Submits bundles to multiple builders (Flashbots, Titan, Beaver, etc.) with bid escalation.

## Smart Contract Flow
1. `ArbitrageExecutor.sol` receives a flash loan from Aave V3.
2. It executes a sequence of `Action` calls (swaps) on various DEXs.
3. It verifies that the final balance exceeds the loan + premium + minimum profit.
4. It pays the miner via `block.coinbase.call`.
5. It repays the Aave flash loan.

## Environment Variables
| Variable | Description |
|----------|-------------|
| `ETH_RPC_URL_1` | Primary HTTP RPC URL |
| `ETH_WSS_URL_1` | Primary WebSocket RPC URL |
| `PRIVATE_KEYS` | Comma-separated private keys for execution |
| `FLASHBOTS_AUTH_KEY` | Private key for Flashbots reputation (no funds needed) |
| `EXECUTOR_CONTRACT_ADDRESS` | Address of deployed `ArbitrageExecutor` |
| `MIN_PROFIT_BPS` | Min profit fraction (e.g. 3000 = 30% of gross) |

## Deployment & Startup

### Smart Contract
```bash
cd contracts
forge script script/Deploy.s.sol --rpc-url $ETH_RPC_URL_1 --broadcast
```

### Rust Engine
```bash
cargo run --release
```

## Security Precautions
- **Dry Run**: Always start with `--dry-run` to verify detection without spending gas.
- **Circuit Breaker**: Monitored via SQLite `pnl.sqlite`. Will halt if losses exceed `CIRCUIT_BREAKER_MAX_LOSS_WEI`.
- **Key Safety**: Use a dedicated "hot" wallet with limited funds.
