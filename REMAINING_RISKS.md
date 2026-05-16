# Remaining Risks & Future Work

## 1. Simulation Accuracy (High Risk)
The `EvmSimulator` currently uses estimated gas and gross profit from the router.
- **Impact**: Real trades may revert due to price changes or state conflicts not captured in the router.
- **Fix**: Implement full `revm` execution with `alloy` state providers.

## 2. Bundle Re-signing (Medium Risk)
Escalating bids in `FlashbotsRelayer` does not re-sign the transaction.
- **Impact**: If a trade is only profitable at a higher bid, the escalated bundle will be rejected by the builder because the signature doesn't match the inner transaction data.
- **Fix**: Move signing logic into the relayer or implement a callback to `BundleBuilder` during escalation.

## 3. Sandwich/Arbitrage Structure (Medium Risk)
The bot currently submits a bundle containing only its own transaction.
- **Impact**: This is only effective for "backrunning" (arbitrage after a state-changing tx). It cannot perform "sandwiching" because it doesn't include the victim transaction in the bundle.
- **Fix**: Extract the target transaction from the mempool and include it at index 0 of the bundle.

## 4. Pool Coverage (Low Risk)
Currently only supports UniswapV2 and UniswapV3.
- **Future Work**: Add support for Curve, Balancer, and Maverick pools to increase arbitrage surface.
