//! Flashbots bundle construction with EIP-1559 transaction signing.

use crate::simulator::evm::SimulationResult;
use crate::types::{ArbitrageRoute, FlashbotsBundle};
use alloy::network::{EthereumWallet, TransactionBuilder};
use alloy::rpc::types::eth::TransactionRequest;
use alloy_primitives::{Address, Bytes, U256};
use alloy_sol_types::{SolCall, sol};
use eyre::Result;

/// Builds Flashbots-compatible bundles from simulation results.
pub struct BundleBuilder {
    executor_contract: Address,
}

sol! {
    struct Action {
        address target;
        uint256 value;
        bytes data;
        address approveToken;
        uint256 approveAmount;
    }

    function executeArbitrage(
        address asset,
        uint256 amount,
        uint256 minProfit,
        uint256 minerReward,
        Action[] actions
    );
}

impl BundleBuilder {
    pub fn new(executor_contract: Address) -> Self {
        Self { executor_contract }
    }

    pub async fn build_and_sign(
        &self,
        route: &ArbitrageRoute,
        sim: &SimulationResult,
        target_tx_hash: alloy_primitives::TxHash,
        target_block: u64,
        miner_reward: U256,
        min_profit: U256,
        wallet: &EthereumWallet,
        nonce: u64,
        chain_id: u64,
        base_fee: U256,
    ) -> Result<FlashbotsBundle> {
        let actions: Vec<Action> = route.legs.iter().enumerate().map(|(i, leg)| {
            let data = match leg.pool {
                crate::types::PoolState::UniswapV2 { .. } => {
                    // swap(uint256,uint256,address,bytes)
                    let (amt0, amt1) = if leg.token_in < leg.token_out {
                        (U256::ZERO, leg.expected_amount_out)
                    } else {
                        (leg.expected_amount_out, U256::ZERO)
                    };
                    let selector = hex::decode("022c0d9f").unwrap();
                    let mut payload = selector;
                    payload.extend(alloy_primitives::FixedBytes::<32>::from(amt0));
                    payload.extend(alloy_primitives::FixedBytes::<32>::from(amt1));
                    payload.extend(Address::repeat_byte(0xEE).into_word()); // Placeholder
                    payload.extend(alloy_primitives::FixedBytes::<32>::from(U256::from(128))); // Offset
                    payload.extend(alloy_primitives::FixedBytes::<32>::from(U256::ZERO)); // Data len
                    payload
                }
                _ => vec![],
            };

            Action {
                target: leg.pool.address(),
                value: U256::ZERO,
                data: Bytes::from(data),
                approveToken: leg.token_in,
                approveAmount: if i == 0 { sim.optimal_loan_size } else { U256::ZERO },
            }
        }).collect();

        let call = executeArbitrageCall {
            asset: route.base_token,
            amount: sim.optimal_loan_size,
            minProfit: min_profit,
            minerReward: miner_reward,
            actions,
        };

        let calldata = call.abi_encode();

        let max_priority_fee = 0u128;
        let max_fee = (base_fee * U256::from(2)).to::<u128>();

        let tx = TransactionRequest::default()
            .with_to(self.executor_contract)
            .with_input(calldata)
            .with_nonce(nonce)
            .with_chain_id(chain_id)
            .with_gas_limit(sim.gas_used + 50_000)
            .with_max_fee_per_gas(max_fee)
            .with_max_priority_fee_per_gas(max_priority_fee);

        let signed = tx.build(wallet).await?;
        let signed_tx_bytes = alloy::eips::eip2718::Encodable2718::encoded_2718(&signed);

        let expected_net = if sim.gross_profit > miner_reward {
            sim.gross_profit - miner_reward
        } else {
            U256::ZERO
        };

        Ok(FlashbotsBundle {
            target_tx_hash,
            signed_txs: vec![Bytes::from(signed_tx_bytes)],
            target_block,
            miner_reward,
            expected_net_profit: expected_net,
        })
    }
}
