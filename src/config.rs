//! Configuration management — loads from environment variables via `dotenvy`
//! with CLI overrides via `clap`. All secrets are validated at startup; the
//! engine refuses to start with missing critical configuration.

use alloy_primitives::Address;
use clap::Parser;
use eyre::{ensure, Result, WrapErr};
use std::str::FromStr;

/// MEV Arbitrage Engine — production-grade Rust MEV bot.
#[derive(Parser, Debug, Clone)]
#[command(
    name = "mev-engine",
    version,
    about = "Production-grade MEV arbitrage engine for Ethereum"
)]
pub struct CliArgs {
    /// Log level (trace, debug, info, warn, error)
    #[arg(long, env = "LOG_LEVEL", default_value = "info")]
    pub log_level: String,

    /// Enable JSON-formatted log output (for production log aggregation)
    #[arg(long, env = "LOG_JSON", default_value = "false")]
    pub log_json: bool,

    /// Prometheus metrics HTTP port (0 = disabled)
    #[arg(long, env = "METRICS_PORT", default_value = "9090")]
    pub metrics_port: u16,

    /// Dry-run mode: simulate but do not submit bundles
    #[arg(long, env = "DRY_RUN", default_value = "false")]
    pub dry_run: bool,
}

/// Fully validated engine configuration.
#[derive(Debug, Clone)]
pub struct Config {
    // ── RPC Endpoints ────────────────────────────────────────────────────
    /// Primary HTTP RPC (used for simulation, state queries, and execution).
    pub rpc_http_url: String,
    /// Secondary HTTP RPC (fallback).
    pub rpc_http_url_2: Option<String>,
    /// Primary WebSocket RPC (used for mempool scanning).
    pub rpc_ws_url: String,
    /// Secondary WebSocket RPC (enables multi-RPC racing).
    pub rpc_ws_url_2: Option<String>,

    // ── Wallet ───────────────────────────────────────────────────────────
    /// Comma-separated private keys for executor wallets.
    pub private_keys: Vec<String>,
    /// Flashbots auth signer key (does NOT need funds).
    pub flashbots_auth_key: String,

    // ── Contract ─────────────────────────────────────────────────────────
    /// Deployed ArbitrageExecutor contract address.
    pub executor_contract: Address,

    // ── Bidding ──────────────────────────────────────────────────────────
    /// Minimum profit fraction in basis points (e.g., 3000 = 30%).
    pub min_profit_bps: u32,
    /// Base miner reward fraction at reference gas price (bps).
    pub base_miner_reward_bps: u32,
    /// Maximum miner reward fraction (bps).
    pub max_miner_reward_bps: u32,
    /// Reference gas price in gwei for bidding baseline.
    pub reference_gas_price_gwei: u64,

    // ── Circuit Breaker ──────────────────────────────────────────────────
    /// Rolling PnL window in minutes.
    pub circuit_breaker_window_minutes: u64,
    /// Maximum loss threshold in wei (positive number; triggers at -threshold).
    pub circuit_breaker_max_loss_wei: u128,
    /// Kill switch — halts all execution immediately.
    pub kill_switch: bool,

    // ── Telegram ─────────────────────────────────────────────────────────
    pub telegram_bot_token: Option<String>,
    pub telegram_chat_id: Option<String>,

    // ── CLI ──────────────────────────────────────────────────────────────
    pub cli: CliArgs,
}

impl Config {
    /// Loads configuration from environment variables (via `.env` file) and CLI args.
    /// Validates all required fields and returns a fully populated `Config`.
    pub fn load() -> Result<Self> {
        // Load .env file if present (non-fatal if missing)
        let _ = dotenvy::dotenv();

        let cli = CliArgs::parse();

        let rpc_http_url = env_required("ETH_RPC_URL_1")?;
        let rpc_ws_url = env_required("ETH_WSS_URL_1")?;
        let private_keys_raw = env_required("PRIVATE_KEYS")?;
        let flashbots_auth_key = env_required("FLASHBOTS_AUTH_KEY")?;
        let executor_contract_raw = env_required("EXECUTOR_CONTRACT_ADDRESS")?;

        let private_keys: Vec<String> = private_keys_raw
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        ensure!(
            !private_keys.is_empty(),
            "PRIVATE_KEYS must contain at least one key"
        );

        let executor_contract = Address::from_str(&executor_contract_raw)
            .wrap_err("Invalid EXECUTOR_CONTRACT_ADDRESS")?;

        Ok(Config {
            rpc_http_url,
            rpc_http_url_2: env_optional("ETH_RPC_URL_2"),
            rpc_ws_url,
            rpc_ws_url_2: env_optional("ETH_WSS_URL_2"),
            private_keys,
            flashbots_auth_key,
            executor_contract,
            min_profit_bps: env_parse("MIN_PROFIT_BPS", 3000),
            base_miner_reward_bps: env_parse("BASE_MINER_REWARD_BPS", 2500),
            max_miner_reward_bps: env_parse("MAX_MINER_REWARD_BPS", 6500),
            reference_gas_price_gwei: env_parse("REFERENCE_GAS_PRICE_GWEI", 20),
            circuit_breaker_window_minutes: env_parse("CIRCUIT_BREAKER_WINDOW_MINUTES", 60),
            circuit_breaker_max_loss_wei: env_parse(
                "CIRCUIT_BREAKER_MAX_LOSS_WEI",
                500_000_000_000_000_000u128, // 0.5 ETH
            ),
            kill_switch: env_parse("KILL_SWITCH", false),
            telegram_bot_token: env_optional("TELEGRAM_BOT_TOKEN"),
            telegram_chat_id: env_optional("TELEGRAM_CHAT_ID"),
            cli,
        })
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn env_required(key: &str) -> Result<String> {
    std::env::var(key).wrap_err_with(|| format!("Missing required env var: {key}"))
}

fn env_optional(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.is_empty())
}

fn env_parse<T: FromStr + Default>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse::<T>().ok())
        .unwrap_or(default)
}
