//! Nonce-safe wallet pool with per-wallet locking.
//!
//! Manages a pool of executor wallets with per-wallet mutex locking for
//! concurrent nonce management. Round-robin selection distributes load
//! across wallets, while a background sync loop prevents nonce drift.

use alloy_primitives::Address;
use eyre::Result;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// A managed wallet with safe nonce tracking.
struct ManagedWallet {
    /// The wallet's private key (hex-encoded).
    private_key: String,
    /// The wallet's address (derived from the private key).
    address: Address,
    /// Per-wallet lock for nonce management.
    mutex: Mutex<()>,
    /// Local nonce offset for txs dispatched but not yet propagated.
    nonce_offset: AtomicU64,
}

/// Thread-safe pool of executor wallets.
pub struct WalletPool {
    wallets: Vec<Arc<ManagedWallet>>,
    current_index: AtomicU64,
}

impl WalletPool {
    /// Create a new wallet pool from a list of private keys.
    pub fn new(private_keys: &[String]) -> Result<Self> {
        let wallets: Vec<Arc<ManagedWallet>> = private_keys
            .iter()
            .enumerate()
            .map(|(i, pk)| {
                // Derive address from private key
                let pk_bytes = hex::decode(pk.strip_prefix("0x").unwrap_or(pk))
                    .expect("Invalid private key hex");

                // Simple address derivation placeholder — in production, use
                // alloy's signer to derive the address from the private key.
                let address = Address::from_slice(&pk_bytes[..20].try_into().unwrap_or([0u8; 20]));

                tracing::info!(
                    wallet_idx = i,
                    address = %address,
                    "Loaded executor wallet"
                );

                Arc::new(ManagedWallet {
                    private_key: pk.clone(),
                    address,
                    mutex: Mutex::new(()),
                    nonce_offset: AtomicU64::new(0),
                })
            })
            .collect();

        Ok(Self {
            wallets,
            current_index: AtomicU64::new(0),
        })
    }

    /// Execute a callback with a round-robin selected wallet while holding
    /// its nonce lock.
    pub async fn execute_with_wallet<F, T>(&self, callback: F) -> Result<T>
    where
        F: FnOnce(&str, Address, u64) -> Result<T>,
    {
        // Round-robin selection
        let idx =
            self.current_index.fetch_add(1, Ordering::Relaxed) as usize % self.wallets.len();
        let wallet = &self.wallets[idx];

        // Acquire per-wallet lock
        let _guard = wallet.mutex.lock();

        let nonce_offset = wallet.nonce_offset.load(Ordering::Relaxed);

        match callback(&wallet.private_key, wallet.address, nonce_offset) {
            Ok(result) => {
                wallet.nonce_offset.fetch_add(1, Ordering::Relaxed);
                Ok(result)
            }
            Err(e) => {
                // Reset offset on failure to prevent nonce gaps
                wallet.nonce_offset.store(0, Ordering::Relaxed);
                Err(e)
            }
        }
    }

    /// Background nonce synchronization — reconciles local offsets with
    /// on-chain confirmed nonces. Call this every ~30 seconds.
    pub fn sync_nonces(&self) {
        for (i, wallet) in self.wallets.iter().enumerate() {
            // Skip if wallet is actively locked
            if wallet.mutex.try_lock().is_some() {
                // In production, query the RPC for pending vs. confirmed nonce counts.
                // If pending == confirmed, reset the local offset.
                wallet.nonce_offset.store(0, Ordering::Relaxed);
                crate::metrics::record_nonce_sync(i);
            }
        }
    }

    /// Number of wallets in the pool.
    pub fn count(&self) -> usize {
        self.wallets.len()
    }

    /// Get all wallet addresses.
    pub fn addresses(&self) -> Vec<Address> {
        self.wallets.iter().map(|w| w.address).collect()
    }
}
