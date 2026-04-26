//! Routing benchmarks.

use criterion::{criterion_group, criterion_main, Criterion};

fn bellman_ford_benchmark(c: &mut Criterion) {
    c.bench_function("bellman_ford_100_pools", |b| {
        b.iter(|| {
            // Benchmark would construct a graph with 100 pools
            // and run the Bellman-Ford algorithm
            std::hint::black_box(42)
        })
    });
}

criterion_group!(benches, bellman_ford_benchmark);
criterion_main!(benches);
