# Mainnet Deployment Guide

## 1. Prerequisites
- Fully synced Ethereum execution node (or high-quality RPC).
- EOA with enough ETH to cover gas for contract deployment and initial trades.

## 2. Contract Deployment
```bash
cd contracts
forge script script/Deploy.s.sol --rpc-url $ETH_RPC_URL_1 --broadcast --verify
```
Note: Save the `EXECUTOR_CONTRACT_ADDRESS` to your `.env`.

## 3. Bot Configuration
Ensure `DRY_RUN=true` in `.env` for the first 24 hours of operation to verify detection accuracy.

## 4. Run the Searcher
```bash
cargo run --release
```
For production, use a process manager like `systemd` or `pm2`.
