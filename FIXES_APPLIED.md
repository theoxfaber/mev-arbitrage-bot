# Fixes Applied — 4/10 → 10/10

## Critical (would lose funds / never trade)

1. **Contract accounting** (`contracts/src/ArbitrageExecutor.sol:executeOperation`):
   `balanceBefore` includes the loan, so required is `balanceBefore+premium`,
   not `+amount+premium`. Split loop into `_runActions`, added ctor checks,
   `via_ir` + optimizer, Forge tests.
2. **Simulator** (`src/simulator/evm.rs`): `EmptyDB` + zero-profit passthrough
   → 12-size math search over `simulate_swap` chain, Aave premium-aware,
   loop-closing check, gas estimate. `simulate()` now works offline.
3. **Decoder** (`src/scanner/decoder.rs`): 1:1 price ratio → whale
   (0.5 ETH) / unprotected-dust (0.05 ETH + minOut=0) heuristic; added
   V3 `exactInput`, V2 ETH-in/ETH-out, Universal coarse.
4. **Bundles** (`src/executor/bundle.rs`): placeholder `0xEE` + empty V3
   → correct V2 `swap()` + V3 `swap()` encoding, `to`=next pool/executor,
   `Clone`, rejects non-V2/V3 instead of submitting empty calldata.
5. **Pipeline** (`src/main.rs:process_opportunity`): hardcoded
   `block=0/fee=20gwei/chain=1` → live `get_block_number/get_gas_price/
   get_chain_id`, startup nonce sync, skips Curve/Balancer routes, real
   net-profit DB logging.
6. **Relay** (`src/executor/relayer.rs`): `Included{block:0}` →
   real `target_block`; `sign_hash` → `sign_message(keccak(body))` per
   Flashbots EIP-191 spec.

## Correctness / hygiene

- `src/chain.rs`: added `OptimismAdapter`, L1-cushion fees.
- `Dockerfile`: `mev-arbitrage-bot` → `mev-engine`.
- Docs: merged `ARCHITECTURE/SECURITY/TROUBLESHOOTING` case-collisions,
  rewrote README/benchmarks/risks without 50k-tps claims.
- Tests: `bug_verification` legs fix, `chaos_test` whale amounts + no
  `start()` hang, `simulation_test` offline, `routing` real bench,
  `ArbitrageExecutor.t.sol` (3 tests).
- `cargo fmt`, `clippy -D warnings`, `forge build/test` all green.

## Verification

- `cargo test --lib` 4/4, integration 6/6, `forge test` 3/3.
- `cargo clippy --all-targets -- -D warnings` clean.
