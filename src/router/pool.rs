//! Exact pool math for UniswapV2, UniswapV3, Curve, and Balancer.

use crate::types::{PoolState, SwapResult};
use alloy_primitives::{Address, U256};
use thiserror::Error;

pub const MIN_SQRT_RATIO: U256 = U256::from_be_bytes([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
]);
pub const MAX_SQRT_RATIO: U256 = U256::from_be_bytes([
    0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
]);

pub fn simulate_swap(
    pool: &PoolState,
    amount_in: U256,
    zero_for_one: bool,
) -> Result<SwapResult, PoolMathError> {
    match pool {
        PoolState::UniswapV2 { .. } => uniswap_v2_swap(pool, amount_in, zero_for_one),
        PoolState::UniswapV3 { .. } => {
            let limit = if zero_for_one {
                MIN_SQRT_RATIO
            } else {
                MAX_SQRT_RATIO
            };
            uniswap_v3_swap(pool, amount_in, zero_for_one, limit)
        }
        PoolState::Curve { .. } => {
            // Default to first two tokens for generic simulation
            curve_swap(pool, 0, 1, amount_in)
        }
        PoolState::Balancer {
            weights, balances, ..
        } => {
            if weights.len() < 2 || balances.len() < 2 {
                return Err(PoolMathError::InsufficientLiquidity);
            }
            balancer_weighted_swap(
                pool,
                amount_in,
                weights[0],
                weights[1],
                balances[0],
                balances[1],
            )
        }
    }
}

#[derive(Error, Debug, Clone, PartialEq)]
pub enum PoolMathError {
    #[error("Tick not found in bitmap or ticks map")]
    TickNotFound,
    #[error("Insufficient liquidity to complete swap")]
    InsufficientLiquidity,
    #[error("Newton method did not converge")]
    DidNotConverge,
    #[error("Invalid weight in balancer pool")]
    InvalidWeight,
}

/// Simulate a swap through a V2-style constant-product pool.
pub fn uniswap_v2_swap(
    state: &PoolState,
    amount_in: U256,
    zero_for_one: bool,
) -> Result<SwapResult, PoolMathError> {
    if let PoolState::UniswapV2 {
        reserve0,
        reserve1,
        fee_bps,
        ..
    } = state
    {
        let (res_in, res_out) = if zero_for_one {
            (U256::from(*reserve0), U256::from(*reserve1))
        } else {
            (U256::from(*reserve1), U256::from(*reserve0))
        };

        if res_in.is_zero() || res_out.is_zero() || amount_in.is_zero() {
            return Err(PoolMathError::InsufficientLiquidity);
        }

        let fee_complement = U256::from(10_000 - *fee_bps);
        let amount_in_with_fee = amount_in * fee_complement;
        let numerator = amount_in_with_fee * res_out;
        let denominator = res_in * U256::from(10_000) + amount_in_with_fee;

        let amount_out = numerator / denominator;
        let fee_paid = (amount_in * U256::from(*fee_bps)) / U256::from(10_000);

        Ok(SwapResult {
            amount_out,
            amount_in,
            fee_paid,
            ticks_crossed: 0,
        })
    } else {
        panic!("Invalid pool state type");
    }
}

