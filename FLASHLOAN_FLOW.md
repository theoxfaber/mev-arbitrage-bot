# Flashloan Flow

1. **Initiation**: The Rust bot calls `executeArbitrage` on `ArbitrageExecutor.sol`.
2. **Flashloan**: The contract requests a `flashLoanSimple` from Aave V3.
3. **Callback**: Aave calls `executeOperation` on the contract.
4. **Validation**: The contract verifies the caller is the Aave Pool and the initiator is itself.
5. **Execution**: The contract loops through the `Action` array, executing swaps.
6. **Repayment**: The contract approves Aave to pull the loan amount + premium.
7. **Miner Payment**: If profitable, the contract pays the miner via `block.coinbase.call`.
8. **Final Check**: The contract reverts if the final balance is less than required.
