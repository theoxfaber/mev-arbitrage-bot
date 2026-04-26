import re

with open("src/router/pool.rs", "r") as f:
    content = f.read()

# Add helper u256_to_f64
if "fn u256_to_f64" not in content:
    content += "\nfn u256_to_f64(val: U256) -> f64 {\n    let bits = val.bit_len();\n    if bits <= 64 {\n        val.to::<u64>() as f64\n    } else {\n        let shift = bits - 64;\n        let high = (val >> shift).to::<u64>() as f64;\n        high * 2f64.powi(shift as i32)\n    }\n}\n"

# Fix .to::<f64>()
content = re.sub(r'([a-zA-Z0-9_\[\]\.]+)\.to::<f64>\(\)', r'u256_to_f64(\1)', content)

# Fix patterns missing ..
content = content.replace("PoolState::UniswapV2 { reserve0, reserve1, fee_bps }", "PoolState::UniswapV2 { reserve0, reserve1, fee_bps, .. }")
content = content.replace("PoolState::UniswapV3 {", "PoolState::UniswapV3 { .. ") # Wait, need careful regex for multiline
content = re.sub(r'PoolState::UniswapV3 \{(.*?)\}', r'PoolState::UniswapV3 {\1, ..}', content, flags=re.DOTALL)
# Actually, I can just do this safely with string replacement because I know the exact code I wrote.

with open("src/router/pool.rs", "w") as f:
    f.write(content)
