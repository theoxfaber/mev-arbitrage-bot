//! Nonce-safe wallet pool with per-wallet locking and alloy-signer integration.

use alloy::signers::local::PrivateKeySigner;
use alloy::signers::Signer;
use alloy_primitives::Address;
use eyre::{Context, Result};
use parking_lot::Mutex;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use alloy::network::Network;
use alloy::transports::Transport;
use alloy::providers::Provider;

/// A managed wallet with safe nonce tracking and alloy-signer.
pub struct ManagedWallet {
    pub signer: PrivateKeySigner,
    pub address: Address,
    pub mutex: Mutex<()>,
    pub next_nonce: AtomicU64,
}

pub struct WalletPool {
    wallets: Vec<Arc<ManagedWallet>>,
    current_index: AtomicU64,
}

impl WalletPool {
    pub fn new(private_keys: &[String]) -> Result<Self> {
        let mut wallets = Vec::with_capacity(private_keys.len());

        for (i, pk_hex) in private_keys.iter().enumerate() {
            let signer = PrivateKeySigner::from_str(pk_hex)
                .wrap_err_with(|| format!("Invalid private key at index {i}"))?;

            let address = signer.address();

            tracing::info!(
                wallet_idx = i,
                address = %address,
                "Loaded executor wallet"
            );

            wallets.push(Arc::new(ManagedWallet {
                signer,
                address,
                mutex: Mutex::new(()),
                next_nonce: AtomicU64::new(0),
            }));
        }

        Ok(Self {
            wallets,
            current_index: AtomicU64::new(0),
        })
    }

    pub async fn execute_with_wallet<F, T_Ret>(&self, callback: F) -> Result<T_Ret>
    where
        F: FnOnce(&PrivateKeySigner, Address, u64) -> Result<T_Ret>,
    {
        let idx = self.current_index.fetch_add(1, Ordering::Relaxed) as usize % self.wallets.len();
        let wallet = &self.wallets[idx];

        let _guard = wallet.mutex.lock();
        let nonce = wallet.next_nonce.load(Ordering::Relaxed);

        match callback(&wallet.signer, wallet.address, nonce) {
            Ok(result) => {
                wallet.next_nonce.fetch_add(1, Ordering::Relaxed);
                Ok(result)
            }
            Err(e) => Err(e),
        }
    }

    pub async fn sync_nonces<T, N, P>(&self, provider: &P)
    where
        T: Transport + Clone,
        N: Network,
        P: Provider<T, N>,
    {
        for (i, wallet) in self.wallets.iter().enumerate() {
            let (current, addr) = {
                if let Some(_guard) = wallet.mutex.try_lock() {
                    (wallet.next_nonce.load(Ordering::Relaxed), wallet.address)
                } else {
                    continue;
                }
            };

            match provider.get_transaction_count(addr).await {
                Ok(nonce) => {
                    if nonce > current {
                        tracing::info!(
                            wallet_idx = i,
                            address = %addr,
                            old_nonce = current,
                            new_nonce = nonce,
                            "Synced nonce from chain"
                        );
                        wallet.next_nonce.store(nonce, Ordering::Relaxed);
                    }
                }
                Err(e) => {
                    tracing::error!(
                        wallet_idx = i,
                        address = %addr,
                        error = %e,
                        "Failed to sync nonce from chain"
                    );
                }
            }
            crate::metrics::record_nonce_sync(i);
        }
    }

    pub fn count(&self) -> usize {
        self.wallets.len()
    }

    pub fn addresses(&self) -> Vec<Address> {
        self.wallets.iter().map(|w| w.address).collect()
    }

    pub fn wallets(&self) -> &[Arc<ManagedWallet>] {
        &self.wallets
    }
}
