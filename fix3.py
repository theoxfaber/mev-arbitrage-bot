with open("src/router/graph.rs", "r") as f:
    c = f.read()
c = c.replace("pool.address,", "pool.address(),")
with open("src/router/graph.rs", "w") as f:
    f.write(c)

with open("src/router/pool.rs", "r") as f:
    c = f.read()
c += """
pub fn is_zero_for_one(pool: &PoolState, token_in: Address) -> bool {
    pool.token0() == token_in
}
"""
with open("src/router/pool.rs", "w") as f:
    f.write(c)

with open("src/simulator/evm.rs", "r") as f:
    c = f.read()
c = c.replace("simulate_v2_swap(&leg.pool, current_amount, z41)", "uniswap_v2_swap(&leg.pool, current_amount, z41).map(|res| res.amount_out).unwrap_or(alloy_primitives::U256::ZERO)")
c = c.replace("use crate::router::pool::{uniswap_v2_swap};", "use crate::router::pool::{uniswap_v2_swap, is_zero_for_one};")
with open("src/simulator/evm.rs", "w") as f:
    f.write(c)

