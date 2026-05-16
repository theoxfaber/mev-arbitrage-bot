# Details and How to Run

## Setup Steps
1. Install Rust (1.80+).
2. Install Foundry (`foundryup`).
3. Clone the repo and `cp .env.example .env`.
4. Add your `ETH_RPC_URL_1`, `ETH_WSS_URL_1`, and `PRIVATE_KEYS`.
5. Run `cargo build --release`.

## Foundry Commands
- Build: `forge build`
- Test: `forge test`
- Deploy: `forge script script/Deploy.s.sol --rpc-url $ETH_RPC_URL_1 --broadcast`

## Monitoring
Access Prometheus metrics at `http://localhost:9090/metrics`.
Check `pnl.sqlite` for execution history.

## Safety Precautions
- Start with small amounts.
- Use the `KILL_SWITCH` env var for emergency shutdown.
- Never share your `.env` file.
