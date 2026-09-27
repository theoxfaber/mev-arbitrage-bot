# Performance Benchmarks

Measured with `cargo bench` (Criterion) and `cargo test`, not projections.

## Method

- `benches/routing.rs:spfa_100_pools` builds 100 synthetic V2 pools and
  runs `ArbitrageRouter::find_arbitrage_routes`. Run:
  `cargo bench --bench routing`.
- Simulation latency measured in `SimulationResult::simulation_duration_ms`
  (12 candidate loan sizes × ≤5 hops of pure `U256` math, typically <1ms).

## Guidance (not guarantees)

- SPFA dominates at O(k·E) per SCC; SCC split + rayon keeps 100-pool
  graphs in low-ms on 4+ cores.
- Mempool dedup is DashMap `check_and_insert` (<10µs per hash by design,
  verify with `chaos_test`).
- End-to-end tx→bundle is simulation-bound; keep `MAX_HOPS=5`.

## Hardware

4+ cores, 8GB+ RAM, <50ms to RPC. Co-locate WS endpoint for best results.
