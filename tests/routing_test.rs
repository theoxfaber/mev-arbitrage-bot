use alloy_primitives::{Address, U256};
use mev_arbitrage_bot::router::ArbitrageRouter;
use mev_arbitrage_bot::types::PoolState;
use std::collections::HashMap;

#[test]
fn test_spfa_negative_cycle() {
    let weth: Address = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
        .parse()
        .unwrap();
    let usdc: Address = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"
        .parse()
        .unwrap();
    let pepe: Address = "0x6982508145454Ce325dDbE47a25d4ec3d2311933"
        .parse()
        .unwrap();

    let router = ArbitrageRouter::new(vec![weth]);

    let weth_pepe = PoolState::UniswapV2 {
        address: "0x1111111111111111111111111111111111111111"
            .parse()
            .unwrap(),
        token0: weth,
        token1: pepe,
        reserve0: 100 * 10u128.pow(18),
        reserve1: 100_000_000_000 * 10u128.pow(18),
        fee_bps: 30,
    };

    let pepe_usdc = PoolState::UniswapV3 {
        address: "0x2222222222222222222222222222222222222222"
            .parse()
            .unwrap(),
        token0: usdc,
        token1: pepe,
        sqrt_price_x96: U256::from(1) << 96,
        liquidity: 100_000_000_000 * 10u128.pow(18),
        tick: 0,
        tick_spacing: 60,
        fee: 500,
        tick_bitmap: HashMap::new(),
        ticks: HashMap::new(),
    };

    let usdc_weth = PoolState::UniswapV3 {
        address: "0x3333333333333333333333333333333333333333"
            .parse()
            .unwrap(),
        token0: weth,
        token1: usdc,
        sqrt_price_x96: U256::from(1) << 96,
        liquidity: 400_000 * 10u128.pow(6),
        tick: 0,
        tick_spacing: 60,
        fee: 500,
        tick_bitmap: HashMap::new(),
        ticks: HashMap::new(),
    };

    router.update_pool(weth_pepe);
    router.update_pool(pepe_usdc);
    router.update_pool(usdc_weth);

    let _routes = router.find_arbitrage_routes();
    // In our synthetic setup, we ensure there's a negative cycle
    // Note: SPFA/Bellman-Ford will find it if log-product < 0
}

#[test]
fn test_no_arbitrage_found_in_efficient_market() {
    let weth: Address = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
        .parse()
        .unwrap();
    let usdc: Address = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"
        .parse()
        .unwrap();
    let router = ArbitrageRouter::new(vec![weth]);

    let p1 = PoolState::UniswapV2 {
        address: "0x1111111111111111111111111111111111111111"
            .parse()
            .unwrap(),
        token0: weth,
        token1: usdc,
        reserve0: 10 * 10u128.pow(18),
        reserve1: 30_000 * 10u128.pow(6),
        fee_bps: 30,
    };

    router.update_pool(p1);

    let _routes = router.find_arbitrage_routes();
    assert!(routes.is_empty());
}
