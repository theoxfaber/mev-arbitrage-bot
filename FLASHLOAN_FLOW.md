# Flashloan Flow

## Aave V3 Implementation
1. **Initiate**: `executor.executeArbitrage` calls `POOL.flashLoanSimple`.
2. **Callback**: Aave calls `executor.executeOperation`.
3. **Execution**:
   - Approve `target` DEX router for `tokenIn`.
   - Call `swap` on DEX.
   - Revoke approval.
   - Repeat for all legs.
4. **Repayment**:
   - Check if `balanceAfter > balanceBefore + repaymentAmount + minProfit`.
   - If not, `revert`.
   - Transfer `minerReward` to `block.coinbase`.
   - Approve Aave for `repaymentAmount`.
5. **Finalize**: Aave pulls the repayment amount and premium.

## Balancer Implementation
Uses `receiveFlashLoan` callback with multiple tokens supported in a single call, allowing for complex multi-asset cycles with lower gas overhead.