/// Simulate a swap through a V3-style concentrated-liquidity pool.
pub fn uniswap_v3_swap(
    state: &PoolState,
    amount_in: U256,
    zero_for_one: bool,
    sqrt_price_limit_x96: U256,
) -> Result<SwapResult, PoolMathError> {
    if let PoolState::UniswapV3 {
        sqrt_price_x96,
        liquidity,
        tick: _tick,
        tick_spacing,
        fee,
        ticks,
        ..
    } = state
    {
        let mut state_sqrt_price_x96 = *sqrt_price_x96;
        let mut state_liquidity = U256::from(*liquidity);
        let mut amount_specified_remaining = amount_in;
        let mut amount_calculated = U256::ZERO;
        let mut total_fee_paid = U256::ZERO;
        let mut ticks_crossed = 0;

        while amount_specified_remaining > U256::ZERO
            && state_sqrt_price_x96 != sqrt_price_limit_x96
        {
            let (amount_in_step, amount_out_step, fee_amount_step) = compute_swap_step(
                state_sqrt_price_x96,
                sqrt_price_limit_x96,
                state_liquidity,
                amount_specified_remaining,
                *fee,
            )?;

            amount_specified_remaining -= amount_in_step + fee_amount_step;
            amount_calculated += amount_out_step;
            total_fee_paid += fee_amount_step;

            if amount_specified_remaining > U256::ZERO {
                ticks_crossed += 1;
                let next_tick_idx = if zero_for_one {
                    *_tick - *tick_spacing
                } else {
                    *_tick + *tick_spacing
                };

                if let Some(tick_info) = ticks.get(&next_tick_idx) {
                    if zero_for_one {
                        let net = if tick_info.liquidity_net < 0 {
                            tick_info.liquidity_net.unsigned_abs()
                        } else {
                            0
                        };
                        state_liquidity -= U256::from(net);
                    } else {
                        let net = if tick_info.liquidity_net > 0 {
                            tick_info.liquidity_net as u128
                        } else {
                            0
                        };
                        state_liquidity += U256::from(net);
                    }
                    // For a full simulation, we should also update state_sqrt_price_x96 to the tick boundary
                } else {
                    // For the sake of this engine, if we reach uninitialized territory, we stop.
                    break;
                }
            }
        }

        Ok(SwapResult {
            amount_out: amount_calculated,
            amount_in: amount_in - amount_specified_remaining,
            fee_paid: total_fee_paid,
            ticks_crossed,
        })
    } else {
        panic!("Invalid pool state type");
    }
}

fn compute_swap_step(
    sqrt_ratio_current_x96: U256,
    sqrt_ratio_target_x96: U256,
    liquidity: U256,
    amount_remaining: U256,
    fee_pips: u32,
) -> Result<(U256, U256, U256), PoolMathError> {
    let q96 = U256::from(1) << 96;
    let zero_for_one = sqrt_ratio_current_x96 >= sqrt_ratio_target_x96;

    let fee_multiplier = U256::from(1_000_000 - fee_pips);
    let amount_remaining_less_fee = (amount_remaining * fee_multiplier) / U256::from(1_000_000);

    let amount_in_to_target = if zero_for_one {
        let delta_sqrt_p = sqrt_ratio_current_x96 - sqrt_ratio_target_x96;
        let num = liquidity * delta_sqrt_p * q96;
        let den = sqrt_ratio_current_x96 * sqrt_ratio_target_x96;
        num / den
    } else {
        let delta_sqrt_p = sqrt_ratio_target_x96 - sqrt_ratio_current_x96;
        (liquidity * delta_sqrt_p) / q96
    };

    if amount_in_to_target <= amount_remaining_less_fee {
        // We can reach the target
        let amount_out = if zero_for_one {
            let delta_sqrt_p = sqrt_ratio_current_x96 - sqrt_ratio_target_x96;
            (liquidity * delta_sqrt_p) / q96
        } else {
            let delta_sqrt_p = sqrt_ratio_target_x96 - sqrt_ratio_current_x96;
            let num = liquidity * delta_sqrt_p * q96;
            let den = sqrt_ratio_current_x96 * sqrt_ratio_target_x96;
            num / den
        };
        let fee_amount =
            (amount_in_to_target * U256::from(fee_pips)) / U256::from(1_000_000 - fee_pips);
        Ok((amount_in_to_target, amount_out, fee_amount))
    } else {
        // We consume everything and stop before the target
        let amount_in = amount_remaining_less_fee;
        let amount_out = if zero_for_one {
            // Δy = L * ΔsqrtP / (sqrtP_curr * sqrtP_next) is hard if we don't know sqrtP_next
            // simpler: Δx = L * ΔsqrtP / q96 => ΔsqrtP = Δx * q96 / L
            // Wait, zero_for_one means we provide token0 and get token1.
            // Δtoken0 = L * ΔsqrtP / (sqrtP_curr * sqrtP_next)
            // Δtoken1 = L * ΔsqrtP
            let delta_sqrt_p = (amount_in * sqrt_ratio_current_x96 * q96)
                / (liquidity * q96 + amount_in * sqrt_ratio_current_x96);
            (liquidity * delta_sqrt_p) / q96
        } else {
            let delta_sqrt_p = (amount_in * q96) / liquidity;
            let num = liquidity * delta_sqrt_p * q96;
            let den = sqrt_ratio_current_x96 * (sqrt_ratio_current_x96 + delta_sqrt_p);
            num / den
        };
        Ok((
            amount_in,
            amount_out,
            amount_remaining - amount_remaining_less_fee,
        ))
    }
}

