# AMM Liquidity Pool (Constant Product)

A Rust implementation of a **Uniswap-style constant-product AMM** (`x*y=k`) with LP tokens, fees, slippage protection, and comprehensive tests.

## Features

- **Add liquidity** — mint LP tokens proportional to deposit
- **Remove liquidity** — burn LP tokens to reclaim reserves
- **Swap A↔B** — with configurable fee (default 0.3%)
- **Slippage protection** — `min_out` / `min_a` / `min_b` parameters
- **Price impact** calculation (basis points)
- **K-invariant** verification
- **15 unit tests** covering all edge cases

## Usage

```rust
use amm_liquidity_pool::AmmPool;

let mut pool = AmmPool::new(30).unwrap(); // 0.3% fee
pool.add_liquidity(1000, 1000, 0, 0).unwrap();
let out = pool.swap_a_for_b(100, 0).unwrap();
println!("Got {} B tokens", out);
```

## Build & Test

```bash
cargo build
cargo test
```

## License

MIT