# Execution Pipeline

## 1. Scanner Stage
- **Input**: WebSocket streams from multiple RPCs.
- **Logic**: Deduplicates transaction hashes. Decodes swap calldata to extract `tokenIn`, `tokenOut`, and `amountIn`.
- **Output**: `SandwichOpportunity` channel.

## 2. Router Stage
- **Input**: `SandwichOpportunity`.
- **Logic**: Updates the internal pool graph. Runs Bellman-Ford/SPFA from the base tokens (WETH/USDC/USDT) to find negative cycles.
- **Output**: `ArbitrageRoute`.

## 3. Simulator Stage
- **Input**: `ArbitrageRoute`.
- **Logic**: Forks mainnet state into a local `revm` instance. Executes a binary search over loan amounts to find the profit-maximizing volume, accounting for flash loan premiums.
- **Output**: `SimulationResult`.

## 4. Bidding Stage
- **Input**: `SimulationResult` + Current `baseFee`.
- **Logic**: Adjusts the miner reward (bribe) based on network congestion and historical competitor floors to ensure inclusion while protecting margin.
- **Output**: `BiddingDecision`.

## 5. Submitter Stage
- **Input**: `BiddingDecision` + `ArbitrageRoute`.
- **Logic**: Selects a wallet from the pool. Signs an EIP-1559 transaction backrunning the target tx. Builds a Flashbots bundle and signs the auth header.
- **Output**: Parallel submission to all configured relays.
