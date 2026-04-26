import re
with open("src/router/pool.rs", "r") as f:
    c = f.read()

c = c.replace("""    if let PoolState::UniswapV3 { .. 
        sqrt_price_x96,
        liquidity,
        tick: _tick,
        tick_spacing,
        fee,
        tick_bitmap: _, // Simplified for brevity in exact single-tick bounds
        ticks,
    , ..} = state""", """    if let PoolState::UniswapV3 {
        sqrt_price_x96,
        liquidity,
        tick: _tick,
        tick_spacing,
        fee,
        ticks,
        ..
    } = state""")

c = c.replace("PoolState::Curve { balances, amp, n_coins, fee_bps }", "PoolState::Curve { balances, amp, n_coins, fee_bps, .. }")
c = c.replace("PoolState::Balancer { balances, weights, fee_bps }", "PoolState::Balancer { balances, weights, fee_bps, .. }")

with open("src/router/pool.rs", "w") as f:
    f.write(c)

with open("src/simulator/evm.rs", "r") as f:
    c = f.read()
c = c.replace("leg.pool.address,", "leg.pool.address(),")
c = c.replace("use crate::router::pool::{is_zero_for_one, simulate_v2_swap};", "use crate::router::pool::{uniswap_v2_swap};")
with open("src/simulator/evm.rs", "w") as f:
    f.write(c)

