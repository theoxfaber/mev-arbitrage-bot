//! Routing benchmarks — real SPFA over a synthetic 100-pool graph.

use alloy_primitives::{Address, U256};
use criterion::{criterion_group, criterion_main, Criterion};
use mev_arbitrage_bot::router::ArbitrageRouter;
use mev_arbitrage_bot::types::PoolState;
use std::collections::HashMap;

fn build_router(pools: usize) -> ArbitrageRouter {
    let weth: Address = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
        .parse()
        .unwrap();
    let router = ArbitrageRouter::new(vec![weth]);
    for i in 0..pools {
        let a = Address::repeat_byte((i % 250) as u8 + 1);
        let b = Address::repeat_byte(((i + 1) % 250) as u8 + 1);
        router.update_pool(PoolState::UniswapV2 {
            address: Address::repeat_byte((i % 250) as u8 + 10),
            token0: a,
            token1: b,
            reserve0: 100 * 10u128.pow(18),
            reserve1: 100 * 10u128.pow(18),
            fee_bps: 30,
        });
        let _ = (weth, U256::ZERO, HashMap::<i32, u8>::new());
    }
    router
}

fn spfa_benchmark(c: &mut Criterion) {
    let router = build_router(100);
    c.bench_function("spfa_100_pools", |b| {
        b.iter(|| {
            let routes = router.find_arbitrage_routes();
            std::hint::black_box(routes.len())
        })
    });
}

criterion_group!(benches, spfa_benchmark);
criterion_main!(benches);
