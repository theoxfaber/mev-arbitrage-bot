# Security Policy

## Core Principles
- **No Private Key Leakage**: Private keys never leave the bot's process.
- **Environment Secrets**: All credentials must be loaded via environment variables.
- **Secure Signing**: All transaction and authentication signing is handled by `alloy-signer`.
- **Atomic Execution**: All trades are wrapped in a flash loan on-chain to ensure they either succeed entirely or revert.

## Mitigations
- **Flashbots Auth**: Bundles are signed using EIP-191, preventing credential theft by relays.
- **Redacted Logs**: Sensitive values are never printed to stdout or stored in logs.
- **Circuit Breaker**: An automatic monitor halts execution if rolling PnL drops below a safe threshold.
- **Access Control**: The `ArbitrageExecutor.sol` contract is protected by `onlyOwner` modifiers and reentrancy guards.

## Vulnerability Remediation
See `FINAL_SECURITY_REPORT.md` for a list of fixed vulnerabilities from the initial audit.

---

## Operational Measures (merged from legacy SECURITY.md)

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
