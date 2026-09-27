# MEV Arbitrage Engine v3.1 (Rust)

Ethereum arbitrage searcher: mempool → SPFA cycle detection → math-based
simulation → Aave V3 flash-loan execution → multi-relay Flashbots submission.

## Status

Functional reference searcher. All 10 Rust tests + 3 Forge tests pass,
`clippy -D warnings` clean, `forge build` clean. See `FIXES_APPLIED.md`
for what changed from the 4/10 prototype.

## What it does

- **Mempool**: WS `newPendingTransactions` with multi-RPC dedup
  (`src/scanner/mempool.rs`), size-based triggering for V2/V3/Universal
  Router (`src/scanner/decoder.rs`), optional MEV-Share SSE.
- **Routing**: SCC + parallel SPFA, max 5 hops (`src/router/graph.rs`);
  exact V2/V3/Curve/Balancer math (`src/router/pool.rs`).
- **Simulation**: offline multi-size search over exact pool math, net of
  Aave 0.05% premium (`src/simulator/evm.rs`). No RPC roundtrip.
- **Execution**: `ArbitrageExecutor.sol` (Aave V3 `flashLoanSimple`,
  `nonReentrant`, `onlyOwner`, correct `balanceBefore+premium` accounting).
- **Submission**: EIP-191 auth (`signMessage(keccak(body))`), 4 relays,
  escalation with re-sign (`src/executor/relayer.rs`).

## Quick Start

1. `cp .env.example .env` — fill `ETH_RPC_URL_1`, `ETH_WSS_URL_1`, `PRIVATE_KEYS`.
2. `cargo build --release`
3. `cd contracts && forge test && forge script script/Deploy.s.sol --broadcast`
4. `./target/release/mev-engine` (start with `DRY_RUN=true` 24h).

## Limits (honest)

- Execution supports V2/V3 only; Curve/Balancer simulate but are skipped
  for submission (`KNOWN_LIMITATIONS.md`).
- Decoder triggers on size + no-protection, not oracle slippage.
- `subscribe_full_pending_transactions` needs a premium node.
- See `REMAINING_RISKS.md` before funding.

## Disclaimer

Arbitrage involves risk. Audit the contract yourself. Start small.