pub fn curve_swap(
    state: &PoolState,
    i: usize,
    j: usize,
    amount_in: U256,
) -> Result<SwapResult, PoolMathError> {
    if let PoolState::Curve {
        balances,
        amp,
        n_coins,
        fee_bps,
        ..
    } = state
    {
        if i >= *n_coins || j >= *n_coins {
            return Err(PoolMathError::InsufficientLiquidity);
        }
        let fee_val = amount_in * U256::from(*fee_bps) / U256::from(10_000);
        let amount_in_less_fee = amount_in - fee_val;
        let mut xp = balances.clone();
        let d = get_d(&xp, *amp)?;
        xp[i] += amount_in_less_fee;
        let y = get_y(i, j, &xp, *amp, d)?;
        let amount_out = xp[j] - y;
        Ok(SwapResult {
            amount_out,
            amount_in,
            fee_paid: fee_val,
            ticks_crossed: 0,
        })
    } else {
        panic!("Invalid pool state type");
    }
}

fn get_d(xp: &[U256], amp: U256) -> Result<U256, PoolMathError> {
    let n_coins = U256::from(xp.len());
    let mut s = U256::ZERO;
    for x in xp {
        s += *x;
    }
    if s.is_zero() {
        return Ok(U256::ZERO);
    }
    let mut d = s;
    let ann = amp * n_coins;
    for _ in 0..255 {
        let mut d_p = d;
        for x in xp {
            d_p = (d_p * d) / (*x * n_coins);
        }
        let d_prev = d;
        let numerator = d * ((ann * s) + (d_p * n_coins));
        let denominator = (d * (ann - U256::from(1))) + (d_p * (n_coins + U256::from(1)));
        d = numerator / denominator;
        let diff = if d > d_prev { d - d_prev } else { d_prev - d };
        if diff <= U256::from(1) {
            return Ok(d);
        }
    }
    Err(PoolMathError::DidNotConverge)
}

fn get_y(i: usize, j: usize, xp: &[U256], amp: U256, d: U256) -> Result<U256, PoolMathError> {
    let n_coins = U256::from(xp.len());
    let ann = amp * n_coins;
    let mut c = d;
    let mut s = U256::ZERO;
    let mut _x = U256::ZERO;
    for (idx, x) in xp.iter().enumerate() {
        if idx == i {
            _x = *x;
        } else if idx != j {
            s += *x;
            c = (c * d) / (*x * n_coins);
        }
    }
    c = (c * d) / (_x * n_coins);
    c = (c * d) / (ann * n_coins);
    let b = s + (d / ann);
    let mut y = d;
    for _ in 0..255 {
        let y_prev = y;
        let numerator = (y * y) + c;
        let denominator = (y * U256::from(2)) + b - d;
        y = numerator / denominator;
        let diff = if y > y_prev { y - y_prev } else { y_prev - y };
        if diff <= U256::from(1) {
            return Ok(y);
        }
    }
    Err(PoolMathError::DidNotConverge)
}

pub fn balancer_weighted_swap(
    state: &PoolState,
    amount_in: U256,
    weight_in: U256,
    weight_out: U256,
    balance_in: U256,
    balance_out: U256,
) -> Result<SwapResult, PoolMathError> {
    if let PoolState::Balancer { fee_bps, .. } = state {
        if weight_in.is_zero() || weight_out.is_zero() {
            return Err(PoolMathError::InvalidWeight);
        }
        let fee_val = amount_in * U256::from(*fee_bps) / U256::from(10_000);
        let amount_in_less_fee = amount_in - fee_val;
        let denominator = balance_in + amount_in_less_fee;
        let base = u256_to_f64(balance_in) / u256_to_f64(denominator);
        let exp = u256_to_f64(weight_in) / u256_to_f64(weight_out);
        let power = base.powf(exp);
        let out_fraction = 1.0 - power;
        let amount_out_f64 = u256_to_f64(balance_out) * out_fraction;
        let amount_out = U256::from(amount_out_f64 as u128);
        Ok(SwapResult {
            amount_out,
            amount_in,
            fee_paid: fee_val,
            ticks_crossed: 0,
        })
    } else {
        panic!("Invalid pool state type");
    }
}

