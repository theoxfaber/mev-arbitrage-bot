# Troubleshooting

## 1. High Revert Rate
- Check if your RPC is providing stale state.
- Verify that `EvmSimulator` is forking the correct block.

## 2. Low Inclusion Rate
- Your bids may be too low. Increase `MAX_MINER_REWARD_BPS`.
- Ensure your `FLASHBOTS_AUTH_KEY` has built up reputation.

## 3. Nonce Desync
- Restart the bot to force a fresh nonce sync from the chain.
- Check if multiple instances are using the same `PRIVATE_KEYS`.
