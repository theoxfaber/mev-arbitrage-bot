//! SQLite-backed PnL tracking and bundle execution log.
//!
//! Uses `rusqlite` (synchronous, zero-copy) instead of async SQLite.
//! All wei values are stored as TEXT to avoid integer overflow — SQLite's
//! INTEGER type is i64, which caps at ~9.2 ETH in wei.

use eyre::Result;
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Mutex;

/// Thread-safe SQLite database wrapper.
pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Open (or create) the SQLite database at the given path.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;

        // Enable WAL mode for concurrent read performance
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS bundle_logs (
                id                INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp         DATETIME DEFAULT CURRENT_TIMESTAMP,
                target_tx_hash    TEXT NOT NULL,
                target_block      INTEGER NOT NULL,
                status            TEXT NOT NULL,
                miner_reward_wei  TEXT NOT NULL DEFAULT '0',
                net_profit_wei    TEXT NOT NULL DEFAULT '0',
                gas_used          INTEGER NOT NULL DEFAULT 0,
                route_hops        INTEGER NOT NULL DEFAULT 0,
                base_token        TEXT NOT NULL DEFAULT ''
            );

            CREATE INDEX IF NOT EXISTS idx_bundle_logs_timestamp
                ON bundle_logs(timestamp);

            CREATE INDEX IF NOT EXISTS idx_bundle_logs_status
                ON bundle_logs(status);",
        )?;

        tracing::info!("SQLite database initialized (WAL mode)");
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Log a bundle submission result.
    #[allow(clippy::too_many_arguments)]
    pub fn log_bundle(
        &self,
        tx_hash: &str,
        block: u64,
        status: &str,
        miner_reward_wei: u128,
        net_profit_wei: i128,
        gas_used: u64,
        route_hops: usize,
        base_token: &str,
    ) {
        let conn = self.conn.lock().expect("db lock poisoned");
        if let Err(e) = conn.execute(
            "INSERT INTO bundle_logs
                (target_tx_hash, target_block, status, miner_reward_wei, net_profit_wei, gas_used, route_hops, base_token)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                tx_hash,
                block as i64,
                status,
                miner_reward_wei.to_string(),
                net_profit_wei.to_string(),
                gas_used as i64,
                route_hops as i64,
                base_token,
            ],
        ) {
            tracing::error!(%e, "Failed to log bundle to DB");
        }
    }

    /// Returns the net PnL (in wei) over a rolling time window.
    ///
    /// Uses parameterized queries to prevent SQL injection.
    pub fn rolling_pnl(&self, window_minutes: u64) -> i128 {
        let conn = self.conn.lock().expect("db lock poisoned");
        let result: Result<Option<String>, _> = conn.query_row(
            "SELECT CAST(SUM(CAST(net_profit_wei AS INTEGER)) AS TEXT)
             FROM bundle_logs
             WHERE timestamp >= datetime('now', ?1)",
            params![format!("-{window_minutes} minutes")],
            |row| row.get(0),
        );

        match result {
            Ok(Some(s)) => s.parse::<i128>().unwrap_or(0),
            _ => 0,
        }
    }

    /// Returns aggregate statistics for the Telegram health report.
    pub fn stats_summary(&self) -> (u64, u64, i128) {
        let conn = self.conn.lock().expect("db lock poisoned");

        let total_submitted: u64 = conn
            .query_row("SELECT COUNT(*) FROM bundle_logs", [], |row| row.get(0))
            .unwrap_or(0);

        let total_included: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM bundle_logs WHERE status = 'INCLUDED'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let total_pnl = self.rolling_pnl(u64::MAX / 2); // All-time PnL

        (total_submitted, total_included, total_pnl)
    }
}
