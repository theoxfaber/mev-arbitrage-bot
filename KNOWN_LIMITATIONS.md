# Known Limitations

## 1. Multi-Step State Changes
The current simulator does not fully capture tokens that change their logic between blocks or pools with custom taxation (fee-on-transfer).

## 2. Reorg Probability
While Flashbots protects against reverts, a chain reorg (> 1 block) can still invalidate a bundle if the target transaction was only in the reorged tip.

## 3. Storage Slot Discovery
In the current refactor, storage slots for new pools must be pre-indexed or discovered via heavy RPC lookups during the first encounter.

## 4. Latency
Rust provides sub-millisecond local logic, but network latency to the RPC (mempool) and Relayer remains the primary bottleneck for competitive opportunities.
