//! Math-based trade simulator with optimal loan sizing.
//!
//! The previous `EmptyDB` revm placeholder never forked state and always
//! returned zero profit. This implementation simulates the full multi-hop
//! chain with exact pool math from `crate::router::pool` and searches loan
//! sizes for maximum net profit (after Aave premium).
//!
//! A `revm` fork can be layered on top later for final validation, but routing
//! decisions must never depend on an empty DB.

use crate::router::pool::{is_zero_for_one, simulate_swap};
use crate::types::ArbitrageRoute;
use alloy_primitives::{Address, Bytes, U256};
use eyre::Result;
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

    /// Pure-math simulation: no RPC, no revm DB. Deterministic and testable.
    pub fn simulate_math(&self, route: &ArbitrageRoute) -> Result<SimulationResult> {
        let start = Instant::now();
        if route.legs.is_empty() {
            eyre::bail!("empty route");
        }

        // Candidate loan sizes in raw units, spanning 6-dec (USDC) and
        // 18-dec (WETH) base tokens: 1 USDC .. 50 ETH.
        const CANDIDATES: [u128; 12] = [
            1_000_000,                  // 1 USDC
            1_000_000_000,              // 1k USDC
            1_000_000_000_000,          // 1M USDC / 1e-6 ETH dust
            1_000_000_000_000_000,      // 1e15: 0.001 ETH
            5_000_000_000_000_000,      // 0.005 ETH
            10_000_000_000_000_000,     // 0.01 ETH
            100_000_000_000_000_000,    // 0.1 ETH
            500_000_000_000_000_000,    // 0.5 ETH
            1_000_000_000_000_000_000,  // 1 ETH
            5_000_000_000_000_000_000,  // 5 ETH
            10_000_000_000_000_000_000, // 10 ETH
            50_000_000_000_000_000_000, // 50 ETH
        ];

        let mut best_profit = U256::ZERO;
        let mut best_loan = U256::ZERO;
        let mut best_legs: Vec<OptimizedLeg> = Vec::new();
        let mut best_gas: u64 = 0;

        for &candidate in &CANDIDATES {
            let loan = U256::from(candidate);
            match self.execute_chain(route, loan) {
                Some((final_amount, legs, gas)) => {
                    if final_amount <= loan {
                        continue;
                    }
                    let gross = final_amount - loan;
                    let premium =
                        (loan * U256::from(self.flash_loan_premium_bps)) / U256::from(10_000u64);
                    if gross <= premium {
                        continue;
                    }
                    let net = gross - premium;
                    if net > best_profit {
                        best_profit = net;
                        best_loan = loan;
                        best_legs = legs;
                        best_gas = gas;
                    }
                }
                None => continue,
            }
        }

        // Refine around the best loan with a local binary-style search
        // (half / double) to squeeze out extra bps.
        if !best_loan.is_zero() {
            for factor_num in [7500u64, 12500u64, 9000u64, 11000u64] {
                let loan = (best_loan * U256::from(factor_num)) / U256::from(10_000u64);
                if loan.is_zero() {
                    continue;
                }
                if let Some((final_amount, legs, gas)) = self.execute_chain(route, loan) {
                    if final_amount <= loan {
                        continue;
                    }
                    let gross = final_amount - loan;
                    let premium =
                        (loan * U256::from(self.flash_loan_premium_bps)) / U256::from(10_000u64);
                    if gross <= premium {
                        continue;
                    }
                    let net = gross - premium;
                    if net > best_profit {
                        best_profit = net;
                        best_loan = loan;
                        best_legs = legs;
                        best_gas = gas;
                    }
                }
            }
        }

        let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
        crate::metrics::record_simulation(duration_ms, !best_profit.is_zero());

        if best_profit.is_zero() {
            eyre::bail!("no profitable loan size found");
        }

        Ok(SimulationResult {
            optimal_loan_size: best_loan,
            gross_profit: best_profit,
            gas_used: best_gas,
            optimized_legs: best_legs,
            simulation_duration_ms: duration_ms,
        })
    }

    fn execute_chain(
        &self,
        route: &ArbitrageRoute,
        loan: U256,
    ) -> Option<(U256, Vec<OptimizedLeg>, u64)> {
        let mut amount = loan;
        let mut legs = Vec::with_capacity(route.legs.len());
        let mut gas: u64 = 120_000; // flashloan + executor overhead

        for leg in &route.legs {
            let zfo = is_zero_for_one(&leg.pool, leg.token_in);
            let res = simulate_swap(&leg.pool, amount, zfo).ok()?;
            if res.amount_out.is_zero() {
                return None;
            }
            gas += 85_000 + (res.ticks_crossed as u64) * 15_000;
            legs.push(OptimizedLeg {
                pool_address: leg.pool.address(),
                token_in: leg.token_in,
                token_out: leg.token_out,
                amount_in: amount,
                amount_out: res.amount_out,
            });
            amount = res.amount_out;
        }

        // Route must close the loop.
        if legs.last().map(|l| l.token_out) != Some(route.base_token) {
            // Allow non-closing routes only if caller explicitly wants them;
            // arbitrage requires returning to base.
            return None;
        }

        Some((amount, legs, gas))
    }

    /// Backwards-compatible async entrypoint. The provider/calldata args are
    /// kept for API stability but the decision is pure-math (no RPC roundtrip).
    /// A revm fork-validation can be added here once an AlloyDB is wired.
    pub async fn simulate<T, N, P>(
        &self,
        route: &ArbitrageRoute,
        _provider: std::sync::Arc<P>,
        _executor_address: Address,
        _calldata: Bytes,
    ) -> Result<SimulationResult>
    where
        T: alloy::transports::Transport + Clone,
        N: alloy::network::Network,
        P: alloy::providers::Provider<T, N>,
    {
        self.simulate_math(route)
    }
}
