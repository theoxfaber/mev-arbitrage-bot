//! MEV Arbitrage Engine — Main Orchestrator
//!
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                         ENGINE PIPELINE                                │
//! │                                                                        │
//! │   ┌──────────┐   ┌──────────┐   ┌───────────┐   ┌──────────────────┐  │
//! │   │ Mempool  │──▶│  Route   │──▶│ Simulator │──▶│  Flashbots      │  │
//! │   │ Scanner  │   │ Discovery│   │  (revm)   │   │  Relayer        │  │
//! │   └──────────┘   │(Bellman- │   │  Binary   │   │  Multi-relay    │  │
//! │   ┌──────────┐   │  Ford)   │   │  Search   │   │  Bid Escalation │  │
//! │   │ MEV-Share│──▶│          │   │           │   │                 │  │
//! │   │ Scanner  │   └──────────┘   └───────────┘   └──────────────────┘  │
//! │   └──────────┘                                                        │
//! │                                                                        │
//! │   ┌──────────────────────────────────────────────────────────────────┐ │
//! │   │  Observability: Prometheus │ Tracing │ SQLite PnL │ Telegram   │ │
//! │   └──────────────────────────────────────────────────────────────────┘ │
//! └─────────────────────────────────────────────────────────────────────────┘
//!
//! **Architecture**: Lock-free, event-driven Tokio pipeline. Each stage
//! communicates via bounded async channels. Zero mutex contention in the
//! hot path — DashMap for deduplication, atomics for nonce tracking.

use mev_arbitrage_bot::config::Config;
use mev_arbitrage_bot::db::Database;
use mev_arbitrage_bot::executor::{BundleBuilder, FlashbotsRelayer, WalletPool};
use mev_arbitrage_bot::metrics;
use mev_arbitrage_bot::router::ArbitrageRouter;
use mev_arbitrage_bot::scanner::decoder::new_decimals_cache;
use mev_arbitrage_bot::scanner::{MempoolScanner, MevShareScanner};
use mev_arbitrage_bot::simulator::EvmSimulator;
use mev_arbitrage_bot::types::SandwichOpportunity;

use alloy_primitives::{Address, U256};
use eyre::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;

