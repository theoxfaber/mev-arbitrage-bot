//! Dynamic gas-pressure-aware bidding strategy.
//!
//! Replaces hardcoded profit/reward ratios with a model that scales the miner
//! reward fraction based on current network gas pressure. When gas is expensive
//! (high competition), builders need more incentive — we offer more. When gas
//! is cheap, we keep more of the profit.
//!
//! All math uses integer basis points (1 bps = 0.01%) to avoid floating-point
//! precision issues in the hot path.

use crate::types::BiddingDecision;
use alloy_primitives::U256;

/// Dynamic bidding strategy with gas-pressure scaling.
pub struct BiddingStrategy {
    /// Minimum profit fraction in basis points (e.g., 3000 = 30%).
    min_profit_bps: u32,
    /// Base miner reward fraction at the reference gas price.
    base_miner_reward_bps: u32,
    /// Maximum miner reward fraction (hard ceiling).
    max_miner_reward_bps: u32,
    /// Reference gas price in gwei for baseline scaling.
    reference_gas_price_gwei: u64,
}

impl BiddingStrategy {
    pub fn new(
        min_profit_bps: u32,
        base_miner_reward_bps: u32,
        max_miner_reward_bps: u32,
        reference_gas_price_gwei: u64,
    ) -> Self {
        Self {
            min_profit_bps,
            base_miner_reward_bps,
            max_miner_reward_bps,
            reference_gas_price_gwei,
        }
    }

    /// Compute the miner reward and minimum profit for a given opportunity.
    ///
    /// # Arguments
    /// * `gross_profit_wei` - Simulated gross profit from the route.
    /// * `base_fee_wei` - Current block's baseFeePerGas.
    /// * `gas_used` - Estimated gas for the bundle.
    pub fn compute(
        &self,
        gross_profit_wei: U256,
        base_fee_wei: U256,
        gas_used: u64,
    ) -> BiddingDecision {
        // 1. Compute gas cost
        let gas_cost = U256::from(gas_used) * base_fee_wei;
        let net_profit = if gross_profit_wei > gas_cost {
            gross_profit_wei - gas_cost
        } else {
            return BiddingDecision {
                miner_reward: U256::ZERO,
                min_profit: U256::ZERO,
                effective_miner_fraction_bps: 0,
            };
        };

        // 2. Compute gas pressure ratio (in basis points for integer math)
        // base_fee in gwei = base_fee_wei / 1e9
        let base_fee_gwei_x100 = base_fee_wei / U256::from(10_000_000u64); // base_fee / 1e7 = gwei * 100
        let ref_gwei_x100 = self.reference_gas_price_gwei * 100;

        // pressure_ratio = base_fee_gwei / reference_gwei (scaled by 10000 for bps math)
        let pressure_ratio_bps = if ref_gwei_x100 > 0 {
            let ratio = base_fee_gwei_x100 * U256::from(10_000u64) / U256::from(ref_gwei_x100);
            ratio.to::<u64>() as u32
        } else {
            10_000u32 // 1.0x if reference is zero
        };

        // 3. Scale miner reward: base_reward_bps * pressure_ratio / 10000
        let scaled_bps = (self.base_miner_reward_bps as u64 * pressure_ratio_bps as u64 / 10_000) as u32;
        let effective_bps = scaled_bps
            .max(self.base_miner_reward_bps)
            .min(self.max_miner_reward_bps);

        // 4. Compute actual amounts using integer bps math
        let miner_reward = (net_profit * U256::from(effective_bps)) / U256::from(10_000u64);
        let min_profit = (net_profit * U256::from(self.min_profit_bps)) / U256::from(10_000u64);

        BiddingDecision {
            miner_reward,
            min_profit,
            effective_miner_fraction_bps: effective_bps,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_baseline_bidding() {
        let strategy = BiddingStrategy::new(3000, 2500, 6500, 20);

        // Simulate: 1 ETH profit, 20 gwei base fee, 200k gas
        let profit = U256::from(1_000_000_000_000_000_000u128); // 1 ETH
        let base_fee = U256::from(20_000_000_000u64); // 20 gwei
        let gas = 200_000u64;

        let decision = strategy.compute(profit, base_fee, gas);

        // Gas cost = 200k * 20 gwei = 0.004 ETH
        // Net profit ≈ 0.996 ETH
        // At reference gas price (1.0x), miner fraction = base 25%
        // Miner reward ≈ 0.249 ETH
        assert!(decision.miner_reward > U256::ZERO);
        assert!(decision.min_profit > U256::ZERO);
        assert_eq!(decision.effective_miner_fraction_bps, 2500);
    }

    #[test]
    fn test_high_gas_pressure() {
        let strategy = BiddingStrategy::new(3000, 2500, 6500, 20);

        let profit = U256::from(1_000_000_000_000_000_000u128);
        let base_fee = U256::from(60_000_000_000u64); // 60 gwei (3x reference)
        let gas = 200_000u64;

        let decision = strategy.compute(profit, base_fee, gas);

        // At 3x reference gas, miner fraction should scale up
        assert!(decision.effective_miner_fraction_bps > 2500);
        assert!(decision.effective_miner_fraction_bps <= 6500);
    }

    #[test]
    fn test_unprofitable_after_gas() {
        let strategy = BiddingStrategy::new(3000, 2500, 6500, 20);

        let profit = U256::from(1_000_000u128); // Tiny profit
        let base_fee = U256::from(100_000_000_000u64); // 100 gwei
        let gas = 500_000u64;

        let decision = strategy.compute(profit, base_fee, gas);

        // Gas cost >> profit → should be zero
        assert_eq!(decision.miner_reward, U256::ZERO);
    }
}
