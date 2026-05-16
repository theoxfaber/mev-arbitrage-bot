# Setup Guide

## Requirements
- Rust 1.80+
- Foundry (for smart contract management)
- Ethereum Mainnet RPC (HTTP + WebSocket)
- Funded Ethereum Wallet (~$20 ETH for gas)

## Deployment

### 1. Deploy the Executor Contract
```bash
cd contracts
forge script script/Deploy.s.sol --rpc-url $ETH_RPC_URL --broadcast
```

### 2. Configure Environment
Copy `.env.example` to `.env` and fill in:
- `ETH_RPC_URL_1`: Primary HTTP RPC
- `ETH_WSS_URL_1`: Primary WebSocket RPC
- `PRIVATE_KEYS`: Comma-separated keys for your bot wallets
- `FLASHBOTS_AUTH_KEY`: Dedicated key for Flashbots reputation
- `EXECUTOR_CONTRACT_ADDRESS`: Address of your deployed contract

### 3. Build & Run
```bash
cargo build --release
./target/release/mev-engine
```

## Maintenance
- **Sync Nonces**: The bot handles this automatically every 30 seconds.
- **PnL Tracking**: Check `pnl.sqlite` for historical performance.
- **Circuit Breaker**: If tripped, the bot will skip opportunities until manually reset.