/// Engine-wide kill switch (atomic for lock-free access).
static KILL_SWITCH: AtomicBool = AtomicBool::new(false);

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize color-eyre for rich error reports
    color_eyre::install()?;

    // Load and validate configuration
    let config = Config::load()?;

    // Initialize structured logging
    init_logging(&config);

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        wallets = config.private_keys.len(),
        dry_run = config.cli.dry_run,
        "╔══════════════════════════════════════════════════════╗"
    );
    tracing::info!(
        "║  MEV Arbitrage Engine v{}                        ║",
        env!("CARGO_PKG_VERSION")
    );
    tracing::info!("║  Rust-native · revm Simulation · Bellman-Ford       ║");
    tracing::info!("╚══════════════════════════════════════════════════════╝");

    // Initialize Prometheus metrics server
    metrics::init_metrics_server(config.cli.metrics_port)?;

    // Initialize database
    let db = Arc::new(Database::open("./pnl.sqlite")?);

    // Initialize components
    let decimals_cache = new_decimals_cache();

    // Anchor tokens for Bellman-Ford route discovery
    let anchor_tokens: Vec<Address> = vec![
        "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
            .parse()
            .unwrap(), // WETH
        "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"
            .parse()
            .unwrap(), // USDC
        "0xdAC17F958D2ee523a2206206994597C13D831ec7"
            .parse()
            .unwrap(), // USDT
    ];

    // Optimize: Pin main orchestrator to core 0
    mev_arbitrage_bot::latency::pin_to_core(0);

    let router = Arc::new(ArbitrageRouter::new(anchor_tokens));
    let simulator = Arc::new(EvmSimulator::new());
    let bidding_engine = Arc::new(mev_arbitrage_bot::executor::BiddingEngine::new(
        config.min_profit_bps,
        config.base_miner_reward_bps,
        config.max_miner_reward_bps,
        config.reference_gas_price_gwei,
    ));
    let bundle_builder = Arc::new(BundleBuilder::new(config.executor_contract));
    let relayer = Arc::new(FlashbotsRelayer::new(config.flashbots_auth_key.clone()));
    let wallet_pool = Arc::new(WalletPool::new(&config.private_keys)?);

    // Set initial kill switch state
    KILL_SWITCH.store(config.kill_switch, Ordering::Relaxed);

    // Create the opportunity channel (bounded to apply backpressure)
    let (opportunity_tx, mut opportunity_rx) = mpsc::channel::<SandwichOpportunity>(256);

    // Start mempool scanner
    let mut ws_urls = vec![config.rpc_ws_url.clone()];
    if let Some(url2) = &config.rpc_ws_url_2 {
        ws_urls.push(url2.clone());
    }
    let mempool_scanner = MempoolScanner::new(ws_urls, decimals_cache.clone());
    mempool_scanner.start(opportunity_tx.clone()).await?;

    // Start Event (Log) scanner
    let event_scanner = Arc::new(mev_arbitrage_bot::scanner::EventScanner::new(
        opportunity_tx.clone(),
    ));
    let _event_scanner_spawn = Arc::clone(&event_scanner);
    tokio::spawn(async move {
        // In a real implementation, this would subscribe to eth_subscribe("logs")
        tracing::info!("Event log scanner started");
    });

    // Start MEV-Share scanner
    let (mev_share_tx, mut _mev_share_rx) = mpsc::channel(64);
    let mev_share_scanner = MevShareScanner::new();
    mev_share_scanner.start(mev_share_tx).await?;

    // Start background nonce synchronizer
    let wallet_pool_sync = Arc::clone(&wallet_pool);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            wallet_pool_sync.sync_nonces();
        }
    });

    // Start circuit breaker monitor
    let db_monitor = Arc::clone(&db);
    let config_monitor = config.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
        loop {
            interval.tick().await;
            let pnl = db_monitor.rolling_pnl(config_monitor.circuit_breaker_window_minutes);
            let pnl_eth = pnl as f64 / 1e18;
            metrics::set_rolling_pnl_eth(pnl_eth);

            if pnl < -(config_monitor.circuit_breaker_max_loss_wei as i128) {
                tracing::error!(
                    rolling_pnl_eth = pnl_eth,
                    threshold_eth = -(config_monitor.circuit_breaker_max_loss_wei as f64 / 1e18),
                    "🚨 CIRCUIT BREAKER TRIPPED — halting execution"
                );
                KILL_SWITCH.store(true, Ordering::Relaxed);
                metrics::record_circuit_breaker_trip();
            }
        }
    });

    // Start uptime tracker
    let start_time = Instant::now();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
        loop {
            interval.tick().await;
            metrics::set_uptime(start_time.elapsed().as_secs());
        }
    });

    tracing::info!(
        wallets = wallet_pool.count(),
        pools = router.pool_count(),
        "Engine started — scanning mempool for opportunities"
    );

    // ── Main Event Loop ──────────────────────────────────────────────────

    while let Some(opportunity) = opportunity_rx.recv().await {
        // Kill switch check (atomic, zero overhead)
        if KILL_SWITCH.load(Ordering::Relaxed) {
            tracing::warn!("Kill switch active — skipping opportunity");
            continue;
        }

        // Check if the opportunity is worth pursuing
        if !opportunity.is_actionable {
            continue;
        }

        let router = Arc::clone(&router);
        let simulator = Arc::clone(&simulator);
        let bidding = Arc::clone(&bidding_engine);
        let builder = Arc::clone(&bundle_builder);
        let relayer = Arc::clone(&relayer);
        let db = Arc::clone(&db);
        let dry_run = config.cli.dry_run;

        // Process each opportunity in its own task (bounded by channel backpressure)
        tokio::spawn(async move {
            if let Err(e) = process_opportunity(
                opportunity,
                &router,
                &simulator,
                &bidding,
                &builder,
                &relayer,
                &db,
                dry_run,
            )
            .await
            {
                tracing::debug!(error = %e, "Opportunity processing failed");
            }
        });
    }

    tracing::info!("Engine shutting down");
    Ok(())
}

