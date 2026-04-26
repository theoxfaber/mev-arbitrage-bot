<div align="center">

# ⚡ MEV Arbitrage Engine

### Rust-Native · revm Simulation · Bellman-Ford Routing · Flashbots MEV-Share

[![CI](https://github.com/theoxfaber/mev-arbitrage-bot/actions/workflows/ci.yml/badge.svg)](https://github.com/theoxfaber/mev-arbitrage-bot/actions)
[![Rust](https://img.shields.io/badge/Rust-1.80+-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Solidity](https://img.shields.io/badge/Solidity-0.8.20-363636.svg)](contracts/)

*A production-grade MEV arbitrage engine for Ethereum, written entirely in Rust. Discovers multi-hop arbitrage opportunities via Bellman-Ford negative-cycle detection, simulates trades locally using revm (zero RPC latency), and executes atomically through Aave V3 flash loans with parallel multi-relay Flashbots bundle submission.*

</div>

---

## Why Rust?

| Metric | TypeScript (typical) | **This Engine (Rust)** |
|---|---|---|
| Mempool → Simulation latency | 50–200ms | **< 1ms** |
| Binary search (64 iterations) | 80ms | **< 0.5ms** |
| Memory usage | ~300MB | **< 30MB** |
| Concurrency model | Event loop | **Lock-free multi-threaded** |
| GC pauses | Unpredictable | **None** |

Most open-source MEV bots are TypeScript or Python — inheriting garbage collection pauses, event-loop blocking, and RPC-bound simulation latency. This engine eliminates all three.

---

## Architecture

```
╔══════════════════════════════════════════════════════════════════════════╗
║                        ENGINE PIPELINE                                  ║
║                                                                          ║
║  ┌───────────────┐   ┌──────────────┐   ┌─────────────┐   ┌──────────┐ ║
║  │   Mempool     │──▶│    Route     │──▶│  Simulator  │──▶│ Flashbots│ ║
║  │   Scanner     │   │  Discovery   │   │   (revm)    │   │  Relayer │ ║
║  │  (WebSocket)  │   │ (Bellman-    │   │   Binary    │   │  Multi-  │ ║
║  └───────────────┘   │   Ford)      │   │   Search    │   │  Relay   │ ║
║  ┌───────────────┐   │              │   │             │   │  + Bid   │ ║
║  │  MEV-Share    │──▶│  Neg-Cycle   │   │  64-step    │   │  Escal.  │ ║
║  │   Scanner     │   │  Detection   │   │  Optimal    │   │          │ ║
║  │   (SSE)       │   │              │   │  Loan Size  │   │          │ ║
║  └───────────────┘   └──────────────┘   └─────────────┘   └──────────┘ ║
║                                                                          ║
║  ┌────────────────────────────────────────────────────────────────────┐  ║
║  │  Prometheus Metrics │ Structured Tracing │ SQLite PnL │ Telegram  │  ║
║  └────────────────────────────────────────────────────────────────────┘  ║
╚══════════════════════════════════════════════════════════════════════════╝
```

## Key Differentiators

### 🔬 Local EVM Simulation via `revm`
No `eth_call` RPC roundtrips. The engine forks chain state locally and runs the entire arbitrage sequence in-process. Binary search for optimal loan sizing completes in **< 0.5ms** — 100x faster than RPC-based simulation.

### 🕸️ Bellman-Ford Graph Routing
Instead of scanning hardcoded pool pairs, the engine maintains a live weighted graph where:
- **Nodes** = tokens
- **Edges** = pool swap rates, weighted as `-log(exchange_rate)`
- **Negative cycles** = arbitrage opportunities

This discovers 2-hop, 3-hop, and 4-hop routes dynamically across any token pair.

### 🔒 Lock-Free Concurrency
- `DashMap` for deduplication (no mutex in the hot path)
- `AtomicU64` for nonce tracking
- `crossbeam-channel` for inter-stage communication
- `parking_lot::Mutex` only for wallet-level serialization

### 📊 Full Observability Stack
- **Prometheus** metrics on every critical code path (scanner, router, simulator, executor)
- **Structured tracing** with JSON output for production log aggregation
- **SQLite** PnL tracking with WAL mode for concurrent reads
- **Telegram** alerts for health monitoring

---

## Modules

```
src/
├── main.rs                 # Engine orchestrator + event loop
├── config.rs               # Env + CLI configuration (clap + dotenvy)
├── types.rs                # Domain types (PoolState, Route, Bundle, etc.)
├── db.rs                   # SQLite PnL tracking (rusqlite, WAL mode)
├── metrics.rs              # Prometheus counters/gauges/histograms
├── scanner/
│   ├── mempool.rs          # Multi-RPC WebSocket mempool scanner
│   ├── decoder.rs          # ABI decoder (UniV3, V2, 1inch, Curve)
│   └── mev_share.rs        # Flashbots MEV-Share SSE stream
├── router/
│   ├── graph.rs            # Bellman-Ford negative-cycle detection
│   ├── pool.rs             # Constant-product + V3 virtual-reserve math
│   └── multicall.rs        # Multicall3 batched on-chain reads
├── simulator/
│   └── evm.rs              # revm-based local simulation + binary search
└── executor/
    ├── bidding.rs           # Dynamic gas-pressure-aware bidding
    ├── bundle.rs            # Bundle construction (Action[] encoding)
    ├── relayer.rs           # Multi-relay submission + bid escalation
    └── wallet.rs            # Nonce-safe wallet pool (per-wallet mutex)

contracts/
└── ArbitrageExecutor.sol   # Aave V3 flash loan atomic executor
```

---

## Quick Start

### Prerequisites
- Rust 1.80+ (`rustup install stable`)
- At least one Ethereum RPC endpoint (Alchemy, Infura, etc.)
- Funded executor wallet(s)
- Deployed `ArbitrageExecutor.sol` contract

### Build
```bash
# Clone
git clone https://github.com/theoxfaber/mev-arbitrage-bot.git
cd mev-arbitrage-bot

# Build (optimized release binary)
cargo build --release

# The binary is at target/release/mev-engine
```

### Configure
```bash
cp .env.example .env
# Edit .env with your RPC URLs, private keys, and contract address
```

### Run
```bash
# Production
./target/release/mev-engine

# Dry run (simulate without submitting)
./target/release/mev-engine --dry-run

# With JSON logging for production log aggregation
./target/release/mev-engine --log-json --log-level info

# With custom metrics port
./target/release/mev-engine --metrics-port 9091
```

### Contract Deployment
```bash
# Using Hardhat (requires Node.js)
npm install
npx hardhat compile
npx hardhat run scripts/deploy.ts --network mainnet
```

---

## Safety Features

| Feature | Description |
|---|---|
| **Circuit Breaker** | Halts execution if rolling 60-min PnL drops below -0.5 ETH |
| **Kill Switch** | Atomic boolean — set `KILL_SWITCH=true` in env for instant halt |
| **Reorg Protection** | Verifies target tx canonical confirmation (>2 blocks) |
| **Nonce Safety** | Per-wallet mutex + background sync prevents nonce collisions |
| **Flash Loan Atomicity** | On-chain contract reverts entirely if trade is unprofitable |
| **ERC-20 Hygiene** | Per-action approve/revoke prevents lingering allowances |
| **Dry Run Mode** | Full simulation pipeline without bundle submission |

---

## Metrics

The engine exposes Prometheus metrics at `http://localhost:9090/metrics`:

```
# Scanner
scanner_txs_received_total{rpc="..."}
scanner_txs_deduplicated_total
scanner_opportunities_found_total{protocol="..."}

# Router
router_pool_count
router_routes_evaluated_total
router_profitable_routes_total{hops="..."}

# Simulator
simulator_duration_ms (histogram)
simulator_executions_total{result="success|failure"}

# Executor
executor_bundles_submitted_total{relay="..."}
executor_bundle_results_total{outcome="..."}
executor_profit_eth (histogram)

# Circuit Breaker
circuit_breaker_rolling_pnl_eth
circuit_breaker_trips_total

# System
engine_uptime_seconds
```

---

## Threat Model

See [ThreatModel.md](ThreatModel.md) for a detailed analysis of operational blind spots and mitigation strategies.

---

## License

MIT — see [LICENSE](LICENSE) for details.
