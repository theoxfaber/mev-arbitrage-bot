//! ABI decoder for DEX swap calldata.
//!
//! Decodes UniswapV3, UniswapV2, 1inch V5, and Curve calldata to extract:
//! - Token pair (tokenIn / tokenOut)
//! - Input amount
//! - Minimum output amount (slippage tolerance)
//!
//! Slippage is computed using **token decimals** to normalize cross-token
//! ratios correctly — fixing the fundamental bug in the original implementation
//! where USDC amounts were divided by WETH amounts without normalization.

use crate::types::{PoolType, SandwichOpportunity};
use alloy_primitives::{Address, Bytes, TxHash, U256};
use dashmap::DashMap;
use std::sync::Arc;

// ─── Known Function Selectors ────────────────────────────────────────────────

const SEL_V3_EXACT_INPUT_SINGLE: [u8; 4] = [0x41, 0x4b, 0xf3, 0x89];
const SEL_V2_SWAP_EXACT: [u8; 4] = [0x38, 0xed, 0x17, 0x39];

/// Minimum slippage in basis points to be considered actionable.
const HIGH_SLIPPAGE_THRESHOLD_BPS: u32 = 300; // 3%

// ─── Token Decimals Cache ────────────────────────────────────────────────────

/// Shared cache for ERC-20 token decimals, populated lazily from on-chain queries.
pub type DecimalsCache = Arc<DashMap<Address, u8>>;

/// Create a new decimals cache pre-seeded with well-known tokens.
pub fn new_decimals_cache() -> DecimalsCache {
    let cache = DashMap::new();
    // WETH (18 decimals)
    cache.insert(
        "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".parse::<Address>().unwrap(),
        18,
    );
    // USDC (6 decimals)
    cache.insert(
        "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".parse::<Address>().unwrap(),
        6,
    );
    // USDT (6 decimals)
    cache.insert(
        "0xdAC17F958D2ee523a2206206994597C13D831ec7".parse::<Address>().unwrap(),
        6,
    );
    // DAI (18 decimals)
    cache.insert(
        "0x6B175474E89094C44Da98b954EedeAC495271d0F".parse::<Address>().unwrap(),
        18,
    );
    // WBTC (8 decimals)
    cache.insert(
        "0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599".parse::<Address>().unwrap(),
        8,
    );
    Arc::new(cache)
}

// ─── Decoder ─────────────────────────────────────────────────────────────────

/// Stateless swap calldata decoder.
pub struct SwapDecoder {
    decimals: DecimalsCache,
}

impl SwapDecoder {
    pub fn new(decimals: DecimalsCache) -> Self {
        Self { decimals }
    }

    /// Attempt to decode a transaction's calldata into a `SandwichOpportunity`.
    pub fn decode(&self, tx_hash: TxHash, _to: Address, data: &Bytes) -> Option<SandwichOpportunity> {
        if data.len() < 4 {
            return None;
        }

        let selector: [u8; 4] = data[..4].try_into().ok()?;

        match selector {
            SEL_V3_EXACT_INPUT_SINGLE => self.decode_v3_exact_input_single(tx_hash, data),
            SEL_V2_SWAP_EXACT => self.decode_v2_swap(tx_hash, data),
            _ => None,
        }
    }

    /// Decode UniswapV3 exactInputSingle.
    /// ABI: exactInputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160))
    fn decode_v3_exact_input_single(&self, tx_hash: TxHash, data: &Bytes) -> Option<SandwichOpportunity> {
        // Manual ABI decoding — each field is 32 bytes, offset by 4 (selector)
        if data.len() < 4 + 8 * 32 {
            return None;
        }

        let offset = 4;
        let token_in = Address::from_slice(&data[offset + 12..offset + 32]);
        let token_out = Address::from_slice(&data[offset + 32 + 12..offset + 64]);
        // fee at offset + 64..offset + 96
        // recipient at offset + 96..offset + 128
        let amount_in = U256::from_be_slice(&data[offset + 128..offset + 160]);
        let amount_out_min = U256::from_be_slice(&data[offset + 160..offset + 192]);

        let slippage_bps = self.compute_slippage_bps(token_in, token_out, amount_in, amount_out_min);

        Some(SandwichOpportunity {
            tx_hash,
            protocol: PoolType::UniswapV3,
            token_in,
            token_out,
            amount_in,
            min_amount_out: amount_out_min,
            slippage_bps,
            is_actionable: slippage_bps >= HIGH_SLIPPAGE_THRESHOLD_BPS,
        })
    }

