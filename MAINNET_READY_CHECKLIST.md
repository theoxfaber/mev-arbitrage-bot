# Mainnet Readiness Checklist

## Phase 1: Environment & Secrets
- [ ] `PRIVATE_KEYS` contains at least one funded EOA (0.1+ ETH recommended).
- [ ] `FLASHBOTS_AUTH_KEY` is set (can be a fresh key).
- [ ] `EXECUTOR_CONTRACT_ADDRESS` matches the address from `forge script`.
- [ ] `ETH_RPC_URL_1` and `ETH_WSS_URL_1` are high-performance mainnet nodes (e.g., Alchemy/Infura/Custom).

## Phase 2: Functional Verification
- [x] Contracts compiled and tested (`forge test`).
- [x] Bot compiled (`cargo build --release`).
- [x] `tests/bug_verification.rs` passes.
- [ ] Running on a server with low latency to Ethereum p2p network.

## Phase 3: Operational Safety
- [ ] `DRY_RUN=true` for the first 24 hours.
- [ ] Circuit breaker window and max loss thresholds verified.
- [ ] Prometheus metrics are being collected and monitored.
- [ ] `pnl.sqlite` is writable and being populated.

## Phase 4: Strategy Validation
- [ ] Check logs for "Opportunity processing failed" — investigate recurring errors.
- [ ] Verify that SPFA is finding cycles in under 5ms.
- [ ] Verify that bundles are reaching the Flashbots relay (check `bundles_submitted` metric).
