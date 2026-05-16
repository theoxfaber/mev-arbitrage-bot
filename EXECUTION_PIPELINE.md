# Execution Pipeline

1. **Mempool Trigger**: `newPendingTransactions` -> `SwapDecoder`.
2. **Pathfinding**: `ArbitrageRouter` -> `ArbitrageRoute`.
3. **Calldata Prep**: `BundleBuilder` encodes `executeArbitrage` params.
4. **First Simulation**: `EvmSimulator` runs the trade at base gas price.
5. **Bid Calculation**: `BiddingEngine` determines `minerReward`.
6. **Signing**: `BundleBuilder` signs the transaction.
7. **Submission**: `FlashbotsRelayer` sends bundle to multiple builders.
8. **Inclusion Monitor**: The bot waits for the target block and logs the result.
