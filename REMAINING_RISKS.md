# Remaining Risks & Future Work

## Fixed in v3.1 (see FIXES_APPLIED.md)

- Simulator EmptyDB → math search; contract double-count → `+premium`;
  decoder 1:1 price → size-based; bundle V2-only placeholder → V2+V3;
  hardcoded block/fee → live RPC; auth `sign_hash` → `sign_message`;
  Dockerfile binary; case-colliding docs; hanging chaos test.

## Honest residuals

1. **State drift (Medium)**: math sim uses cached pool states, not a
   fork at `target_block`. Reverts are safe (atomic) but cost reputation.
   Next: AlloyDB fork + `eth_call` preflight.
2. **Coverage (Medium)**: Curve/Balancer simulate but cannot execute;
   Universal Router only coarsely flagged. Next: per-protocol calldata.
3. **Private flow (Medium)**: 40-50% flow is private (UniswapX, CoW,
   builder deals). MEV-Share is wired but orderflow deals are not.
4. **Builder trust (Low-Med)**: multi-relay improves inclusion but
   requires reputable builders. Keep Flashbots+Titan+Beaver+rsync only.
5. **Gas (Low)**: L2 `estimate_effective_gas_price` uses 0.5 gwei L1
   cushion, not a real L1 fee oracle.

Do not deploy more than you can lose. `DRY_RUN=true` first.
