//! Core domain types for the MEV arbitrage engine.
//!
//! All monetary values use `U256` to prevent precision loss. Addresses use
//! `Address` from alloy-primitives. No floating-point is used anywhere in the
//! hot path — all fraction math uses basis-point integers.

use alloy_primitives::{Address, Bytes, TxHash, U256};
use serde::{Deserialize, Serialize};
use std::fmt;

// ─── Pool Types ──────────────────────────────────────────────────────────────

/// Represents the type of DEX pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PoolType {
    UniswapV2,
    UniswapV3,
    SushiswapV2,
    CurveStableSwap,
    BalancerWeighted,
}

impl fmt::Display for PoolType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UniswapV2 => write!(f, "UniswapV2"),
            Self::UniswapV3 => write!(f, "UniswapV3"),
            Self::SushiswapV2 => write!(f, "SushiswapV2"),
            Self::CurveStableSwap => write!(f, "Curve"),
            Self::BalancerWeighted => write!(f, "Balancer"),
        }
    }
}

/// On-chain state of a liquidity pool at a specific block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolState {
    pub address: Address,
    pub pool_type: PoolType,
    pub token0: Address,
    pub token1: Address,
    /// Reserve of token0 (V2) or virtual reserve derived from sqrtPriceX96 (V3).
    pub reserve0: U256,
    /// Reserve of token1 (V2) or virtual reserve derived from sqrtPriceX96 (V3).
    pub reserve1: U256,
    /// Fee in basis points × 100 (e.g., 3000 = 0.30%).
    pub fee_bps_x100: u32,
    /// V3-specific: sqrtPriceX96 from slot0().
    pub sqrt_price_x96: Option<U256>,
    /// V3-specific: active liquidity in the current tick range.
    pub liquidity: Option<u128>,
    /// V3-specific: current tick.
    pub tick: Option<i32>,
    /// Block number at which this state was fetched.
    pub block_number: u64,
}

impl PoolState {
    /// Compute the price of token1 in terms of token0, scaled by 1e18.
    pub fn price_1_in_0_scaled(&self) -> U256 {
        if self.reserve0.is_zero() {
            return U256::ZERO;
        }
        let scale = U256::from(10u64.pow(18));
        (self.reserve1 * scale) / self.reserve0
    }

    /// Fee as a fraction in millionths (e.g., 3000 → 997000 multiplier).
    pub fn fee_complement_millionths(&self) -> U256 {
        U256::from(1_000_000u64 - u64::from(self.fee_bps_x100))
    }
}

// ─── Token Metadata ──────────────────────────────────────────────────────────

/// Cached ERC-20 token metadata for decimal-aware calculations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMeta {
    pub address: Address,
    pub symbol: String,
    pub decimals: u8,
}

// ─── Arbitrage Route ─────────────────────────────────────────────────────────

/// A single swap leg in a multi-hop arbitrage route.
#[derive(Debug, Clone)]
pub struct SwapLeg {
    pub pool: PoolState,
    pub token_in: Address,
    pub token_out: Address,
    pub amount_in: U256,
    pub expected_amount_out: U256,
}

/// A complete arbitrage route: a cycle through 2+ pools that returns to the starting token.
#[derive(Debug, Clone)]
pub struct ArbitrageRoute {
    /// The token we borrow via flash loan and must return.
    pub base_token: Address,
    /// Ordered sequence of swap legs forming a cycle.
    pub legs: Vec<SwapLeg>,
    /// Expected gross profit in base_token units (before gas + miner reward).
    pub expected_gross_profit: U256,
    /// Optimal flash loan size determined by binary search.
    pub optimal_loan_size: U256,
    /// Combined confidence score [0.0, 1.0] from volatility model.
    pub confidence: f64,
}

impl ArbitrageRoute {
    pub fn num_hops(&self) -> usize {
        self.legs.len()
    }
}

// ─── Bundle & Execution ──────────────────────────────────────────────────────

