# Arbitrage Strategy

## Negative Cycle Detection
The bot models the DEX ecosystem as a directed graph where tokens are nodes and pools are edges.
The edge weight is defined as `-log(exchange_rate)`.
A negative cycle (where the sum of weights is < 0) corresponds to a sequence of trades where you end up with more of the base token than you started with.

## Optimization
Profit is maximized by finding the optimal volume for the cycle. Since pool reserves are finite, larger trades suffer more slippage.
The bot uses a **binary search** over the flash loan amount to find the point where marginal slippage equals marginal gain, using `revm` for exact results.

## Execution
- **Atomic**: Wrapped in a single Ethereum transaction using a flash loan from Aave V3 or Balancer.
- **Flashbots**: Submitted via bundles to avoid being frontrun by other searchers and to protect against reverts (no gas cost if the bundle is not included).
- **Multi-Relay**: Submits to Flashbots, Titan, Beaver, and rsync to maximize inclusion probability across different block builders.
