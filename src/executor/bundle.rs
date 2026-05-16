//! Flashbots bundle construction from simulation results.

use crate::simulator::evm::SimulationResult;
use crate::types::{ArbitrageRoute, FlashbotsBundle};
use alloy_primitives::{Address, Bytes, U256};

/// Builds Flashbots-compatible bundles from simulation results.
pub struct BundleBuilder {
    #[allow(dead_code)]
    executor_contract: Address,
}

impl BundleBuilder {
    pub fn new(executor_contract: Address) -> Self {
        Self { executor_contract }
    }

    /// Construct a Flashbots bundle from a simulated route.
    pub fn build(
        &self,
        route: &ArbitrageRoute,
        sim: &SimulationResult,
        target_tx_hash: alloy_primitives::TxHash,
        target_block: u64,
        miner_reward: U256,
        min_profit: U256,
    ) -> FlashbotsBundle {
        // Build the action sequence for the on-chain executor
        // Each leg becomes an executeArbitrage Action with:
        // - target: swap router address
        // - data: encoded swap calldata
        // - approveToken: the input token for this leg
        // - approveAmount: the input amount
        let mut calldata_parts: Vec<u8> = Vec::new();

        // ABI-encode executeArbitrage(address,uint256,uint256,uint256,Action[])
        // Function selector for executeArbitrage(address,uint256,uint256,uint256,(address,uint256,bytes,address,uint256)[])
        // Note: This is still a partial implementation for demonstration.
        calldata_parts.extend_from_slice(&[0x54, 0xc1, 0x3d, 0x76]);

        // Encode asset (base token)
        let mut asset_bytes = [0u8; 32];
        asset_bytes[12..32].copy_from_slice(route.base_token.as_slice());
        calldata_parts.extend_from_slice(&asset_bytes);

        // Encode amount (optimal loan size)
        calldata_parts.extend_from_slice(&sim.optimal_loan_size.to_be_bytes::<32>());

        // Encode minProfit
        calldata_parts.extend_from_slice(&min_profit.to_be_bytes::<32>());

        // Encode minerReward
        calldata_parts.extend_from_slice(&miner_reward.to_be_bytes::<32>());

        let expected_net = if sim.gross_profit > miner_reward {
            sim.gross_profit - miner_reward
        } else {
            U256::ZERO
        };

        FlashbotsBundle {
            target_tx_hash,
            signed_txs: vec![Bytes::from(calldata_parts)],
            target_block,
            miner_reward,
            expected_net_profit: expected_net,
        }
    }
}