/// Process a single arbitrage opportunity through the full pipeline.
#[allow(clippy::too_many_arguments)]
async fn process_opportunity(
    opportunity: SandwichOpportunity,
    router: &ArbitrageRouter,
    simulator: &EvmSimulator,
    bidding: &mev_arbitrage_bot::executor::BiddingEngine,
    builder: &BundleBuilder,
    relayer: &FlashbotsRelayer,
    db: &Database,
    dry_run: bool,
) -> Result<()> {
    let span = tracing::info_span!("opportunity", tx = %opportunity.tx_hash);
    let _guard = span.enter();

    tracing::info!(
        protocol = %opportunity.protocol,
        slippage_bps = opportunity.slippage_bps,
        amount_in = %opportunity.amount_in,
        "Processing opportunity"
    );

    // 1. Route discovery — find the best arbitrage cycle
    let routes = router.find_arbitrage_routes();
    if routes.is_empty() {
        tracing::debug!("No profitable routes found");
        return Ok(());
    }

    let best_route = &routes[0];
    tracing::info!(
        hops = best_route.num_hops(),
        confidence = best_route.confidence,
        "Route discovered"
    );

    // 2. Simulate the trade to find optimal loan size
    let sim_result = simulator.simulate(best_route)?;

    if sim_result.gross_profit.is_zero() {
        tracing::debug!("Simulation shows zero profit");
        return Ok(());
    }

    tracing::info!(
        optimal_loan = %sim_result.optimal_loan_size,
        gross_profit = %sim_result.gross_profit,
        gas_used = sim_result.gas_used,
        sim_ms = sim_result.simulation_duration_ms,
        "Simulation complete"
    );

    // 3. Compute dynamic bidding
    // Use 20 gwei as a baseline for now (would be fetched from the latest block)
    let base_fee = U256::from(20_000_000_000u64);
    let bid = bidding.compute(sim_result.gross_profit, base_fee, sim_result.gas_used);

    if bid.miner_reward.is_zero() {
        tracing::debug!("Unprofitable after gas costs");
        return Ok(());
    }

    tracing::info!(
        miner_reward = %bid.miner_reward,
        min_profit = %bid.min_profit,
        effective_bps = bid.effective_miner_fraction_bps,
        "Bid computed"
    );

    // 4. Build the bundle
    let target_block = 0u64; // Would be fetched from the latest block number + 1
    let mut bundle = builder.build(
        best_route,
        &sim_result,
        opportunity.tx_hash,
        target_block,
        bid.miner_reward,
        bid.min_profit,
    );

    if dry_run {
        tracing::info!(
            profit = %sim_result.gross_profit,
            "DRY RUN — bundle built but not submitted"
        );
        return Ok(());
    }

    // 5. Submit to relays with bid escalation
    let results = relayer.submit_with_escalation(&mut bundle).await;

    // Log results
    for (relay, outcome) in &results {
        tracing::info!(relay = relay, outcome = %outcome, "Relay result");
    }

    // 6. Log to database
    let status = results
        .first()
        .map(|(_, o)| o.to_string())
        .unwrap_or_else(|| "UNKNOWN".to_string());

    let profit_i128 = if sim_result.gross_profit > U256::from(u128::MAX) {
        i128::MAX
    } else {
        sim_result.gross_profit.to::<u128>() as i128
    };

    db.log_bundle(
        &format!("{:?}", opportunity.tx_hash),
        target_block,
        &status,
        bid.miner_reward.to::<u128>(),
        profit_i128,
        sim_result.gas_used,
        best_route.num_hops(),
        &format!("{:?}", best_route.base_token),
    );

    let profit_eth = sim_result.gross_profit.to::<u128>() as f64 / 1e18;
    metrics::record_profit_eth(profit_eth);

    Ok(())
}

/// Initialize structured logging with tracing.
fn init_logging(config: &Config) {
    use tracing_subscriber::{fmt, EnvFilter};

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.cli.log_level));

    if config.cli.log_json {
        fmt()
            .with_env_filter(filter)
            .json()
            .with_target(true)
            .with_thread_ids(true)
            .init();
    } else {
        fmt()
            .with_env_filter(filter)
            .with_target(false)
            .with_thread_ids(false)
            .compact()
            .init();
    }
}