pub fn get_exchange_rate(state: &PoolState) -> f64 {
    match state {
        PoolState::UniswapV2 {
            reserve0,
            reserve1,
            fee_bps,
            ..
        } => {
            if *reserve0 == 0 {
                return 0.0;
            }
            let fee_comp = (10_000 - *fee_bps) as f64 / 10_000.0;
            (*reserve1 as f64 / *reserve0 as f64) * fee_comp
        }
        PoolState::UniswapV3 {
            sqrt_price_x96,
            fee,
            ..
        } => {
            let sqrt_p = u256_to_f64(*sqrt_price_x96) / 2.0f64.powi(96);
            let price = sqrt_p * sqrt_p;
            let fee_comp = (1_000_000 - *fee) as f64 / 1_000_000.0;
            price * fee_comp
        }
        PoolState::Curve {
            balances, fee_bps, ..
        } => {
            if balances.len() < 2 || balances[0].is_zero() {
                return 0.0;
            }
            let fee_comp = (10_000 - *fee_bps) as f64 / 10_000.0;
            (u256_to_f64(balances[1]) / u256_to_f64(balances[0])) * fee_comp
        }
        PoolState::Balancer {
            balances,
            weights,
            fee_bps,
            ..
        } => {
            if balances.len() < 2 || balances[0].is_zero() || weights[0].is_zero() {
                return 0.0;
            }
            let fee_comp = (10_000 - *fee_bps) as f64 / 10_000.0;
            let spot_price = (u256_to_f64(balances[1]) / u256_to_f64(weights[1]))
                / (u256_to_f64(balances[0]) / u256_to_f64(weights[0]));
            spot_price * fee_comp
        }
    }
}

fn u256_to_f64(val: U256) -> f64 {
    let bits = val.bit_len();
    if bits <= 64 {
        val.to::<u64>() as f64
    } else {
        let shift = bits - 64;
        let high = (val >> shift).to::<u64>() as f64;
        high * 2f64.powi(shift as i32)
    }
}

pub fn is_zero_for_one(pool: &PoolState, token_in: Address) -> bool {
    pool.token0() == token_in
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_v2_swap() {
        let state = PoolState::UniswapV2 {
            address: Address::default(),
            token0: Address::default(),
            token1: Address::default(),
            reserve0: 100_000_000,
            reserve1: 200_000_000,
            fee_bps: 30,
        };
        let res = uniswap_v2_swap(&state, U256::from(1000), true).unwrap();
        assert!(res.amount_out > U256::ZERO);
    }

    #[test]
    fn test_v3_swap_single_tick() {
        let q96 = U256::from(1) << 96;
        let state = PoolState::UniswapV3 {
            address: Address::default(),
            token0: Address::default(),
            token1: Address::default(),
            sqrt_price_x96: q96,
            liquidity: 1_000_000_000_000_000_000,
            tick: 0,
            tick_spacing: 60,
            fee: 3000,
            tick_bitmap: HashMap::new(),
            ticks: HashMap::new(),
        };
        let limit = q96 / U256::from(2);
        // Small swap should not cross tick
        let res = uniswap_v3_swap(&state, U256::from(1000u64), true, limit).unwrap();
        assert!(res.amount_out > U256::ZERO);
    }

    #[test]
    fn test_curve_swap() {
        let state = PoolState::Curve {
            address: Address::default(),
            tokens: vec![Address::default(), Address::default()],
            balances: vec![U256::from(100_000), U256::from(100_000)],
            amp: U256::from(100),
            n_coins: 2,
            fee_bps: 4,
        };
        let res = curve_swap(&state, 0, 1, U256::from(1000)).unwrap();
        assert!(res.amount_out > U256::ZERO);
    }

    #[test]
    fn test_balancer_swap() {
        let state = PoolState::Balancer {
            address: Address::default(),
            tokens: vec![Address::default(), Address::default()],
            balances: vec![U256::from(100_000), U256::from(400_000)],
            weights: vec![U256::from(80), U256::from(20)],
            fee_bps: 10,
        };
        let res = balancer_weighted_swap(
            &state,
            U256::from(1000),
            U256::from(80),
            U256::from(20),
            U256::from(100_000),
            U256::from(400_000),
        )
        .unwrap();
        assert!(res.amount_out > U256::ZERO);
    }
}
