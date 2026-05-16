# Known Limitations

## 1. Supported Protocols
Currently supports UniswapV2 (and clones like SushiSwap) and UniswapV3. Curve and Balancer support is implemented in math but requires further on-chain testing for edge cases.

## 2. Competitive Landscape
This bot is a reference searcher. High-frequency competitors may have faster networking or specialized hardware.

## 3. Simulation Latency
The `revm` simulation adds ~10-20ms of latency per trade, which is necessary for safety but may impact success rates in highly competitive blocks.
