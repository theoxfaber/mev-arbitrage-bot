//! Multi-relay Flashbots bundle submission with secure authentication.

use crate::types::{BundleOutcome, FlashbotsBundle};
use alloy::signers::local::PrivateKeySigner;
use alloy::signers::Signer;
use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;

/// Known relay/builder endpoints.
const RELAYS: &[(&str, &str)] = &[
    ("flashbots", "https://relay.flashbots.net"),
    ("titan", "https://rpc.titanbuilder.xyz"),
    ("rsync", "https://rsync-builder.xyz"),
    ("beaverbuild", "https://rpc.beaverbuild.org"),
];

/// Maximum number of bid escalation attempts per bundle.
const MAX_ESCALATION_ATTEMPTS: u32 = 4;

/// Bid escalation increment in basis points (1500 = 15%).
const ESCALATION_INCREMENT_BPS: u32 = 1500;

/// Flashbots multi-relay submitter.
pub struct FlashbotsRelayer {
    client: Client,
    auth_signer: PrivateKeySigner,
}

impl FlashbotsRelayer {
    pub fn new(auth_signer_key: String) -> eyre::Result<Self> {
        let auth_signer = auth_signer_key.parse::<PrivateKeySigner>()?;
        Ok(Self {
            client: Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .expect("HTTP client"),
            auth_signer,
        })
    }

    /// Submit a bundle to all relays in parallel with bid escalation.
    pub async fn submit_bundle(&self, bundle: &FlashbotsBundle) -> Vec<(String, BundleOutcome)> {
        let mut results = Vec::new();

        let bundle_json = self.build_bundle_json(bundle);
        let auth_header = match self.build_auth_header(&bundle_json).await {
            Ok(h) => h,
            Err(e) => {
                tracing::error!(error = %e, "Failed to build Flashbots auth header");
                return vec![("all".to_string(), BundleOutcome::Aborted { reason: e.to_string() })];
            }
        };

        let mut handles = Vec::new();

        for &(name, url) in RELAYS {
            let client = self.client.clone();
            let bundle_json = bundle_json.clone();
            let url = url.to_string();
            let name = name.to_string();
            let auth_header = auth_header.clone();

            handles.push(tokio::spawn(async move {
                let result = Self::submit_to_relay(&client, &url, &bundle_json, &auth_header).await;
                crate::metrics::record_bundle_submitted(&name);
                (name, result)
            }));
        }

        for handle in handles {
            match handle.await {
                Ok((name, outcome)) => {
                    crate::metrics::record_bundle_result(&outcome.to_string());
                    results.push((name, outcome));
                }
                Err(e) => {
                    tracing::error!(error = %e, "Relay submission task panicked");
                }
            }
        }

        results
    }

    /// Submit with bid escalation: retry with 15% higher miner rewards.
    ///
    /// WARNING: Bid escalation currently requires rebuilding and re-signing
    /// the transaction to update the `minerReward` in the contract call.
    /// The current implementation only updates the struct field, which is
    /// insufficient for on-chain enforcement.
    pub async fn submit_with_escalation(
        &self,
        bundle: &mut FlashbotsBundle,
    ) -> Vec<(String, BundleOutcome)> {
        let original_reward = bundle.miner_reward;
        let mut all_results = Vec::new();

        for attempt in 0..MAX_ESCALATION_ATTEMPTS {
            if attempt > 0 {
                // Escalate: increase miner reward by 15%
                let increment = (original_reward
                    * alloy_primitives::U256::from(ESCALATION_INCREMENT_BPS * attempt))
                    / alloy_primitives::U256::from(10_000u64);
                bundle.miner_reward = original_reward + increment;

                tracing::info!(
                    attempt,
                    new_reward = %bundle.miner_reward,
                    "Escalating bid (Warning: Re-signing required but not implemented in relayer)"
                );
            }

            let results = self.submit_bundle(bundle).await;

            // Check if any relay accepted
            let any_success = results
                .iter()
                .any(|(_, outcome)| matches!(outcome, BundleOutcome::Included { .. }));

            all_results.extend(results);

            if any_success {
                break;
            }

            // Wait briefly before escalating
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        // Restore original reward
        bundle.miner_reward = original_reward;

        all_results
    }

    async fn submit_to_relay(
        client: &Client,
        url: &str,
        bundle_json: &Value,
        auth_header: &str,
    ) -> BundleOutcome {
        match client
            .post(url)
            .header("X-Flashbots-Signature", auth_header)
            .header("Content-Type", "application/json")
            .json(bundle_json)
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    match response.json::<Value>().await {
                        Ok(body) => {
                            if let Some(error) = body.get("error") {
                                BundleOutcome::SimulationFailed {
                                    reason: error.to_string(),
                                }
                            } else {
                                BundleOutcome::Included { block: 0 }
                            }
                        }
                        Err(e) => BundleOutcome::RelayError {
                            reason: format!("Failed to parse response: {e}"),
                        },
                    }
                } else {
                    BundleOutcome::RelayError {
                        reason: format!("HTTP {}", response.status()),
                    }
                }
            }
            Err(e) => BundleOutcome::RelayError {
                reason: format!("Request failed: {e}"),
            },
        }
    }

    fn build_bundle_json(&self, bundle: &FlashbotsBundle) -> Value {
        let signed_txs: Vec<String> = bundle
            .signed_txs
            .iter()
            .map(|tx| format!("0x{}", hex::encode(tx)))
            .collect();

        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_sendBundle",
            "params": [{
                "txs": signed_txs,
                "blockNumber": format!("0x{:x}", bundle.target_block),
                "minTimestamp": 0,
                "maxTimestamp": 0,
            }]
        })
    }

    /// Sign the bundle payload with the auth key using EIP-191.
    async fn build_auth_header(&self, bundle_json: &Value) -> eyre::Result<String> {
        let body = serde_json::to_string(bundle_json)?;
        let hashed_body = alloy_primitives::keccak256(body.as_bytes());
        let signature = self.auth_signer.sign_hash(&hashed_body).await?;

        Ok(format!(
            "{:?}:0x{}",
            self.auth_signer.address(),
            hex::encode(signature.as_bytes())
        ))
    }
}
