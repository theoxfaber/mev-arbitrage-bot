//! Prometheus metrics for full engine observability.
//!
//! Exposes counters, gauges, and histograms for every critical code path.
//! A Prometheus-compatible HTTP endpoint is started on the configured port.

use eyre::Result;
use metrics::{counter, gauge, histogram};
use metrics_exporter_prometheus::PrometheusBuilder;
use std::net::SocketAddr;

/// Initialize the Prometheus metrics exporter on the given port.
/// Returns immediately after binding the HTTP listener.
pub fn init_metrics_server(port: u16) -> Result<()> {
    if port == 0 {
        tracing::info!("Metrics server disabled (port=0)");
        return Ok(());
    }

    let addr: SocketAddr = ([0, 0, 0, 0], port).into();

    PrometheusBuilder::new()
        .with_http_listener(addr)
        .install()
        .map_err(|e| eyre::eyre!("Failed to start metrics server on {addr}: {e}"))?;

    tracing::info!(%addr, "Prometheus metrics server started");
    Ok(())
}

// ─── Scanner Metrics ─────────────────────────────────────────────────────────

/// Record a pending transaction received from the mempool.
pub fn record_tx_scanned(rpc_id: &str) {
    counter!("scanner.txs_received", "rpc" => rpc_id.to_string()).increment(1);
}

/// Record a deduplicated (dropped) transaction.
pub fn record_tx_deduplicated() {
    counter!("scanner.txs_deduplicated").increment(1);
}

/// Record a decoded swap opportunity.
pub fn record_opportunity_found(protocol: &str) {
    counter!("scanner.opportunities_found", "protocol" => protocol.to_string()).increment(1);
}

// ─── Router Metrics ──────────────────────────────────────────────────────────

/// Record the number of pools in the routing graph.
pub fn set_pool_count(count: usize) {
    gauge!("router.pool_count").set(count as f64);
}

/// Record a routing cycle that was considered.
pub fn record_route_evaluated() {
    counter!("router.routes_evaluated").increment(1);
}

/// Record a profitable route discovered.
pub fn record_profitable_route(hops: usize) {
    counter!("router.profitable_routes", "hops" => hops.to_string()).increment(1);
}

// ─── Simulator Metrics ───────────────────────────────────────────────────────

/// Record a revm simulation execution.
pub fn record_simulation(duration_ms: f64, success: bool) {
    histogram!("simulator.duration_ms").record(duration_ms);
    counter!(
        "simulator.executions",
        "result" => if success { "success" } else { "failure" }.to_string()
    )
    .increment(1);
}

// ─── Executor Metrics ────────────────────────────────────────────────────────

/// Record a bundle submission attempt.
pub fn record_bundle_submitted(relay: &str) {
    counter!("executor.bundles_submitted", "relay" => relay.to_string()).increment(1);
}

/// Record a bundle inclusion result.
pub fn record_bundle_result(outcome: &str) {
    counter!("executor.bundle_results", "outcome" => outcome.to_string()).increment(1);
}

/// Record profit from a successful inclusion (in ETH for readability).
pub fn record_profit_eth(profit_eth: f64) {
    histogram!("executor.profit_eth").record(profit_eth);
    gauge!("executor.last_profit_eth").set(profit_eth);
}

/// Record the rolling PnL.
pub fn set_rolling_pnl_eth(pnl_eth: f64) {
    gauge!("circuit_breaker.rolling_pnl_eth").set(pnl_eth);
}

// ─── Wallet Metrics ──────────────────────────────────────────────────────────

/// Record a nonce sync event.
pub fn record_nonce_sync(wallet_idx: usize) {
    counter!("wallet.nonce_syncs", "wallet" => wallet_idx.to_string()).increment(1);
}

// ─── System Metrics ──────────────────────────────────────────────────────────

/// Set the engine uptime.
pub fn set_uptime(seconds: u64) {
    gauge!("engine.uptime_seconds").set(seconds as f64);
}

/// Record a circuit breaker trip.
pub fn record_circuit_breaker_trip() {
    counter!("circuit_breaker.trips").increment(1);
}
