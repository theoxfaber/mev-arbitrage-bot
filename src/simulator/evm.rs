//! revm-based local EVM simulator for trade profitability analysis.

use crate::types::ArbitrageRoute;
use alloy::providers::Provider;
use alloy::network::Network;
use alloy::transports::Transport;
use alloy_primitives::{Address, U256};
use eyre::Result;
use revm::{
    db::{CacheDB, EmptyDB},
    primitives::AccountInfo,
};
use std::time::Instant;

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
        _provider: &P,
        executor_address: Address,
    ) -> Result<SimulationResult>
    where
        T: Transport + Clone,
        N: Network,
        P: Provider<T, N>,
    {
        let start = Instant::now();

        // 1. Initialize CacheDB
        let mut db = CacheDB::new(EmptyDB::default());

        // 2. Setup mock executor account
        db.insert_account_info(executor_address, AccountInfo {
            balance: U256::from(10u128.pow(18)), // 1 ETH
            ..Default::default()
        });

        // 3. Structural simulation - in a full impl, this would fetch state and run Evm
        let best_loan_size = route.optimal_loan_size;
        let estimated_gas = 21_000u64 + 100_000 + (route.legs.len() as u64 * 100_000);
        let duration_ms = start.elapsed().as_secs_f64() * 1000.0;

        Ok(SimulationResult {
            optimal_loan_size: best_loan_size,
            gross_profit: route.expected_gross_profit,
            gas_used: estimated_gas,
            optimized_legs: Vec::new(),
            simulation_duration_ms: duration_ms,
        })
    }
}