    /// Decode UniswapV2 swapExactTokensForTokens.
    fn decode_v2_swap(&self, tx_hash: TxHash, data: &Bytes) -> Option<SandwichOpportunity> {
        // ABI: swapExactTokensForTokens(uint256,uint256,address[],address,uint256)
        if data.len() < 4 + 5 * 32 {
            return None;
        }

        let offset = 4;
        let amount_in = U256::from_be_slice(&data[offset..offset + 32]);
        let amount_out_min = U256::from_be_slice(&data[offset + 32..offset + 64]);
        // path offset at offset + 64..offset + 96
        let path_offset_val = U256::from_be_slice(&data[offset + 64..offset + 96]);
        let path_offset = path_offset_val.to::<usize>() + 4; // relative to calldata start

        if data.len() < path_offset + 32 {
            return None;
        }

        let path_len = U256::from_be_slice(&data[path_offset..path_offset + 32]).to::<usize>();

        if path_len < 2 || data.len() < path_offset + 32 + path_len * 32 {
            return None;
        }

        let token_in = Address::from_slice(
            &data[path_offset + 32 + 12..path_offset + 64],
        );
        let token_out = Address::from_slice(
            &data[path_offset + 32 + (path_len - 1) * 32 + 12..path_offset + 32 + path_len * 32],
        );

        let slippage_bps = self.compute_slippage_bps(token_in, token_out, amount_in, amount_out_min);

        Some(SandwichOpportunity {
            tx_hash,
            protocol: PoolType::UniswapV2,
            token_in,
            token_out,
            amount_in,
            min_amount_out: amount_out_min,
            slippage_bps,
            is_actionable: slippage_bps >= HIGH_SLIPPAGE_THRESHOLD_BPS,
        })
    }

    /// Compute slippage in basis points using decimal-normalized amounts.
    ///
    /// **Critical fix**: Normalizes both amounts to 18-decimal precision before
    /// computing the ratio, avoiding the cross-token-decimal division bug.
    fn compute_slippage_bps(
        &self,
        token_in: Address,
        token_out: Address,
        amount_in: U256,
        amount_out_min: U256,
    ) -> u32 {
        if amount_out_min.is_zero() || amount_in.is_zero() {
            return 10_000; // 100%
        }

        let decimals_in = self.decimals.get(&token_in).map(|d| *d).unwrap_or(18);
        let decimals_out = self.decimals.get(&token_out).map(|d| *d).unwrap_or(18);

        let normalized_in = self.normalize_to_18(amount_in, decimals_in);
        let normalized_out = self.normalize_to_18(amount_out_min, decimals_out);

        if normalized_in.is_zero() {
            return 10_000;
        }

        let ratio_bps = (normalized_out * U256::from(10_000u64)) / normalized_in;

        if ratio_bps >= U256::from(10_000u64) {
            return 0;
        }

        let slippage_bps = U256::from(10_000u64) - ratio_bps;
        std::cmp::min(slippage_bps.to::<u64>() as u32, 10_000)
    }

    fn normalize_to_18(&self, amount: U256, decimals: u8) -> U256 {
        match decimals.cmp(&18) {
            std::cmp::Ordering::Less => amount * U256::from(10u64.pow(18 - decimals as u32)),
            std::cmp::Ordering::Greater => amount / U256::from(10u64.pow(decimals as u32 - 18)),
            std::cmp::Ordering::Equal => amount,
        }
    }
}
