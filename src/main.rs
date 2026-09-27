//! MEV Arbitrage Engine — Main Orchestrator

use mev_arbitrage_bot::config::Config;
use mev_arbitrage_bot::db::Database;
use mev_arbitrage_bot::executor::{BundleBuilder, FlashbotsRelayer, WalletPool};
use mev_arbitrage_bot::metrics;
use mev_arbitrage_bot::router::ArbitrageRouter;
use mev_arbitrage_bot::scanner::decoder::new_decimals_cache;
use mev_arbitrage_bot::scanner::{MempoolScanner, MevShareScanner};
use mev_arbitrage_bot::simulator::EvmSimulator;
use mev_arbitrage_bot::types::{MevShareHint, PoolState, SandwichOpportunity};

use alloy::network::{Ethereum, EthereumWallet};
use alloy::providers::{Provider, ProviderBuilder, RootProvider};
use alloy::transports::http::Http;
use alloy_primitives::{Address, U256};
use eyre::Result;
use reqwest::Client;
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
        "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
            .parse()
            .unwrap(), // WETH
        "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48"
            .parse()
            .unwrap(), // USDC
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

    let provider = Arc::new(ProviderBuilder::new().on_http(config.rpc_http_url.parse()?));

    // Initial nonce sync so we never start from 0.
    wallet_pool.sync_nonces(&*provider).await;

    KILL_SWITCH.store(config.kill_switch, Ordering::Relaxed);
    let (opportunity_tx, mut opportunity_rx) = mpsc::channel::<SandwichOpportunity>(1024);

    let mut ws_urls = vec![config.rpc_ws_url.clone()];
    if let Some(url2) = &config.rpc_ws_url_2 {
        ws_urls.push(url2.clone());
    }
    let mempool_scanner = MempoolScanner::new(ws_urls, decimals_cache.clone());
    mempool_scanner.start(opportunity_tx.clone()).await?;

    // MEV-Share private flow (optional, never crashes main loop).
    {
        let mev_scanner = MevShareScanner::new();
        let (mev_tx, mut mev_rx) = mpsc::channel::<MevShareHint>(256);
        if let Err(e) = mev_scanner.start(mev_tx).await {
            tracing::warn!(error = %e, "MEV-Share disabled");
        } else {
            let opp_tx = opportunity_tx.clone();
            tokio::spawn(async move {
                while let Some(hint) = mev_rx.recv().await {
                    if let (Some(to), Some(calldata)) = (hint.to, hint.calldata) {
                        // Re-use size heuristic: forward as generic opportunity.
                        use alloy_primitives::{Bytes, TxHash};
                        let _ = opp_tx.try_send(SandwichOpportunity {
                            tx_hash: hint.hash,
                            protocol: mev_arbitrage_bot::types::PoolType::UniswapV3,
                            token_in: Address::ZERO,
                            token_out: to,
                            amount_in: U256::from(500_000_000_000_000_000u128),
                            min_amount_out: U256::ZERO,
                            slippage_bps: 10_000,
                            is_actionable: true,
                        });
                        let _ = (TxHash::ZERO, Bytes::from(calldata));
                    }
                }
            });
        }
    }

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
            mev_arbitrage_bot::metrics::set_rolling_pnl_eth(pnl as f64 / 1e18);
            if pnl < -(config_cb.circuit_breaker_max_loss_wei as i128) {
                tracing::error!(pnl, "CIRCUIT BREAKER TRIPPED - halting execution");
                mev_arbitrage_bot::metrics::record_circuit_breaker_trip();
                KILL_SWITCH.store(true, Ordering::Relaxed);
            }
        }
    });

    // Pool-count telemetry
    {
        let router_telemetry = Arc::clone(&router);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
            loop {
                interval.tick().await;
                mev_arbitrage_bot::metrics::set_pool_count(router_telemetry.pool_count());
            }
        });
    }

    // Main Event Loop
    while let Some(opportunity) = opportunity_rx.recv().await {
        if KILL_SWITCH.load(Ordering::Relaxed) {
            continue;
        }
        if !opportunity.is_actionable {
            continue;
        }

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

fn route_is_executable(route: &mev_arbitrage_bot::types::ArbitrageRoute) -> bool {
    route.legs.iter().all(|l| {
        matches!(
            l.pool,
            PoolState::UniswapV2 { .. } | PoolState::UniswapV3 { .. }
        )
    })
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
) -> Result<()> {
    let routes = router.find_arbitrage_routes();
    if routes.is_empty() {
        return Ok(());
    }
    // Prefer the first executable (V2/V3-only) route.
    let best_route = routes
        .iter()
        .find(|r| route_is_executable(r))
        .unwrap_or(&routes[0]);
    if !route_is_executable(best_route) {
        tracing::debug!("Skipping Curve/Balancer-only route (no calldata support yet)");
        return Ok(());
    }
    mev_arbitrage_bot::metrics::record_route_evaluated();

    // Live chain state — no hardcoding.
    let latest = provider.get_block_number().await.unwrap_or(0);
    let target_block = latest + 1;
    let gas_price = provider.get_gas_price().await.unwrap_or(20_000_000_000);
    let base_fee = U256::from(gas_price);
    let chain_id = provider.get_chain_id().await.unwrap_or(1);

    // Pure-math simulation (no RPC roundtrip).
    let sim_result = match simulator
        .simulate::<Http<Client>, Ethereum, RootProvider<Http<Client>>>(
            best_route,
            provider.clone(),
            Address::ZERO,
            alloy_primitives::Bytes::default(),
        )
        .await
    {
        Ok(r) => r,
        Err(_) => return Ok(()), // no profitable size — not an error
    };
    if sim_result.gross_profit.is_zero() {
        return Ok(());
    }
    mev_arbitrage_bot::metrics::record_profitable_route(best_route.num_hops());

    // Bidding with live base fee.
    let bid = bidding.compute(sim_result.gross_profit, base_fee, sim_result.gas_used);
    if bid.miner_reward.is_zero() {
        return Ok(());
    }

    // Build and sign with a nonce-guarded wallet.
    let (wallet, nonce) = wallet_pool
        .execute_with_wallet(|signer, _addr, nonce| {
            let wallet = EthereumWallet::from(signer.clone());
            Ok((wallet, nonce))
        })
        .await?;

    let signed_bundle = builder
        .build_and_sign(
            best_route,
            &sim_result,
            opportunity.tx_hash,
            target_block,
            bid.miner_reward,
            bid.min_profit,
            &wallet,
            nonce,
            chain_id,
            base_fee,
        )
        .await?;

    if dry_run {
        tracing::info!(
            profit = %sim_result.gross_profit,
            loan = %sim_result.optimal_loan_size,
            block = target_block,
            "DRY_RUN: would submit bundle"
        );
        return Ok(());
    }

    let mut bundle_to_submit = signed_bundle;

    let route_clone = best_route.clone();
    let sim_result_clone = sim_result.clone();
    let wallet_clone = wallet.clone();
    let builder_arc = Arc::new(builder.clone());
    let tx_hash = opportunity.tx_hash;

    let results = relayer
        .submit_with_escalation(&mut bundle_to_submit, |new_reward| {
            let route = route_clone.clone();
            let sim = sim_result_clone.clone();
            let wallet = wallet_clone.clone();
            let builder = Arc::clone(&builder_arc);

            async move {
                builder
                    .build_and_sign(
                        &route,
                        &sim,
                        tx_hash,
                        target_block,
                        new_reward,
                        bid.min_profit,
                        &wallet,
                        nonce,
                        chain_id,
                        base_fee,
                    )
                    .await
            }
        })
        .await;

    let included = results
        .iter()
        .any(|(_, o)| matches!(o, mev_arbitrage_bot::types::BundleOutcome::Included { .. }));
    let status = if included { "INCLUDED" } else { "SUBMITTED" };
    let gas_cost = U256::from(sim_result.gas_used) * base_fee;
    let net = sim_result
        .gross_profit
        .saturating_sub(bid.miner_reward)
        .saturating_sub(gas_cost);

    // Log to DB with real block + net profit.
    db.log_bundle(
        &format!("{:?}", opportunity.tx_hash),
        target_block,
        status,
        bid.miner_reward.to::<u128>(),
        net.to::<u128>() as i128,
        sim_result.gas_used,
        best_route.num_hops(),
        &format!("{:?}", best_route.base_token),
    );
    mev_arbitrage_bot::metrics::record_profit_eth(net.to::<u128>() as f64 / 1e18);

    Ok(())
}

fn init_logging(config: &Config) {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.cli.log_level));
    if config.cli.log_json {
        fmt().with_env_filter(filter).json().init();
    } else {
        fmt().with_env_filter(filter).init();
    }
}
