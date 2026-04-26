with open("src/router/graph.rs", "r") as f:
    c = f.read()

c = c.replace("pool.token0", "pool.token0()")
c = c.replace("pool.token1", "pool.token1()")
c = c.replace("pool.token0()()", "pool.token0()") # Just in case it was already replaced
c = c.replace("pool.token1()()", "pool.token1()")

# Fix exchange rate import
c = c.replace("exchange_rate_0_to_1, exchange_rate_1_to_0", "get_exchange_rate")

# Fix exchange rate usage
c = c.replace("exchange_rate_0_to_1(pool)", "get_exchange_rate(pool)")
c = c.replace("exchange_rate_1_to_0(pool)", "1.0 / get_exchange_rate(pool)")

with open("src/router/graph.rs", "w") as f:
    f.write(c)

with open("src/simulator/evm.rs", "r") as f:
    c = f.read()
c = c.replace("leg.pool.token0", "leg.pool.token0()")
with open("src/simulator/evm.rs", "w") as f:
    f.write(c)