/// An action to execute inside the ArbitrageExecutor contract.
#[derive(Debug, Clone)]
pub struct ExecutorAction {
    pub target: Address,
    pub value: U256,
    pub data: Vec<u8>,
    pub approve_token: Address,
    pub approve_amount: U256,
}

/// A Flashbots bundle ready for relay submission.
#[derive(Debug, Clone)]
pub struct FlashbotsBundle {
    /// The target (victim) transaction to backrun.
    pub target_tx_hash: TxHash,
    /// Raw signed transactions in bundle order: [target_tx, arb_tx].
    pub signed_txs: Vec<Bytes>,
    /// Target block number for inclusion.
    pub target_block: u64,
    /// Miner reward (block.coinbase payment) in wei.
    pub miner_reward: U256,
    /// Expected net profit after miner reward and gas.
    pub expected_net_profit: U256,
}

/// Outcome of a bundle submission attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BundleOutcome {
    Included { block: u64 },
    BlockMissed { block: u64 },
    SimulationFailed { reason: String },
    RelayError { reason: String },
    TargetAlreadyConfirmed { block: u64 },
    Aborted { reason: String },
}

impl fmt::Display for BundleOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Included { block } => write!(f, "INCLUDED@{block}"),
            Self::BlockMissed { block } => write!(f, "MISSED@{block}"),
            Self::SimulationFailed { reason } => write!(f, "SIM_FAILED:{reason}"),
            Self::RelayError { reason } => write!(f, "RELAY_ERR:{reason}"),
            Self::TargetAlreadyConfirmed { block } => write!(f, "TARGET_CONFIRMED@{block}"),
            Self::Aborted { reason } => write!(f, "ABORTED:{reason}"),
        }
    }
}

// ─── Bidding ─────────────────────────────────────────────────────────────────

/// Result of the dynamic bidding computation.
#[derive(Debug, Clone)]
pub struct BiddingDecision {
    /// ETH to pay block.coinbase.
    pub miner_reward: U256,
    /// Minimum acceptable profit (abort if below).
    pub min_profit: U256,
    /// The gas-pressure-adjusted miner fraction, for logging.
    pub effective_miner_fraction_bps: u32,
}

// ─── Opportunity ─────────────────────────────────────────────────────────────

/// Decoded information about a high-slippage swap detected in the mempool.
#[derive(Debug, Clone)]
pub struct SandwichOpportunity {
    pub tx_hash: TxHash,
    pub protocol: PoolType,
    pub token_in: Address,
    pub token_out: Address,
    pub amount_in: U256,
    pub min_amount_out: U256,
    /// Slippage tolerance in basis points (e.g., 500 = 5%).
    pub slippage_bps: u32,
    /// Whether slippage is high enough to be actionable.
    pub is_actionable: bool,
}

// ─── MEV-Share ───────────────────────────────────────────────────────────────

/// A hint from the Flashbots MEV-Share event stream.
#[derive(Debug, Clone)]
pub struct MevShareHint {
    pub hash: TxHash,
    pub to: Option<Address>,
    pub calldata: Option<Vec<u8>>,
    pub logs: Option<Vec<MevShareLog>>,
    pub gas_used: Option<U256>,
    pub mev_gas_price: Option<U256>,
}

/// A log entry from an MEV-Share hint.
#[derive(Debug, Clone)]
pub struct MevShareLog {
    pub address: Address,
    pub topics: Vec<[u8; 32]>,
    pub data: Vec<u8>,
}

// ─── Metrics Snapshot ────────────────────────────────────────────────────────

/// Point-in-time snapshot of engine metrics for health reporting.
#[derive(Debug, Clone, Default)]
pub struct EngineMetrics {
    pub txs_scanned: u64,
    pub opportunities_found: u64,
    pub bundles_submitted: u64,
    pub bundles_included: u64,
    pub total_profit_wei: U256,
    pub total_gas_spent_wei: U256,
    pub rolling_pnl_wei: i128,
    pub uptime_seconds: u64,
}
