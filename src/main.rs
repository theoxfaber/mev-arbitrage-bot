//! MEV Arbitrage Engine — Main Orchestrator

use mev_arbitrage_bot::config::Config;
use mev_arbitrage_bot::db::Database;
use mev_arbitrage_bot::executor::{BundleBuilder, FlashbotsRelayer, WalletPool};
use mev_arbitrage_bot::metrics;
use mev_arbitrage_bot::router::ArbitrageRouter;
use mev_arbitrage_bot::scanner::decoder::new_decimals_cache;
use mev_arbitrage_bot::scanner::MempoolScanner;
use mev_arbitrage_bot::simulator::EvmSimulator;
use mev_arbitrage_bot::types::SandwichOpportunity;

use alloy_primitives::{Address, U256};
use alloy::providers::{ProviderBuilder, RootProvider};
use alloy::network::{EthereumWallet, Ethereum};
use alloy::transports::http::Http;
use reqwest::Client;
use eyre::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

static KILL_SWITCH: AtomicBool = AtomicBool::new(false);

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let config = Config::load()?;
    init_logging(&config);

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        wallets = config.private_keys.len(),
        dry_run = config.cli.dry_run,
        "Engine starting..."
    );

    metrics::init_metrics_server(config.cli.metrics_port)?;
    let db = Arc::new(Database::open("./pnl.sqlite")?);
    let decimals_cache = new_decimals_cache();

    let anchor_tokens: Vec<Address> = vec![
        "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".parse().unwrap(), // WETH
        "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".parse().unwrap(), // USDC
    ];

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
    let relayer = Arc::new(FlashbotsRelayer::new(config.flashbots_auth_key.clone())?);
    let wallet_pool = Arc::new(WalletPool::new(&config.private_keys)?);

    let provider = Arc::new(ProviderBuilder::new()
        .on_http(config.rpc_http_url.parse()?));

    KILL_SWITCH.store(config.kill_switch, Ordering::Relaxed);
    let (opportunity_tx, mut opportunity_rx) = mpsc::channel::<SandwichOpportunity>(256);

    let mut ws_urls = vec![config.rpc_ws_url.clone()];
    if let Some(url2) = &config.rpc_ws_url_2 {
        ws_urls.push(url2.clone());
    }
    let mempool_scanner = MempoolScanner::new(ws_urls, decimals_cache.clone());
    mempool_scanner.start(opportunity_tx.clone()).await?;

    // Background tasks
    let wallet_pool_sync = Arc::clone(&wallet_pool);
    let provider_sync = Arc::clone(&provider);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            wallet_pool_sync.sync_nonces(&*provider_sync).await;
        }
    });

    // Circuit Breaker Task
    let db_cb = Arc::clone(&db);
    let config_cb = config.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
        loop {
            interval.tick().await;
            let pnl = db_cb.rolling_pnl(config_cb.circuit_breaker_window_minutes);
            if pnl < -(config_cb.circuit_breaker_max_loss_wei as i128) {
                tracing::error!(pnl, "CIRCUIT BREAKER TRIPPED - halting execution");
                KILL_SWITCH.store(true, Ordering::Relaxed);
            }
        }
    });

    // Main Event Loop
    while let Some(opportunity) = opportunity_rx.recv().await {
        if KILL_SWITCH.load(Ordering::Relaxed) { continue; }
        if !opportunity.is_actionable { continue; }

        let router = Arc::clone(&router);
        let simulator = Arc::clone(&simulator);
        let bidding = Arc::clone(&bidding_engine);
        let builder = Arc::clone(&bundle_builder);
        let relayer = Arc::clone(&relayer);
        let db = Arc::clone(&db);
        let wallet_pool = Arc::clone(&wallet_pool);
        let provider = Arc::clone(&provider);
        let dry_run = config.cli.dry_run;

        tokio::spawn(async move {
            if let Err(e) = process_opportunity(
                opportunity,
                &router,
                &simulator,
                &bidding,
                &builder,
                &relayer,
                &db,
                &wallet_pool,
                provider,
                dry_run,
            )
            .await
            {
                tracing::debug!(error = %e, "Opportunity processing failed");
            }
        });
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn process_opportunity(
    opportunity: SandwichOpportunity,
    router: &ArbitrageRouter,
    simulator: &EvmSimulator,
    bidding: &mev_arbitrage_bot::executor::BiddingEngine,
    builder: &BundleBuilder,
    relayer: &FlashbotsRelayer,
    db: &Database,
    wallet_pool: &WalletPool,
    provider: Arc<RootProvider<Http<Client>>>,
    dry_run: bool,
) -> Result<()>
{
    let routes = router.find_arbitrage_routes();
    if routes.is_empty() { return Ok(()); }
    let best_route = &routes[0];

    // Simulate
    // NOTE: In production, we must generate real calldata here.
    let sim_result = simulator.simulate::<Http<Client>, Ethereum, RootProvider<Http<Client>>>(best_route, provider, Address::ZERO, alloy_primitives::Bytes::default()).await?;
    if sim_result.gross_profit.is_zero() { return Ok(()); }

    // Bidding
    let base_fee = U256::from(20_000_000_000u64);
    let bid = bidding.compute(sim_result.gross_profit, base_fee, sim_result.gas_used);
    if bid.miner_reward.is_zero() { return Ok(()); }

    // Build and sign
    let (wallet, nonce) = wallet_pool.execute_with_wallet(|signer, _addr, nonce| {
        let wallet = EthereumWallet::from(signer.clone());
        Ok((wallet, nonce))
    }).await?;

    let signed_bundle = builder.build_and_sign(
        best_route,
        &sim_result,
        opportunity.tx_hash,
        0,
        bid.miner_reward,
        bid.min_profit,
        &wallet,
        nonce,
        1,
        base_fee
    ).await?;

    if dry_run { return Ok(()); }

    let mut bundle_to_submit = signed_bundle;

    let route_clone = best_route.clone();
    let sim_result_clone = sim_result.clone();
    let wallet_clone = wallet.clone();
    let builder_clone = builder.clone();
    let tx_hash = opportunity.tx_hash;
    let nonce_clone = nonce;

    let _results = relayer.submit_with_escalation(&mut bundle_to_submit, |new_reward| {
        let route = route_clone.clone();
        let sim = sim_result_clone.clone();
        let wallet = wallet_clone.clone();
        let builder = builder_clone;

        async move {
            builder.build_and_sign(
                &route,
                &sim,
                tx_hash,
                0,
                new_reward,
                bid.min_profit, // Note: min_profit could also be scaled
                &wallet,
                nonce_clone,
                1,
                base_fee
            ).await
        }
    }).await;

    // Log to DB
    db.log_bundle(
        &format!("{:?}", opportunity.tx_hash),
        0,
        "SUBMITTED",
        bid.miner_reward.to::<u128>(),
        sim_result.gross_profit.to::<u128>() as i128,
        sim_result.gas_used,
        best_route.num_hops(),
        &format!("{:?}", best_route.base_token),
    );

    Ok(())
}

fn init_logging(config: &Config) {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.cli.log_level));
    fmt().with_env_filter(filter).init();
}
