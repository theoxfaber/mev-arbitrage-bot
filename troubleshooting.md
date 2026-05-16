# Troubleshooting

## Common Issues

### 1. Bundle Not Included
- **High Competition**: Other searchers might be bidding more for the same opportunity.
- **Low Reputation**: If you submit many failing bundles, relays might deprioritize you. Ensure `FLASHBOTS_AUTH_KEY` is consistent.
- **Gas Limit**: Ensure the gas limit in `bundle.rs` is sufficient for the trade cycle.

### 2. Simulation Mismatch
- **State Drift**: If the on-chain state changes between simulation and inclusion, the trade might fail. The bot uses `target_block` to minimize this.
- **Incomplete Local State**: Ensure the simulator is fetching all relevant storage slots and balances for the pools involved.

### 3. Nonce Gaps
- **Manual Transactions**: If you use the bot wallets manually, the local nonce counter will be out of sync.
- **Failed Submissions**: If a transaction is signed but never reaches the mempool/relay, the bot might skip a nonce. Wait for the auto-sync (30s) or restart.

### 4. Circuit Breaker Trips
- **High Losses**: If the bot is consistently losing gas/profit (e.g., due to frequent reverts on-chain), the breaker will trip. Investigate the cause in `pnl.sqlite` before resetting.
