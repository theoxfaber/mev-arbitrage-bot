//! Bellman-Ford negative-cycle detection on a token exchange-rate graph.
//!
//! This is the mathematical core of the engine. Instead of scanning a fixed
//! set of hardcoded pool pairs, we maintain a directed weighted graph:
//!
//!   Node = Token address
//!   Edge = Pool swap, weighted as -log(exchange_rate)
//!
//! A negative-weight cycle in the -log(rate) graph corresponds to:
//!   product(rates around cycle) > 1.0  ⟹  arbitrage exists
//!
//! We run Bellman-Ford from each "anchor" token (WETH, USDC, etc.) and
//! detect negative cycles by running one extra relaxation pass beyond
//! the standard |V|-1 iterations.
//!
//! **Key differentiator**: Most open-source MEV bots hardcode 2-3 pools.
//! This router dynamically discovers multi-hop routes across ALL pools
//! in the graph, supporting 2-hop, 3-hop, and even 4-hop cycles.

use crate::router::pool::{exchange_rate_0_to_1, exchange_rate_1_to_0};
use crate::types::{ArbitrageRoute, PoolState, SwapLeg};
use alloy_primitives::{Address, U256};
use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::Arc;

/// Maximum hops in an arbitrage route. Beyond 4, gas costs dominate.
const MAX_HOPS: usize = 4;

/// Minimum profitable log-rate for a cycle to be considered (noise filter).
const MIN_LOG_PROFIT: f64 = 0.001; // ~0.1% after fees

/// The arbitrage router maintains a live graph of pools and discovers
/// profitable routes using Bellman-Ford negative-cycle detection.
pub struct ArbitrageRouter {
    /// Live pool states keyed by pool address.
    pools: Arc<DashMap<Address, PoolState>>,
    /// "Anchor" tokens from which we search for cycles (e.g., WETH, USDC).
    anchor_tokens: Vec<Address>,
}

impl ArbitrageRouter {
    pub fn new(anchor_tokens: Vec<Address>) -> Self {
        Self {
            pools: Arc::new(DashMap::new()),
            anchor_tokens,
        }
    }

    /// Get a reference to the pool registry (for external updates).
    pub fn pool_registry(&self) -> Arc<DashMap<Address, PoolState>> {
        Arc::clone(&self.pools)
    }

    /// Update a pool's state (called when Multicall returns fresh reserves).
    pub fn update_pool(&self, pool: PoolState) {
        self.pools.insert(pool.address, pool);
    }

    /// Remove a pool from the graph (e.g., if it's drained or invalid).
    pub fn remove_pool(&self, address: &Address) {
        self.pools.remove(address);
    }

    /// Current number of pools in the graph.
    pub fn pool_count(&self) -> usize {
        self.pools.len()
    }

    /// Run the Bellman-Ford algorithm from each anchor token and collect
    /// all profitable arbitrage cycles.
    ///
    /// Returns routes sorted by expected profit (descending).
    pub fn find_arbitrage_routes(&self) -> Vec<ArbitrageRoute> {
        let pools: Vec<PoolState> = self.pools.iter().map(|e| e.value().clone()).collect();
        if pools.is_empty() {
            return vec![];
        }

        // Build adjacency list: token → [(neighbor_token, pool, direction, weight)]
        let mut graph: HashMap<Address, Vec<Edge>> = HashMap::new();

        for pool in &pools {
            let rate_0_1 = exchange_rate_0_to_1(pool);
            let rate_1_0 = exchange_rate_1_to_0(pool);

            if rate_0_1 > 0.0 {
                graph.entry(pool.token0).or_default().push(Edge {
                    to: pool.token1,
                    pool: pool.clone(),
                    zero_for_one: true,
                    weight: -rate_0_1.ln(), // Negative log for Bellman-Ford
                });
            }

            if rate_1_0 > 0.0 {
                graph.entry(pool.token1).or_default().push(Edge {
                    to: pool.token0,
                    pool: pool.clone(),
                    zero_for_one: false,
                    weight: -rate_1_0.ln(),
                });
            }
        }

        let all_tokens: Vec<Address> = graph.keys().copied().collect();
        let mut routes = Vec::new();

        // Run Bellman-Ford from each anchor token
        for anchor in &self.anchor_tokens {
            if !graph.contains_key(anchor) {
                continue;
            }

            if let Some(cycle) = self.bellman_ford(&graph, &all_tokens, *anchor) {
                let total_weight: f64 = cycle.iter().map(|e| e.weight).sum();

                // A negative total weight means the product of exchange rates > 1.0
                if total_weight < -MIN_LOG_PROFIT {
                    let log_profit = -total_weight;
                    let multiplier = log_profit.exp();

                    let route = ArbitrageRoute {
                        base_token: *anchor,
                        legs: cycle
                            .iter()
                            .map(|e| SwapLeg {
                                pool: e.pool.clone(),
                                token_in: if e.zero_for_one {
                                    e.pool.token0
                                } else {
                                    e.pool.token1
                                },
                                token_out: e.to,
                                amount_in: U256::ZERO, // Set during optimization
                                expected_amount_out: U256::ZERO,
                            })
                            .collect(),
                        expected_gross_profit: U256::ZERO, // Set during simulation
                        optimal_loan_size: U256::ZERO,
                        confidence: (multiplier - 1.0).min(1.0),
                    };

                    if route.num_hops() <= MAX_HOPS {
                        crate::metrics::record_route_evaluated();
                        crate::metrics::record_profitable_route(route.num_hops());
                        routes.push(route);
                    }
                }
            }
        }

        // Sort by confidence (proxy for profit) descending
        routes.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
        routes
    }

