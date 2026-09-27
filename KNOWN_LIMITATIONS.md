# Known Limitations

1. **Execution**: V2 + V3 only. Curve/Balancer legs simulate correctly
   (`src/router/pool.rs`) but `BundleBuilder` rejects them — routes
   containing them are skipped, not mis-executed.
2. **Decoder**: V2 exact/ETH variants, V3 single/multi, Universal coarse.
   No full 1inch Fusion / CoW intent decoding yet.
3. **Trigger**: size + protection heuristic, no oracle. Whales
   (>=0.5 ETH norm) or unprotected dust (>=0.05 ETH, minOut=0) only.
4. **Competition**: reference searcher; HFT with private flow / FPGA /
   co-lo will win ties.
5. **Node**: needs `eth_subscribe` + `newPendingTransactions` +
   full-tx payloads. Public endpoints usually disable these.
