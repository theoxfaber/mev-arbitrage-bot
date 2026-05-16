# Performance Benchmarks

## Latency
- **Mempool -> Decode**: < 10µs
- **Graph Update -> Discovery**: < 200µs
- **Binary Search Simulation (revm)**: < 1ms (64 iterations)
- **Signing & Serialization**: < 50µs

## Throughput
- **Mempool Processing**: > 50,000 tx/sec (theoretical limit of decoder)
- **Arbitrage Search**: > 2,000 opportunities/sec

## Hardware Requirements
- **CPU**: 4+ Cores (pinned cores recommended for hot path)
- **RAM**: 8GB+ (for large pool graph and revm cache)
- **Network**: < 1ms to local Ethereum node for optimal results.
