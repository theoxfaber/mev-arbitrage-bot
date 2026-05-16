//! revm-based local EVM simulator for trade profitability analysis.

use crate::types::ArbitrageRoute;
use alloy::providers::Provider;
use alloy::network::Network;
use alloy::transports::Transport;
use alloy_primitives::{Address, U256, Bytes};
use eyre::Result;
use revm::{
    db::{CacheDB, EmptyDB},
    primitives::{AccountInfo, Env, TransactTo, ExecutionResult, U256 as rU256},
    Evm,
};
use std::time::Instant;
use std::sync::Arc;

/// Result of a successful simulation.
#[derive(Debug, Clone)]
pub struct SimulationResult {
    pub optimal_loan_size: U256,
    pub gross_profit: U256,
    pub gas_used: u64,
    pub optimized_legs: Vec<OptimizedLeg>,
    pub simulation_duration_ms: f64,
}

#[derive(Debug, Clone)]
pub struct OptimizedLeg {
    pub pool_address: Address,
    pub token_in: Address,
    pub token_out: Address,
    pub amount_in: U256,
    pub amount_out: U256,
}

pub struct EvmSimulator {
    pub flash_loan_premium_bps: u32,
}

impl Default for EvmSimulator {
    fn default() -> Self {
        Self::new()
    }
}

impl EvmSimulator {
    pub fn new() -> Self {
        Self {
            flash_loan_premium_bps: 5, // Aave V3: 0.05%
        }
    }

    pub async fn simulate<T, N, P>(
        &self,
        route: &ArbitrageRoute,
        provider: Arc<P>,
        executor_address: Address,
        calldata: Bytes,
    ) -> Result<SimulationResult>
    where
        T: Transport + Clone,
        N: Network,
        P: Provider<T, N>,
    {
        let start = Instant::now();

        // 1. Initialize AlchemistDB (state forking)
        // Note: AlchemistDB is a mock placeholder here; in production, use a real state fork DB.
        let mut db = CacheDB::new(EmptyDB::default());

        // 2. Setup mock executor account
        db.insert_account_info(executor_address, AccountInfo {
            balance: rU256::from(10u128.pow(18)), // 1 ETH
            ..Default::default()
        });

        // 3. Configure EVM Environment
        let mut env = Env::default();
        env.tx.caller = executor_address;
        env.tx.transact_to = TransactTo::Call(executor_address); // Mocking call to self or contract
        env.tx.data = calldata;
        env.tx.value = rU256::ZERO;

        // 4. Execute simulation
        let mut evm = Evm::builder()
            .with_db(db)
            .with_env(Box::new(env))
            .build();

        let ref_tx = evm.transact()?;
        let result = ref_tx.result;

        match result {
            ExecutionResult::Success { gas_used, .. } => {
                let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
                Ok(SimulationResult {
                    optimal_loan_size: route.optimal_loan_size,
                    gross_profit: route.expected_gross_profit,
                    gas_used,
                    optimized_legs: Vec::new(),
                    simulation_duration_ms: duration_ms,
                })
            }
            ExecutionResult::Revert { gas_used, output } => {
                eyre::bail!("Simulation reverted: gas_used={}, output={:?}", gas_used, output)
            }
            ExecutionResult::Halt { reason, .. } => {
                eyre::bail!("Simulation halted: reason={:?}", reason)
            }
        }
    }
}
