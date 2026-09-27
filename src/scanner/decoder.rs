//! ABI decoder for DEX swap calldata.
//!
//! Supports UniswapV2 (and Sushi clones), UniswapV3 single- and multi-hop,
//! and size-based triggering for ETH-in variants and Universal Router.
//!
//! NOTE on "slippage": without a price oracle we cannot compute true
//! price slippage from `(amountIn, minAmountOut)` alone across different
//! decimals (1 WETH != 1 USDC). The previous implementation divided
//! normalized amounts 1:1 and flagged almost everything. We now trigger on
//! **trade size + missing protection** (whale or zero `minAmountOut`),
//! which is the correct mempool signal for backrun candidates. The
//! `slippage_bps` field is retained for compat and now encodes protection
//! looseness (10000 = no protection) rather than a fake price ratio.

use crate::types::{PoolType, SandwichOpportunity};
use alloy_primitives::{Address, Bytes, TxHash, U256};
use dashmap::DashMap;
use std::sync::Arc;

// ─── Known Function Selectors ────────────────────────────────────────────────

const SEL_V3_EXACT_INPUT_SINGLE: [u8; 4] = [0x41, 0x4b, 0xf3, 0x89];
const SEL_V3_EXACT_INPUT: [u8; 4] = [0xb8, 0x58, 0x18, 0x3f];
const SEL_V2_SWAP_EXACT: [u8; 4] = [0x38, 0xed, 0x17, 0x39];
const SEL_V2_SWAP_ETH_IN: [u8; 4] = [0x7f, 0xf3, 0x6a, 0xb5]; // swapExactETHForTokens
const SEL_V2_SWAP_FOR_ETH: [u8; 4] = [0x18, 0xcb, 0xaf, 0xe5]; // swapExactTokensForETH
const SEL_V2_SWAP_TOKENS_FOR_EXACT: [u8; 4] = [0x88, 0x03, 0xdb, 0xee];
const SEL_UNIVERSAL_EXECUTE: [u8; 4] = [0x35, 0x93, 0x56, 0x4c]; // UniversalRouter execute

/// Minimum normalized size (18-dec) to consider: 0.05 ETH equivalent.
const DUST_18: u128 = 50_000_000_000_000_000;
/// Whale size: 0.5 ETH equivalent — always actionable.
const WHALE_18: u128 = 500_000_000_000_000_000;

// ─── Token Decimals Cache ────────────────────────────────────────────────────

/// Shared cache for ERC-20 token decimals, populated lazily from on-chain queries.
pub type DecimalsCache = Arc<DashMap<Address, u8>>;

