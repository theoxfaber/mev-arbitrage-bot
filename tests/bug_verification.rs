use alloy_primitives::{Address, Bytes, TxHash, U256, hex};
use mev_arbitrage_bot::scanner::decoder::{new_decimals_cache, SwapDecoder};
use mev_arbitrage_bot::executor::BundleBuilder;
use mev_arbitrage_bot::types::{ArbitrageRoute, SwapLeg, PoolState};
use mev_arbitrage_bot::simulator::evm::SimulationResult;
use alloy::network::EthereumWallet;
use alloy::signers::local::PrivateKeySigner;

#[tokio::test]
async fn test_uniswap_v3_decoder_offsets() {
    let decimals = new_decimals_cache();
    let decoder = SwapDecoder::new(decimals);

    // exactInputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160))
    // selector: 414bf389
    // offset to struct: 0000000000000000000000000000000000000000000000000000000000000020
    // tokenIn: ...
    let data = hex::decode("414bf3890000000000000000000000000000000000000000000000000000000000000020000000000000000000000000c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2000000000000000000000000a0b86991c6218b36c1d19d4a2e9eb0ce3606eb4800000000000000000000000000000000000000000000000000000000000001f400000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000de0b6b3a7640000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").unwrap();

    let result = decoder.decode(TxHash::ZERO, Address::ZERO, &Bytes::from(data));
    assert!(result.is_some(), "Decoder failed to decode UniswapV3 exactInputSingle");
    let opp = result.unwrap();
    assert_eq!(opp.amount_in, U256::from(10u128.pow(18)));
}

#[tokio::test]
async fn test_bundle_builder_calldata_generation() {
    let builder = BundleBuilder::new(Address::ZERO);
    let route = ArbitrageRoute {
        base_token: Address::ZERO,
        legs: vec![
            SwapLeg {
                pool: PoolState::UniswapV2 {
                    address: Address::repeat_byte(1),
                    token0: Address::ZERO,
                    token1: Address::repeat_byte(2),
                    reserve0: 100,
                    reserve1: 100,
                    fee_bps: 30,
                },
                token_in: Address::ZERO,
                token_out: Address::repeat_byte(2),
                amount_in: U256::ZERO,
                expected_amount_out: U256::ZERO,
            }
        ],
        expected_gross_profit: U256::ZERO,
        optimal_loan_size: U256::from(1000),
        confidence: 1.0,
    };

    let sim = SimulationResult {
        optimal_loan_size: U256::from(1000),
        gross_profit: U256::ZERO,
        gas_used: 100_000,
        optimized_legs: vec![],
        simulation_duration_ms: 0.0,
    };

    let signer: PrivateKeySigner = "0000000000000000000000000000000000000000000000000000000000000001".parse().unwrap();
    let wallet = EthereumWallet::from(signer);

    let bundle = builder.build_and_sign(
        &route,
        &sim,
        TxHash::ZERO,
        1,
        U256::ZERO,
        U256::ZERO,
        &wallet,
        0,
        1,
        U256::from(20_000_000_000u64),
    ).await.unwrap();

    assert!(!bundle.signed_txs.is_empty());
    let signed_tx = &bundle.signed_txs[0];
    assert!(signed_tx.len() > 100); // Should contain some calldata
}
