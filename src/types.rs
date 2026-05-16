//! Core domain types for the MEV arbitrage engine.

use alloy_primitives::{Address, Bytes, TxHash, U256};
use alloy_sol_types::sol;
use serde::{Deserialize, Serialize};
use std::fmt;

sol! {
    struct Action {
        address target;
        uint256 value;
        bytes data;
        address approveToken;
        uint256 approveAmount;
    }
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickInfo {
    pub liquidity_gross: u128,
    pub liquidity_net: i128,
    pub initialized: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SwapResult {
    pub amount_out: U256,
    pub amount_in: U256,
    pub fee_paid: U256,
    pub ticks_crossed: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PoolState {
    UniswapV2 {
        address: Address,
        token0: Address,
        token1: Address,
        reserve0: u128,
        reserve1: u128,
        fee_bps: u32,
    },
    UniswapV3 {
        address: Address,
        token0: Address,
        token1: Address,
        sqrt_price_x96: U256,
        liquidity: u128,
        tick: i32,
        tick_spacing: i32,
        fee: u32,
        tick_bitmap: std::collections::HashMap<i16, U256>,
        ticks: std::collections::HashMap<i32, TickInfo>,
    },
    Curve {
        address: Address,
        tokens: Vec<Address>,
        balances: Vec<U256>,
        amp: U256,
        n_coins: usize,
        fee_bps: u32,
    },
    Balancer {
        address: Address,
        tokens: Vec<Address>,
        balances: Vec<U256>,
        weights: Vec<U256>,
        fee_bps: u32,
    },
}

impl PoolState {
    pub fn address(&self) -> Address {
        match self {
            Self::UniswapV2 { address, .. } => *address,
            Self::UniswapV3 { address, .. } => *address,
            Self::Curve { address, .. } => *address,
            Self::Balancer { address, .. } => *address,
        }
    }

    pub fn token0(&self) -> Address {
        match self {
            Self::UniswapV2 { token0, .. } => *token0,
            Self::UniswapV3 { token0, .. } => *token0,
            Self::Curve { tokens, .. } => tokens[0],
            Self::Balancer { tokens, .. } => tokens[0],
        }
    }

    pub fn token1(&self) -> Address {
        match self {
            Self::UniswapV2 { token1, .. } => *token1,
            Self::UniswapV3 { token1, .. } => *token1,
            Self::Curve { tokens, .. } => tokens[1],
            Self::Balancer { tokens, .. } => tokens[1],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMeta {
    pub address: Address,
    pub symbol: String,
    pub decimals: u8,
}

#[derive(Debug, Clone)]
pub struct SwapLeg {
    pub pool: PoolState,
    pub token_in: Address,
    pub token_out: Address,
    pub amount_in: U256,
    pub expected_amount_out: U256,
}

#[derive(Debug, Clone)]
pub struct OptimizedLeg {
    pub pool_address: Address,
    pub token_in: Address,
    pub token_out: Address,
    pub amount_in: U256,
    pub amount_out: U256,
}

#[derive(Debug, Clone)]
pub struct ArbitrageRoute {
    pub base_token: Address,
    pub legs: Vec<SwapLeg>,
    pub expected_gross_profit: U256,
    pub optimal_loan_size: U256,
    pub confidence: f64,
}

impl ArbitrageRoute {
    pub fn num_hops(&self) -> usize {
        self.legs.len()
    }
}

#[derive(Debug, Clone)]
pub struct FlashbotsBundle {
    pub target_tx_hash: TxHash,
    pub signed_txs: Vec<Bytes>,
    pub target_block: u64,
    pub miner_reward: U256,
    pub expected_net_profit: U256,
}

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

#[derive(Debug, Clone)]
pub struct BiddingDecision {
    pub miner_reward: U256,
    pub min_profit: U256,
    pub effective_miner_fraction_bps: u32,
}

#[derive(Debug, Clone)]
pub struct SandwichOpportunity {
    pub tx_hash: TxHash,
    pub protocol: PoolType,
    pub token_in: Address,
    pub token_out: Address,
    pub amount_in: U256,
    pub min_amount_out: U256,
    pub slippage_bps: u32,
    pub is_actionable: bool,
}

pub struct MevShareHint {
    pub hash: TxHash,
    pub to: Option<Address>,
    pub calldata: Option<Vec<u8>>,
    pub logs: Option<Vec<MevShareLog>>,
    pub gas_used: Option<U256>,
    pub mev_gas_price: Option<U256>,
}

pub struct MevShareLog {
    pub address: Address,
    pub topics: Vec<[u8; 32]>,
    pub data: Vec<u8>,
}

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

pub struct PendingTx {
    pub hash: TxHash,
    pub to: Option<Address>,
    pub input: Bytes,
}
