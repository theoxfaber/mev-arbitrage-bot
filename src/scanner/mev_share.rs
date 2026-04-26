//! Flashbots MEV-Share SSE (Server-Sent Events) scanner.
//!
//! Connects to the MEV-Share event stream to receive private transaction hints.

use crate::types::MevShareHint;
use alloy_primitives::{Address, TxHash, U256};
use eyre::Result;
use reqwest::Client;
use tokio::sync::mpsc;

const MEV_SHARE_SSE_URL: &str = "https://mev-share.flashbots.net";

/// Scanner for the Flashbots MEV-Share event stream.
pub struct MevShareScanner {
    client: Client,
}

impl MevShareScanner {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    /// Start listening to the MEV-Share SSE stream.
    pub async fn start(&self, tx: mpsc::Sender<MevShareHint>) -> Result<()> {
        tracing::info!("Connecting to MEV-Share SSE stream at {MEV_SHARE_SSE_URL}");

        tokio::spawn({
            let client = self.client.clone();
            let tx = tx;

            async move {
                loop {
                    if let Err(e) = Self::run_sse_stream(&client, &tx).await {
                        tracing::warn!(
                            error = %e,
                            "MEV-Share SSE stream disconnected, reconnecting in 5s..."
                        );
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    }
                }
            }
        });

        Ok(())
    }

    async fn run_sse_stream(
        client: &Client,
        tx: &mpsc::Sender<MevShareHint>,
    ) -> Result<()> {
        let response = client
            .get(MEV_SHARE_SSE_URL)
            .header("Accept", "text/event-stream")
            .send()
            .await?;

        let mut stream = response.bytes_stream();

        use futures_util::StreamExt;
        let mut buffer = String::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            let text = String::from_utf8_lossy(&chunk);
            buffer.push_str(&text);

            while let Some(idx) = buffer.find("\n\n") {
                let event = buffer[..idx].to_string();
                buffer = buffer[idx + 2..].to_string();

                if let Some(data) = Self::extract_sse_data(&event) {
                    if let Some(hint) = Self::parse_hint(&data) {
                        if Self::is_actionable(&hint) {
                            let _ = tx.try_send(hint);
                        }
                    }
                }
            }
        }

        eyre::bail!("MEV-Share SSE stream ended")
    }

    fn extract_sse_data(event: &str) -> Option<String> {
        for line in event.lines() {
            if let Some(data) = line.strip_prefix("data:") {
                return Some(data.trim().to_string());
            }
        }
        None
    }

    /// Parse a JSON hint into our MevShareHint struct manually.
    fn parse_hint(data: &str) -> Option<MevShareHint> {
        let v: serde_json::Value = serde_json::from_str(data).ok()?;

        let hash_str = v.get("hash")?.as_str()?;
        let hash: TxHash = hash_str.parse().ok()?;

        let to = v.get("to")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<Address>().ok());

        let calldata = v.get("calldata")
            .and_then(|v| v.as_str())
            .and_then(|s| hex::decode(s.strip_prefix("0x").unwrap_or(s)).ok());

        Some(MevShareHint {
            hash,
            to,
            calldata,
            logs: None,
            gas_used: None,
            mev_gas_price: None,
        })
    }

    fn is_actionable(hint: &MevShareHint) -> bool {
        if hint.to.is_none() {
            return false;
        }
        hint.calldata.is_some()
            || hint.logs.as_ref().map(|l| !l.is_empty()).unwrap_or(false)
    }
}
