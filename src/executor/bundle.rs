//! Flashbots bundle construction with EIP-1559 transaction signing.

use crate::simulator::evm::SimulationResult;
use crate::types::{ArbitrageRoute, FlashbotsBundle};
use alloy::network::{EthereumWallet, TransactionBuilder};
use alloy::rpc::types::eth::TransactionRequest;
use alloy_primitives::{Address, Bytes, U256};
use alloy_sol_types::{sol, SolCall};
use eyre::{eyre, Result};

/// Builds Flashbots-compatible bundles from simulation results.
#[derive(Clone)]
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

// UniswapV3 pool swap limits.
const V3_MIN_SQRT: u128 = 4295128739 + 1;

impl BundleBuilder {
    pub fn new(executor_contract: Address) -> Self {
        Self { executor_contract }
    }

    #[allow(clippy::too_many_arguments)]
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
        if route.legs.is_empty() {
            return Err(eyre!("empty route"));
        }
        if sim.optimized_legs.len() != route.legs.len() {
            return Err(eyre!("sim legs mismatch route legs"));
        }
        if target_block == 0 {
            return Err(eyre!("target_block must be set (got 0)"));
        }

        let mut actions: Vec<Action> = Vec::with_capacity(route.legs.len());
        for (i, leg) in route.legs.iter().enumerate() {
            let opt = &sim.optimized_legs[i];
            let next_to = if i + 1 < route.legs.len() {
                route.legs[i + 1].pool.address()
            } else {
                // Last hop returns funds to the executor for flashloan repayment.
                self.executor_contract
            };
            let data = Self::encode_leg(leg, opt.amount_in, opt.amount_out, next_to)?;

            actions.push(Action {
                target: leg.pool.address(),
                value: U256::ZERO,
                data: Bytes::from(data),
                approveToken: leg.token_in,
                approveAmount: if i == 0 {
                    sim.optimal_loan_size
                } else {
                    U256::ZERO
                },
            });
        }

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

    fn encode_leg(
        leg: &crate::types::SwapLeg,
        amount_in: U256,
        amount_out: U256,
        to: Address,
    ) -> Result<Vec<u8>> {
        match &leg.pool {
            crate::types::PoolState::UniswapV2 { .. } => {
                // swap(uint amount0Out, uint amount1Out, address to, bytes data)
                let token0 = leg.pool.token0();
                let (amt0, amt1) = if leg.token_out == token0 {
                    (amount_out, U256::ZERO)
                } else {
                    (U256::ZERO, amount_out)
                };
                let mut payload = hex::decode("022c0d9f").unwrap();
                payload.extend(alloy_primitives::FixedBytes::<32>::from(amt0).as_slice());
                payload.extend(alloy_primitives::FixedBytes::<32>::from(amt1).as_slice());
                payload.extend(to.into_word().as_slice());
                payload
                    .extend(alloy_primitives::FixedBytes::<32>::from(U256::from(128)).as_slice());
                payload.extend(alloy_primitives::FixedBytes::<32>::from(U256::ZERO).as_slice());
                Ok(payload)
            }
            crate::types::PoolState::UniswapV3 { .. } => {
                // swap(address recipient, bool zeroForOne, int256 amountSpecified, uint160 sqrtPriceLimitX96, bytes data)
                let zero_for_one = leg.token_in == leg.pool.token0();
                let limit = if zero_for_one {
                    U256::from(V3_MIN_SQRT)
                } else {
                    // 2^160 - 2 as a safe max
                    U256::from_str_radix(
                        "fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffb",
                        16,
                    )
                    .unwrap_or(U256::MAX)
                };
                let mut payload = hex::decode("128acb08").unwrap();
                payload.extend(to.into_word().as_slice());
                payload.extend(
                    alloy_primitives::FixedBytes::<32>::from(U256::from(if zero_for_one {
                        1u64
                    } else {
                        0u64
                    }))
                    .as_slice(),
                );
                // amountSpecified positive = exact input
                payload.extend(alloy_primitives::FixedBytes::<32>::from(amount_in).as_slice());
                payload.extend(alloy_primitives::FixedBytes::<32>::from(limit).as_slice());
                payload.extend(
                    alloy_primitives::FixedBytes::<32>::from(U256::from(160u64)).as_slice(),
                );
                payload.extend(alloy_primitives::FixedBytes::<32>::from(U256::ZERO).as_slice());
                Ok(payload)
            }
            _ => Err(eyre!(
                "unsupported pool type for calldata (only V2/V3); pool={:?}",
                leg.pool.address()
            )),
        }
    }
}
