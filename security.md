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
