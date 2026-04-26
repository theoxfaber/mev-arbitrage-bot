use alloy_primitives::{Address, U256};
use mev_arbitrage_bot::router::ArbitrageRouter;
use mev_arbitrage_bot::types::{PoolState, PoolType};

#[test]
fn test_bellman_ford_negative_cycle() {
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

    // Construct a synthetic triangular arbitrage opportunity:
    // WETH -> PEPE -> USDC -> WETH
    // 1 WETH = 1,000,000,000 PEPE
    // 1,000,000,000 PEPE = 4,000 USDC
    // 4,000 USDC = 1.1 WETH (Profit!)

    // WETH / PEPE Pool (UniswapV2)
    let weth_pepe = PoolState {
        address: "0x1111111111111111111111111111111111111111"
            .parse()
            .unwrap(),
        pool_type: PoolType::UniswapV2,
        token0: weth,
        token1: pepe,
        reserve0: U256::from(100u64) * U256::from(10u64.pow(18)), // 100 WETH
        reserve1: U256::from(100_000_000_000u64) * U256::from(10u64.pow(18)), // 100B PEPE
        fee_bps_x100: 3000,
        sqrt_price_x96: None,
        liquidity: None,
        tick: None,
        block_number: 1,
    };

    // PEPE / USDC Pool (UniswapV3)
    // 1 PEPE = 0.000004 USDC => 100B PEPE = 400,000 USDC
    let pepe_usdc = PoolState {
        address: "0x2222222222222222222222222222222222222222"
            .parse()
            .unwrap(),
        pool_type: PoolType::UniswapV3,
        token0: usdc, // usually lower address is token0
        token1: pepe,
        reserve0: U256::from(400_000u64) * U256::from(10u64.pow(6)), // 400k USDC
        reserve1: U256::from(100_000_000_000u64) * U256::from(10u64.pow(18)), // 100B PEPE
        fee_bps_x100: 500,
        sqrt_price_x96: None, // Simplified for the test mock
        liquidity: None,
        tick: None,
        block_number: 1,
    };

    // USDC / WETH Pool (UniswapV3)
    // 4,000 USDC = 1.1 WETH (mispriced!)
    // True rate should be 4000 USDC = 1 WETH. But here 400k USDC = 110 WETH.
    let usdc_weth = PoolState {
        address: "0x3333333333333333333333333333333333333333"
            .parse()
            .unwrap(),
        pool_type: PoolType::UniswapV3,
        token0: weth,
        token1: usdc,
        reserve0: U256::from(110u64) * U256::from(10u64.pow(18)), // 110 WETH
        reserve1: U256::from(400_000u64) * U256::from(10u64.pow(6)), // 400k USDC
        fee_bps_x100: 500,
        sqrt_price_x96: None,
        liquidity: None,
        tick: None,
        block_number: 1,
    };

    router.update_pool(weth_pepe);
    router.update_pool(pepe_usdc);
    router.update_pool(usdc_weth);

    // Ensure routes are found
    let routes = router.find_arbitrage_routes();
    assert!(
        !routes.is_empty(),
        "Bellman-Ford failed to find negative cycle"
    );

    // Check route properties
    let best_route = &routes[0];
    assert_eq!(best_route.num_hops(), 3, "Expected a 3-hop route");

    // Legs should form a continuous cycle
    assert_eq!(best_route.legs[0].token_out, best_route.legs[1].token_in);
    assert_eq!(best_route.legs[1].token_out, best_route.legs[2].token_in);
    assert_eq!(best_route.legs[2].token_out, best_route.legs[0].token_in);

    // Base token must be part of the cycle
    let base_in_cycle = best_route
        .legs
        .iter()
        .any(|leg| leg.token_in == best_route.base_token);
    assert!(base_in_cycle, "Base token must be part of the cycle");
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

    // Perfectly efficient pools
    let p1 = PoolState {
        address: "0x1111111111111111111111111111111111111111"
            .parse()
            .unwrap(),
        pool_type: PoolType::UniswapV2,
        token0: weth,
        token1: usdc,
        reserve0: U256::from(10u64) * U256::from(10u64.pow(18)),
        reserve1: U256::from(30_000u64) * U256::from(10u64.pow(6)),
        fee_bps_x100: 3000,
        sqrt_price_x96: None,
        liquidity: None,
        tick: None,
        block_number: 1,
    };

    let p2 = PoolState {
        address: "0x2222222222222222222222222222222222222222"
            .parse()
            .unwrap(),
        pool_type: PoolType::UniswapV3,
        token0: weth,
        token1: usdc,
        reserve0: U256::from(10u64) * U256::from(10u64.pow(18)),
        reserve1: U256::from(30_000u64) * U256::from(10u64.pow(6)),
        fee_bps_x100: 500,
        sqrt_price_x96: None,
        liquidity: None,
        tick: None,
        block_number: 1,
    };

    router.update_pool(p1);
    router.update_pool(p2);

    let routes = router.find_arbitrage_routes();
    assert!(
        routes.is_empty(),
        "Found arbitrage in an efficient market (fees should prevent it)"
    );
}
