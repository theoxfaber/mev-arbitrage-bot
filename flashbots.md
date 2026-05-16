# Flashbots Integration

## Bundle Structure
- **Target Transaction**: The pending transaction from the mempool that the bot backruns.
- **Arbitrage Transaction**: The bot's signed transaction that executes the cycle.

## Authentication
Flashbots requires every bundle request to be signed with a dedicated private key. This key does not need funds and is used solely to track the searcher's reputation.
The bot implements this using the `X-Flashbots-Signature` header, containing an EIP-191 signature of the JSON-RPC request body.

## Relay Endpoints
- **Flashbots**: `https://relay.flashbots.net`
- **Titan**: `https://rpc.titanbuilder.xyz`
- **Beaverbuild**: `https://rpc.beaverbuild.org`
- **rsync**: `https://rsync-builder.xyz`

## Bid Escalation
If a bundle is not included in the first attempt, the bot can escalate the miner reward (the "bribe") in subsequent blocks to remain competitive with other searchers.
This is configured via `MAX_ESCALATION_ATTEMPTS` and `ESCALATION_INCREMENT_BPS` in `src/executor/relayer.rs`.
