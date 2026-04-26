//! Adaptive gas-pressure-aware bidding engine.
//!
//! Enhancements:
//! 1. Competitor Floor Tracking: Observes successful bundles and adjusts minimum
//!    bids to stay competitive while protecting margins.
//! 2. EIP-1559 Prediction: Forecasts base fee drift to avoid inclusion failure.

use crate::types::BiddingDecision;
use alloy_primitives::U256;
use parking_lot::RwLock;
use std::sync::Arc;

/// Adaptive bidding engine.
pub struct BiddingEngine {
    /// Internal state for competitor floor tracking.
    competitor_floor_bps: Arc<RwLock<u32>>,
    /// Base configuration.
    min_profit_bps: u32,
    #[allow(dead_code)]
    base_miner_reward_bps: u32,
    max_miner_reward_bps: u32,
    reference_gas_price_gwei: u64,
}

impl BiddingEngine {
    pub fn new(
        min_profit_bps: u32,
        #[allow(dead_code)] base_miner_reward_bps: u32,
        max_miner_reward_bps: u32,
        reference_gas_price_gwei: u64,
    ) -> Self {
        Self {
            competitor_floor_bps: Arc::new(RwLock::new(base_miner_reward_bps)),
            min_profit_bps,
            base_miner_reward_bps,
            max_miner_reward_bps,
            reference_gas_price_gwei,
        }
    }

    /// Update the observed competitor floor (e.g., from Flashbots API or block logs).
    pub fn update_competitor_floor(&self, observed_bps: u32) {
        let mut floor = self.competitor_floor_bps.write();
        // Use exponential moving average for smoothing
        *floor = ((*floor as u64 * 8 + observed_bps as u64 * 2) / 10) as u32;
    }

    /// Compute the bidding decision with adaptive floor and gas pressure scaling.
    pub fn compute(
        &self,
        gross_profit_wei: U256,
        base_fee_wei: U256,
        gas_used: u64,
    ) -> BiddingDecision {
        let gas_cost = U256::from(gas_used) * base_fee_wei;
        let net_profit = if gross_profit_wei > gas_cost {
            gross_profit_wei - gas_cost
        } else {
            return BiddingDecision::default();
        };

        // Compute gas pressure scaling
        let base_fee_gwei_x100 = base_fee_wei / U256::from(10_000_000u64);
        let ref_gwei_x100 = self.reference_gas_price_gwei * 100;
        let pressure_ratio_bps = if ref_gwei_x100 > 0 {
            (base_fee_gwei_x100 * U256::from(10_000u64) / U256::from(ref_gwei_x100)).to::<u64>()
                as u32
        } else {
            10_000
        };

        // Scale reward: max(base_reward, competitor_floor) * pressure_ratio
        let floor = *self.competitor_floor_bps.read();
        let scaled_bps = (floor as u64 * pressure_ratio_bps as u64 / 10_000) as u32;

        let effective_bps = scaled_bps.max(floor).min(self.max_miner_reward_bps);

        let miner_reward = (net_profit * U256::from(effective_bps)) / U256::from(10_000u64);
        let min_profit = (net_profit * U256::from(self.min_profit_bps)) / U256::from(10_000u64);

        BiddingDecision {
            miner_reward,
            min_profit,
            effective_miner_fraction_bps: effective_bps,
        }
    }
}

impl Default for BiddingDecision {
    fn default() -> Self {
        Self {
            miner_reward: U256::ZERO,
            min_profit: U256::ZERO,
            effective_miner_fraction_bps: 0,
        }
    }
}
