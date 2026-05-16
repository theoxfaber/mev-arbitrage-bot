use alloy_primitives::{Address, U256};
use mev_arbitrage_bot::simulator::EvmSimulator;
use mev_arbitrage_bot::types::{ArbitrageRoute, PoolState, SwapLeg};
use std::collections::HashMap;
use alloy::providers::ProviderBuilder;

#[tokio::test]
async fn test_evm_simulator_binary_search() {
    let simulator = EvmSimulator::new();

    let weth: Address = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
        .parse()
        .unwrap();
    let usdc: Address = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"
        .parse()
        .unwrap();
    let pepe: Address = "0x6982508145454Ce325dDbE47a25d4ec3d2311933"
        .parse()
        .unwrap();

    // Pool 1: WETH -> PEPE (UniswapV2)
    let p1 = PoolState::UniswapV2 {
        address: "0x1111111111111111111111111111111111111111"
            .parse()
            .unwrap(),
        token0: weth,
        token1: pepe,
        reserve0: 100 * 10u128.pow(18),
        reserve1: 100_000_000_000 * 10u128.pow(18),
        fee_bps: 30,
    };

    // Pool 2: PEPE -> USDC (UniswapV3)
    let p2 = PoolState::UniswapV3 {
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

    // Pool 3: USDC -> WETH (UniswapV2)
    let p3 = PoolState::UniswapV2 {
        address: "0x3333333333333333333333333333333333333333"
            .parse()
            .unwrap(),
        token0: weth,
        token1: usdc,
        reserve0: 110 * 10u128.pow(18),
        reserve1: 400_000 * 10u128.pow(6),
        fee_bps: 30,
    };

    let route = ArbitrageRoute {
        base_token: weth,
        legs: vec![
            SwapLeg {
                pool: p1,
                token_in: weth,
                token_out: pepe,
                amount_in: U256::ZERO,
                expected_amount_out: U256::ZERO,
            },
            SwapLeg {
                pool: p2,
                token_in: pepe,
                token_out: usdc,
                amount_in: U256::ZERO,
                expected_amount_out: U256::ZERO,
            },
            SwapLeg {
                pool: p3,
                token_in: usdc,
                token_out: weth,
                amount_in: U256::ZERO,
                expected_amount_out: U256::ZERO,
            },
        ],
        expected_gross_profit: U256::from(1000u64),
        optimal_loan_size: U256::from(10u128.pow(18)),
        confidence: 0.95,
    };

    let provider = ProviderBuilder::new().on_http("http://localhost:8545".parse().unwrap());

    let result = simulator
        .simulate(&route, &provider, Address::ZERO)
        .await
        .expect("Simulation should succeed");

    assert!(
        result.gross_profit > U256::ZERO,
        "Expected positive gross profit"
    );
}
