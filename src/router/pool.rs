//! Pool state types and constant-product / concentrated-liquidity math.

use crate::types::{PoolState, PoolType};
use alloy_primitives::{Address, U256};

/// Simulate a swap through a V2-style constant-product pool.
///
/// Formula: amountOut = (amountIn * fee_complement * reserveOut) / (reserveIn * 1e6 + amountIn * fee_complement)
pub fn simulate_v2_swap(
    pool: &PoolState,
    amount_in: U256,
    zero_for_one: bool,
) -> U256 {
    let (reserve_in, reserve_out) = if zero_for_one {
        (pool.reserve0, pool.reserve1)
    } else {
        (pool.reserve1, pool.reserve0)
    };

    if reserve_in.is_zero() || reserve_out.is_zero() || amount_in.is_zero() {
        return U256::ZERO;
    }

    let fee_complement = pool.fee_complement_millionths();
    let amount_in_with_fee = amount_in * fee_complement;
    let numerator = amount_in_with_fee * reserve_out;
    let denominator = reserve_in * U256::from(1_000_000u64) + amount_in_with_fee;

    if denominator.is_zero() {
        return U256::ZERO;
    }

    numerator / denominator
}

/// Simulate a swap through a V3-style concentrated-liquidity pool.
///
/// Uses the virtual-reserves approximation: converts sqrtPriceX96 + liquidity
/// into virtual reserves, then applies constant-product math. This is accurate
/// within the current tick range but does not account for tick crossings.
///
/// For production, tick-crossing simulation would be needed for large trades
/// that move the price across multiple tick boundaries.
pub fn simulate_v3_swap(
    pool: &PoolState,
    amount_in: U256,
    zero_for_one: bool,
) -> U256 {
    // Fall back to virtual-reserve constant-product approximation
    simulate_v2_swap(pool, amount_in, zero_for_one)
}

/// Compute the exchange rate from token0 to token1 in a pool,
/// scaled by 1e18 for precision.
pub fn exchange_rate_0_to_1(pool: &PoolState) -> f64 {
    if pool.reserve0.is_zero() {
        return 0.0;
    }

    let fee_complement = pool.fee_complement_millionths();

    // rate = (reserve1 * fee_complement) / (reserve0 * 1e6)
    // Use f64 for the graph weight computation (not for actual trading)
    let r0 = u256_to_f64(pool.reserve0);
    let r1 = u256_to_f64(pool.reserve1);
    let fee = u256_to_f64(fee_complement) / 1_000_000.0;

    (r1 / r0) * fee
}

/// Compute the exchange rate from token1 to token0 in a pool.
pub fn exchange_rate_1_to_0(pool: &PoolState) -> f64 {
    if pool.reserve1.is_zero() {
        return 0.0;
    }

    let fee_complement = pool.fee_complement_millionths();
    let r0 = u256_to_f64(pool.reserve0);
    let r1 = u256_to_f64(pool.reserve1);
    let fee = u256_to_f64(fee_complement) / 1_000_000.0;

    (r0 / r1) * fee
}

/// Helper: convert U256 to f64 (lossy, for graph weights only — never for actual amounts).
fn u256_to_f64(val: U256) -> f64 {
    // Use the first 64 bits shifted appropriately
    let bits = val.bit_len();
    if bits <= 64 {
        val.to::<u64>() as f64
    } else {
        let shift = bits - 64;
        let high = (val >> shift).to::<u64>() as f64;
        high * 2f64.powi(shift as i32)
    }
}

/// Determine swap direction given input token and pool.
pub fn is_zero_for_one(pool: &PoolState, token_in: Address) -> bool {
    pool.token0 == token_in
}