/// Create a new decimals cache pre-seeded with well-known tokens.
pub fn new_decimals_cache() -> DecimalsCache {
    let cache = DashMap::new();
    // WETH (18 decimals)
    cache.insert(
        "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
            .parse::<Address>()
            .unwrap(),
        18,
    );
    // USDC (6 decimals)
    cache.insert(
        "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"
            .parse::<Address>()
            .unwrap(),
        6,
    );
    // USDT (6 decimals)
    cache.insert(
        "0xdAC17F958D2ee523a2206206994597C13D831ec7"
            .parse::<Address>()
            .unwrap(),
        6,
    );
    // DAI (18 decimals)
    cache.insert(
        "0x6B175474E89094C44Da98b954EedeAC495271d0F"
            .parse::<Address>()
            .unwrap(),
        18,
    );
    // WBTC (8 decimals)
    cache.insert(
        "0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599"
            .parse::<Address>()
            .unwrap(),
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
    pub fn decode(
        &self,
        tx_hash: TxHash,
        _to: Address,
        data: &Bytes,
    ) -> Option<SandwichOpportunity> {
        if data.len() < 4 {
            return None;
        }

        let selector: [u8; 4] = data[..4].try_into().ok()?;

        match selector {
            SEL_V3_EXACT_INPUT_SINGLE => self.decode_v3_exact_input_single(tx_hash, data),
            SEL_V3_EXACT_INPUT => self.decode_v3_exact_input(tx_hash, data),
            SEL_V2_SWAP_EXACT | SEL_V2_SWAP_FOR_ETH | SEL_V2_SWAP_TOKENS_FOR_EXACT => {
                self.decode_v2_swap(tx_hash, data)
            }
            SEL_V2_SWAP_ETH_IN => self.decode_v2_eth_in(tx_hash, data),
            SEL_UNIVERSAL_EXECUTE => self.decode_universal(tx_hash, data),
            _ => None,
        }
    }

    /// Decode UniswapV3 exactInputSingle.
    /// ABI: exactInputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160))
    fn decode_v3_exact_input_single(
        &self,
        tx_hash: TxHash,
        data: &Bytes,
    ) -> Option<SandwichOpportunity> {
        if data.len() < 4 + 32 + 8 * 32 {
            return None;
        }

        let struct_offset = 4 + 32;

        let token_in = Address::from_slice(&data[struct_offset + 12..struct_offset + 32]);
        let token_out = Address::from_slice(&data[struct_offset + 32 + 12..struct_offset + 64]);
        let amount_in = U256::from_be_slice(&data[struct_offset + 128..struct_offset + 160]);
        let amount_out_min = U256::from_be_slice(&data[struct_offset + 160..struct_offset + 192]);

        self.build_opportunity(
            tx_hash,
            PoolType::UniswapV3,
            token_in,
            token_out,
            amount_in,
            amount_out_min,
        )
    }

    /// Decode UniswapV3 exactInput (multi-hop).
    /// ABI: exactInput((bytes path, address recipient, uint256 deadline, uint256 amountIn, uint256 amountOutMinimum))
    fn decode_v3_exact_input(&self, tx_hash: TxHash, data: &Bytes) -> Option<SandwichOpportunity> {
        // 5 static slots after selector
        if data.len() < 4 + 5 * 32 {
            return None;
        }
        let base = 4;
        let amount_in = U256::from_be_slice(&data[base + 96..base + 128]);
        let amount_out_min = U256::from_be_slice(&data[base + 128..base + 160]);
        let path_offset = U256::from_be_slice(&data[base..base + 32]).to::<usize>() + 4;

        if data.len() < path_offset + 32 {
            return None;
        }
        let path_len = U256::from_be_slice(&data[path_offset..path_offset + 32]).to::<usize>();
        if path_len < 43 || data.len() < path_offset + 32 + path_len {
            return None;
        }
        let path_start = path_offset + 32;
        // Path: token(20) fee(3) token(20) [fee token]*
        let token_in = Address::from_slice(&data[path_start..path_start + 20]);
        // Last 20 bytes of path = tokenOut
        let token_out =
            Address::from_slice(&data[path_start + path_len - 20..path_start + path_len]);

        self.build_opportunity(
            tx_hash,
            PoolType::UniswapV3,
            token_in,
            token_out,
            amount_in,
            amount_out_min,
        )
    }

    /// Decode UniswapV2 swapExactTokensForTokens and variants.
    fn decode_v2_swap(&self, tx_hash: TxHash, data: &Bytes) -> Option<SandwichOpportunity> {
        // ABI: swapExactTokensForTokens(uint256,uint256,address[],address,uint256)
        if data.len() < 4 + 5 * 32 {
            return None;
        }

        let offset = 4;
        let amount_in = U256::from_be_slice(&data[offset..offset + 32]);
        let amount_out_min = U256::from_be_slice(&data[offset + 32..offset + 64]);
        let path_offset_val = U256::from_be_slice(&data[offset + 64..offset + 96]);
        let path_offset = path_offset_val.to::<usize>() + 4;

        if data.len() < path_offset + 32 {
            return None;
        }

        let path_len = U256::from_be_slice(&data[path_offset..path_offset + 32]).to::<usize>();

        if path_len < 2 || data.len() < path_offset + 32 + path_len * 32 {
            return None;
        }

        let token_in = Address::from_slice(&data[path_offset + 32 + 12..path_offset + 64]);
        let token_out = Address::from_slice(
            &data[path_offset + 32 + (path_len - 1) * 32 + 12..path_offset + 32 + path_len * 32],
        );

        self.build_opportunity(
            tx_hash,
            PoolType::UniswapV2,
            token_in,
            token_out,
            amount_in,
            amount_out_min,
        )
    }

    /// Decode swapExactETHForTokens(uint amountOutMin, address[] path, ...).
    /// amountIn (ETH value) is not in calldata, so we use amountOutMin as
    /// a size proxy and mark protection looseness accordingly.
    fn decode_v2_eth_in(&self, tx_hash: TxHash, data: &Bytes) -> Option<SandwichOpportunity> {
        if data.len() < 4 + 4 * 32 {
            return None;
        }
        let base = 4;
        let amount_out_min = U256::from_be_slice(&data[base..base + 32]);
        let path_offset = U256::from_be_slice(&data[base + 32..base + 64]).to::<usize>() + 4;
        if data.len() < path_offset + 32 {
            return None;
        }
        let path_len = U256::from_be_slice(&data[path_offset..path_offset + 32]).to::<usize>();
        if path_len < 2 || data.len() < path_offset + 32 + path_len * 32 {
            return None;
        }
        let token_in = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
            .parse::<Address>()
            .unwrap(); // WETH
        let token_out = Address::from_slice(
            &data[path_offset + 32 + (path_len - 1) * 32 + 12..path_offset + 32 + path_len * 32],
        );
        // Use amountOutMin (normalized later) as size proxy for ETH in.
        self.build_opportunity(
            tx_hash,
            PoolType::UniswapV2,
            token_in,
            token_out,
            amount_out_min,
            amount_out_min,
        )
    }

    /// Universal Router `execute`: opaque sub-commands. We extract a coarse
    /// size signal (calldata length + non-zero value hint) and only flag
    /// very large inputs to avoid false positives.
    fn decode_universal(&self, tx_hash: TxHash, data: &Bytes) -> Option<SandwichOpportunity> {
        if data.len() < 800 {
            return None; // ignore small router calls
        }
        // Attribute to WETH->USDC whale proxy; router-level exact tokens
        // require full command decoding (future work).
        let weth: Address = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
            .parse()
            .unwrap();
        let usdc: Address = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"
            .parse()
            .unwrap();
        let size_proxy = U256::from(WHALE_18); // large router call
        self.build_opportunity(
            tx_hash,
            PoolType::UniswapV3,
            weth,
            usdc,
            size_proxy,
            U256::ZERO,
        )
    }

    fn build_opportunity(
        &self,
        tx_hash: TxHash,
        protocol: PoolType,
        token_in: Address,
        token_out: Address,
        amount_in: U256,
        min_amount_out: U256,
    ) -> Option<SandwichOpportunity> {
        if amount_in.is_zero() && min_amount_out.is_zero() {
            return None;
        }
        let norm_in = self.normalize_to_18(amount_in, self.decimals_of(token_in));
        let no_protection = min_amount_out.is_zero();
        let norm_u128: u128 = norm_in.to::<u128>();

        let is_whale = norm_u128 >= WHALE_18;
        let is_dust = norm_u128 >= DUST_18;
        let is_actionable = is_whale || (no_protection && is_dust);

        // Protection looseness score for observability (not a price ratio).
        let slippage_bps = if no_protection {
            10_000
        } else if is_whale {
            500
        } else if is_dust {
            100
        } else {
            0
        };

        Some(SandwichOpportunity {
            tx_hash,
            protocol,
            token_in,
            token_out,
            amount_in,
            min_amount_out,
            slippage_bps,
            is_actionable,
        })
    }

    fn decimals_of(&self, token: Address) -> u8 {
        self.decimals.get(&token).map(|d| *d).unwrap_or(18)
    }

    fn normalize_to_18(&self, amount: U256, decimals: u8) -> U256 {
        match decimals.cmp(&18) {
            std::cmp::Ordering::Less => amount * U256::from(10u64.pow(18 - decimals as u32)),
            std::cmp::Ordering::Greater => amount / U256::from(10u64.pow(decimals as u32 - 18)),
            std::cmp::Ordering::Equal => amount,
        }
    }
}
