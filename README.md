# MEV Arbitrage Bot (POC / Template)

**⚠️ WARNING: This project is a Proof-of-Concept (POC) and is NOT production-ready. It has identified security risks and missing core functionality.**

## Project Overview
This repository provides a high-performance framework for detecting and simulating MEV (Maximal Extractable Value) arbitrage opportunities on Ethereum. It uses a Rust-native stack to achieve sub-millisecond route discovery and simulation.

## Architecture
- **Language**: Rust 1.80+
- **Pathfinding**: Parallelized SPFA (Shortest Path Faster Algorithm) on Strongly Connected Components.
- **Simulation**: In-process EVM execution via `revm` (no RPC calls for simulation).
- **Execution**: Atomic flash-loan execution via `ArbitrageExecutor.sol` (Aave V3).

## Data Flow
1. **Mempool Scan**: (Placeholder) Monitor `newPendingTransactions`.
2. **Decode**: Extract swap parameters from calldata.
3. **Graph Analysis**: Update the token graph and search for negative cycles (arbitrage).
4. **Optimization**: Binary search for optimal flash loan size.
5. **Submission**: Bundle submission via Flashbots (Requires implementation of signing).

## Critical Safety Considerations
- **Transaction Signing**: The current codebase **does not sign transactions**. It will not work on-chain without integrating a signer (e.g., `alloy-signer`).
- **Address Derivation**: Private key to address derivation is currently a placeholder.
- **Credential Safety**: Never use your primary keys. Use dedicated, low-balance bot wallets.

## Setup & Run (FOR DEVELOPMENT ONLY)
1. **Clone & Build**:
   ```bash
   git clone https://github.com/theoxfaber/mev-arbitrage-bot
   cargo build
   ```
2. **Config**: Rename `.env.example` to `.env` and fill in RPC URLs.
3. **Test**:
   ```bash
   cargo test
   ```

## Known Risks
See [SECURITY_AUDIT.md](./SECURITY_AUDIT.md) for a detailed breakdown of identified risks, including a recently patched credential leak.

## Disclaimer
This software is provided "as is" without warranty of any kind. Arbitrage trading involves significant risk of financial loss.
