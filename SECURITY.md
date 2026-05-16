# Security Measures

## 1. Smart Contract
- **Access Control**: `onlyOwner` modifier on all critical functions.
- **Reentrancy**: `nonReentrant` guard on execution paths.
- **Safety**: Reverts on any unprofitable trade or failed repayment.

## 2. Key Management
- **Environment Variables**: Private keys are loaded from `.env` and never logged.
- **Wallet Separation**: Use a dedicated EOA for the Flashbots auth key (reputation only).

## 3. Bot Logic
- **Circuit Breaker**: Stops execution if rolling PnL drops below the threshold.
- **Nonce Management**: Per-wallet Mutex prevents race conditions.
- **Simulation**: Mandatory `revm` simulation before any submission.
