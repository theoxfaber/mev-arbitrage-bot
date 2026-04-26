use alloy_primitives::{Address, U256};
use mev_arbitrage_bot::simulator::EvmSimulator;
use mev_arbitrage_bot::types::{ArbitrageRoute, PoolState, PoolType, SwapLeg};

#[test]
fn test_evm_simulator_binary_search() {
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

    // Pool 1: WETH -> PEPE
    let p1 = PoolState {
        address: "0x1111111111111111111111111111111111111111"
            .parse()
            .unwrap(),
        pool_type: PoolType::UniswapV2,
        token0: weth,
        token1: pepe,
        reserve0: U256::from(100u64) * U256::from(10u64.pow(18)),
        reserve1: U256::from(100_000_000_000u64) * U256::from(10u64.pow(18)),
        fee_bps_x100: 3000,
        sqrt_price_x96: None,
        liquidity: None,
        tick: None,
        block_number: 1,
    };

    // Pool 2: PEPE -> USDC
    let p2 = PoolState {
        address: "0x2222222222222222222222222222222222222222"
            .parse()
            .unwrap(),
        pool_type: PoolType::UniswapV3,
        token0: usdc,
        token1: pepe,
        reserve0: U256::from(400_000u64) * U256::from(10u64.pow(6)),
        reserve1: U256::from(100_000_000_000u64) * U256::from(10u64.pow(18)),
        fee_bps_x100: 500,
        sqrt_price_x96: None,
        liquidity: None,
        tick: None,
        block_number: 1,
    };

    // Pool 3: USDC -> WETH
    let p3 = PoolState {
        address: "0x3333333333333333333333333333333333333333"
            .parse()
            .unwrap(),
        pool_type: PoolType::UniswapV2,
        token0: weth,
        token1: usdc,
        reserve0: U256::from(110u64) * U256::from(10u64.pow(18)),
        reserve1: U256::from(400_000u64) * U256::from(10u64.pow(6)),
        fee_bps_x100: 3000,
        sqrt_price_x96: None,
        liquidity: None,
        tick: None,
        block_number: 1,
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
        expected_gross_profit: U256::ZERO,
        optimal_loan_size: U256::ZERO,
        confidence: 0.95,
    };

    let result = simulator
        .simulate(&route)
        .expect("Simulation should succeed");

    // Profit must be strictly positive for this mispriced triangle
    assert!(
        result.gross_profit > U256::ZERO,
        "Expected positive gross profit"
    );

    // The optimal loan size shouldn't be 0 or the max bounds
    assert!(
        result.optimal_loan_size > U256::from(10u64.pow(16)),
        "Optimal loan too small"
    ); // > 0.01 WETH
    assert!(
        result.optimal_loan_size < U256::from(100u64) * U256::from(10u64.pow(18)),
        "Optimal loan too large"
    );

    // Gas should be estimated reasonably for a 3-hop swap (base 50k + 3 * 80k)
    assert!(
        result.gas_used > 150_000 && result.gas_used < 500_000,
        "Gas estimation looks wrong"
    );
}
