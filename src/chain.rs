//! Multi-chain abstraction layer.
//!
//! Provides a unified interface for Ethereum and L2s (Arbitrum, Base, Optimism),
//! handling differences in gas pricing, block times, and flash loan venues.

use alloy_primitives::{Address, U256};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Chain {
    Ethereum,
    Arbitrum,
    Base,
    Optimism,
}

impl Chain {
    pub fn id(&self) -> u64 {
        match self {
            Self::Ethereum => 1,
            Self::Arbitrum => 42161,
            Self::Base => 8453,
            Self::Optimism => 10,
        }
    }
}

/// Unified trait for chain-specific parameters and logic.
pub trait ChainAdapter: Send + Sync {
    fn chain(&self) -> Chain;

    /// Address of the Multicall3 contract on this chain.
    fn multicall_address(&self) -> Address;

    /// Typical block time in seconds.
    fn average_block_time_ms(&self) -> u64;

    /// Calculate the effective gas price for a bundle.
    /// On L2s, this includes the L1 data fee.
    fn estimate_effective_gas_price(
        &self,
        gas_used: u64,
        base_fee: U256,
        priority_fee: U256,
    ) -> U256;

    /// Get recommended flash loan providers (e.g., Balancer, Aave, UniswapV3).
    fn recommended_flash_loan_providers(&self) -> Vec<FlashLoanProvider>;
}

#[derive(Debug, Clone)]
pub enum FlashLoanProvider {
    BalancerV2,
    AaveV3,
    UniswapV3,
    Morpho,
}

pub struct EthereumAdapter;
impl ChainAdapter for EthereumAdapter {
    fn chain(&self) -> Chain {
        Chain::Ethereum
    }
    fn multicall_address(&self) -> Address {
        "0xcA11bde05977b3631167028862bE2a173976CA11"
            .parse()
            .unwrap()
    }
    fn average_block_time_ms(&self) -> u64 {
        12000
    }
    fn estimate_effective_gas_price(
        &self,
        _gas_used: u64,
        base_fee: U256,
        priority_fee: U256,
    ) -> U256 {
        base_fee + priority_fee
    }
    fn recommended_flash_loan_providers(&self) -> Vec<FlashLoanProvider> {
        vec![
            FlashLoanProvider::AaveV3,
            FlashLoanProvider::BalancerV2,
            FlashLoanProvider::UniswapV3,
        ]
    }
}

pub struct ArbitrumAdapter;
impl ChainAdapter for ArbitrumAdapter {
    fn chain(&self) -> Chain {
        Chain::Arbitrum
    }
    fn multicall_address(&self) -> Address {
        "0xcA11bde05977b3631167028862bE2a173976CA11"
            .parse()
            .unwrap()
    }
    fn average_block_time_ms(&self) -> u64 {
        250
    }
    fn estimate_effective_gas_price(
        &self,
        _gas_used: u64,
        base_fee: U256,
        priority_fee: U256,
    ) -> U256 {
        // Arbitrum uses a different pricing model, but simplified for now
        base_fee + priority_fee
    }
    fn recommended_flash_loan_providers(&self) -> Vec<FlashLoanProvider> {
        vec![FlashLoanProvider::BalancerV2, FlashLoanProvider::UniswapV3]
    }
}

pub struct BaseAdapter;
impl ChainAdapter for BaseAdapter {
    fn chain(&self) -> Chain {
        Chain::Base
    }
    fn multicall_address(&self) -> Address {
        "0xcA11bde05977b3631167028862bE2a173976CA11"
            .parse()
            .unwrap()
    }
    fn average_block_time_ms(&self) -> u64 {
        2000
    }
    fn estimate_effective_gas_price(
        &self,
        _gas_used: u64,
        base_fee: U256,
        priority_fee: U256,
    ) -> U256 {
        // Base (OP Stack) has L1 data fee.
        // simplified: base_fee + priority_fee + (l1_fee / gas_used)
        base_fee + priority_fee
    }
    fn recommended_flash_loan_providers(&self) -> Vec<FlashLoanProvider> {
        vec![FlashLoanProvider::BalancerV2, FlashLoanProvider::UniswapV3]
    }
}