    /// Standard Bellman-Ford with negative-cycle detection.
    ///
    /// Returns the edges forming the negative cycle if one exists,
    /// or None if no negative cycle is reachable from `source`.
    fn bellman_ford(
        &self,
        graph: &HashMap<Address, Vec<Edge>>,
        all_tokens: &[Address],
        source: Address,
    ) -> Option<Vec<Edge>> {
        let n = all_tokens.len();
        let idx: HashMap<Address, usize> = all_tokens
            .iter()
            .enumerate()
            .map(|(i, &addr)| (addr, i))
            .collect();

        let mut dist = vec![f64::INFINITY; n];
        let mut predecessor: Vec<Option<(usize, Edge)>> = vec![None; n];

        let source_idx = *idx.get(&source)?;
        dist[source_idx] = 0.0;

        // Relax edges |V| - 1 times
        for _ in 0..n - 1 {
            let mut updated = false;
            for (&from_token, edges) in graph {
                let from_idx = match idx.get(&from_token) {
                    Some(&i) => i,
                    None => continue,
                };

                if dist[from_idx] == f64::INFINITY {
                    continue;
                }

                for edge in edges {
                    let to_idx = match idx.get(&edge.to) {
                        Some(&i) => i,
                        None => continue,
                    };

                    let new_dist = dist[from_idx] + edge.weight;
                    if new_dist < dist[to_idx] - 1e-10 {
                        dist[to_idx] = new_dist;
                        predecessor[to_idx] = Some((from_idx, edge.clone()));
                        updated = true;
                    }
                }
            }

            if !updated {
                break; // Early termination — no more relaxations possible
            }
        }

        // One more pass to detect negative cycles
        for (&from_token, edges) in graph {
            let from_idx = match idx.get(&from_token) {
                Some(&i) => i,
                None => continue,
            };

            if dist[from_idx] == f64::INFINITY {
                continue;
            }

            for edge in edges {
                let to_idx = match idx.get(&edge.to) {
                    Some(&i) => i,
                    None => continue,
                };

                if dist[from_idx] + edge.weight < dist[to_idx] - 1e-10 {
                    // Update predecessor to close the cycle
                    let mut pred_clone = predecessor.clone();
                    pred_clone[to_idx] = Some((from_idx, edge.clone()));
                    // Found a negative cycle — trace it back
                    return self.extract_cycle(&pred_clone, to_idx, all_tokens, &idx);
                }
            }
        }

        None
    }

    /// Trace the predecessor chain to extract the negative cycle.
    fn extract_cycle(
        &self,
        predecessor: &[Option<(usize, Edge)>],
        start: usize,
        _all_tokens: &[Address],
        _idx: &HashMap<Address, usize>,
    ) -> Option<Vec<Edge>> {
        let n = predecessor.len();
        let mut visited = vec![false; n];
        let mut current = start;

        // Walk back to find a node in the cycle
        for _ in 0..n {
            if visited[current] {
                break;
            }
            visited[current] = true;
            current = match &predecessor[current] {
                Some((prev, _)) => *prev,
                None => return None,
            };
        }

        // Now `current` is in the cycle — trace the full cycle
        let cycle_start = current;
        let mut cycle_edges = Vec::new();

        loop {
            let (prev, edge) = predecessor[current].as_ref()?;
            cycle_edges.push(edge.clone());
            current = *prev;
            if current == cycle_start {
                break;
            }
            if cycle_edges.len() > MAX_HOPS {
                return None; // Cycle too long
            }
        }

        cycle_edges.reverse();

        // Verify the cycle returns to the start token
        if let Some(first) = cycle_edges.first() {
            let start_token = if first.zero_for_one {
                first.pool.token0
            } else {
                first.pool.token1
            };
            if let Some(last) = cycle_edges.last() {
                if last.to != start_token {
                    return None; // Not a valid cycle
                }
            }
        }

        Some(cycle_edges)
    }
}

/// An edge in the token exchange-rate graph.
#[derive(Debug, Clone)]
struct Edge {
    /// The destination token.
    to: Address,
    /// The pool through which this swap occurs.
    pool: PoolState,
    /// Swap direction: true = token0→token1, false = token1→token0.
    zero_for_one: bool,
    /// Edge weight: -log(exchange_rate). Negative cycle ⟹ arbitrage.
    weight: f64,
}
