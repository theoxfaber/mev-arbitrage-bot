# MEV Engine Architecture

## Overview
The MEV Arbitrage Engine is a high-performance, asynchronous Rust application designed for atomic arbitrage on Ethereum.

## Components

### 1. Mempool Scanner (`src/scanner/mempool.rs`)
- Uses `alloy-provider` with WebSocket subscriptions to monitor `newPendingTransactions`.
- Supports multi-RPC racing with a lock-free deduplication cache to minimize latency.
- Decodes transactions in real-time to identify high-slippage swaps.

### 2. Swap Decoder (`src/scanner/decoder.rs`)
- Manual ABI decoding for Uniswap V2/V3, SushiSwap, Curve, and Balancer.
- Normalizes token amounts to 18 decimals to correctly compute slippage across different tokens (e.g., USDC/WETH).

### 3. Route Discovery (`src/router/graph.rs`)
- Maintains a live graph of liquidity pools.
- Uses Bellman-Ford/SPFA to detect negative cycles (arbitrage opportunities).

### 4. Local Simulator (`src/simulator/evm.rs`)
- Uses `revm` for in-process EVM execution.
- Forks chain state locally to eliminate RPC round-trips during simulation.
- Performs binary search to find the optimal flash loan size.

### 5. Bundle Builder & Signer (`src/executor/bundle.rs`)
- Constructs EIP-1559 transactions using type-safe `sol!` bindings.
- Signs transactions using `alloy-signer` for production-grade security.

### 6. Multi-Relay Relayer (`src/executor/relayer.rs`)
- Submits bundles to multiple Flashbots-compatible relays (Flashbots, Titan, Beaverbuild, rsync).
- Uses EIP-191 authentication signatures to protect relay reputation.
- Implements bid escalation to increase inclusion probability.

### 7. Wallet Pool (`src/executor/wallet.rs`)
- Manages multiple executor wallets.
- Atomic nonce tracking with background chain synchronization.

## Data Flow
1. **Detect**: Mempool scanner finds a high-slippage swap.
2. **Search**: Router finds the most profitable arbitrage cycle.
3. **Optimize**: Simulator finds the best loan size using `revm`.
4. **Sign**: Bundle builder signs the transaction with a managed wallet.
5. **Execute**: Relayer submits the bundle to Flashbots relays.
