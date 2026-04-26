//! Multicall3 batched on-chain reads.
//!
//! Instead of making N individual RPC calls to fetch pool states, we batch them
//! into a single `Multicall3.aggregate3` call for 10-50x latency reduction.

use crate::types::PoolType;
use alloy_primitives::{Address, Bytes, U256};

/// Multicall3 contract address (same on all EVM chains).
pub const MULTICALL3_ADDRESS: &str = "0xcA11bde05977b3631167028862bE2a173976CA11";

/// Build the getReserves() calldata for a V2 pool.
pub fn encode_get_reserves() -> Bytes {
    // Function selector: getReserves() = 0x0902f1ac
    Bytes::from(vec![0x09, 0x02, 0xf1, 0xac])
}

/// Build the slot0() calldata for a V3 pool.
pub fn encode_slot0() -> Bytes {
    // Function selector: slot0() = 0x3850c7bd
    Bytes::from(vec![0x38, 0x50, 0xc7, 0xbd])
}

/// Build the liquidity() calldata for a V3 pool.
pub fn encode_liquidity() -> Bytes {
    // Function selector: liquidity() = 0x1a686502
    Bytes::from(vec![0x1a, 0x68, 0x65, 0x02])
}

/// Build a Multicall3 aggregate call for pool state reads.
///
/// Returns a list of (target, calldata) pairs to be batched.
pub fn build_pool_state_calls(pools: &[(Address, PoolType)]) -> Vec<(Address, Bytes)> {
    let mut calls = Vec::new();

    for (addr, pool_type) in pools {
        match pool_type {
            PoolType::UniswapV2 | PoolType::SushiswapV2 => {
                calls.push((*addr, encode_get_reserves()));
            }
            PoolType::UniswapV3 => {
                calls.push((*addr, encode_slot0()));
                calls.push((*addr, encode_liquidity()));
            }
            PoolType::CurveStableSwap | PoolType::BalancerWeighted => {
                // Curve/Balancer need custom per-pool ABI calls
            }
        }
    }

    calls
}

/// Intermediate update type for pool state parsing.
#[derive(Debug)]
pub enum PoolStateUpdate {
    V2Reserves {
        reserve0: U256,
        reserve1: U256,
    },
    V3State {
        sqrt_price_x96: U256,
        tick: i32,
        liquidity: u128,
    },
}
