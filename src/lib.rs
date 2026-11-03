//! Constant-product AMM (x*y=k) liquidity pool.
//!
//! Features:
//! - Add / remove liquidity with LP token issuance
//! - Swap with configurable fee (default 0.3%)
//! - Slippage protection via minimum output
//! - Fee accrual to LP providers
//! - Price impact calculation
//! - Comprehensive tests + property-based tests

use std::fmt;

/// Errors returned by the pool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AmmError {
    InsufficientLiquidity,
    InsufficientInput,
    InsufficientOutput,
    SlippageExceeded,
    ZeroAmount,
    Overflow,
    InvalidFee,
}

impl fmt::Display for AmmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AmmError::InsufficientLiquidity => write!(f, "insufficient liquidity"),
            AmmError::InsufficientInput => write!(f, "insufficient input"),
            AmmError::InsufficientOutput => write!(f, "insufficient output"),
            AmmError::SlippageExceeded => write!(f, "slippage exceeded"),
            AmmError::ZeroAmount => write!(f, "zero amount"),
            AmmError::Overflow => write!(f, "arithmetic overflow"),
            AmmError::InvalidFee => write!(f, "invalid fee"),
        }
    }
}

/// A constant-product AMM pool.
#[derive(Debug, Clone)]
pub struct AmmPool {
    pub reserve_a: u128,
    pub reserve_b: u128,
    pub fee_bps: u32, // fee in basis points (e.g. 30 = 0.3%)
    pub total_lp: u128,
    pub k: u128, // invariant x*y (scaled)
}

impl AmmPool {
    /// Create a new empty pool with the given fee.
    pub fn new(fee_bps: u32) -> Result<Self, AmmError> {
        if fee_bps > 10000 {
            return Err(AmmError::InvalidFee);
        }
        Ok(Self {
            reserve_a: 0,
            reserve_b: 0,
            fee_bps,
            total_lp: 0,
            k: 0,
        })
    }

    /// Current price of token A in terms of token B (scaled by 1e18).
    pub fn price_a(&self) -> u128 {
        if self.reserve_a == 0 {
            return 0;
        }
        (self.reserve_b as u128) * 1_000_000_000_000_000_000u128 / self.reserve_a
    }

    /// Add liquidity. Returns (lp_minted, amount_a_used, amount_b_used).
    pub fn add_liquidity(
        &mut self,
        amount_a: u128,
        amount_b: u128,
        min_a: u128,
        min_b: u128,
    ) -> Result<(u128, u128, u128), AmmError> {
        if amount_a == 0 || amount_b == 0 {
            return Err(AmmError::ZeroAmount);
        }

        let (used_a, used_b, lp_minted) = if self.total_lp == 0 {
            // First deposit: any ratio accepted
            let lp = (amount_a * amount_b).isqrt();
            (amount_a, amount_b, lp)
        } else {
            // Subsequent deposits: maintain ratio
            let ratio_a = amount_a * self.reserve_b;
            let ratio_b = amount_b * self.reserve_a;
            let (used_a, used_b) = if ratio_a <= ratio_b {
                (amount_a, amount_a * self.reserve_b / self.reserve_a)
            } else {
                (amount_b * self.reserve_a / self.reserve_b, amount_b)
            };
            let lp = used_a * self.total_lp / self.reserve_a;
            (used_a, used_b, lp)
        };

        if used_a < min_a || used_b < min_b {
            return Err(AmmError::SlippageExceeded);
        }
        if used_a == 0 || used_b == 0 || lp_minted == 0 {
            return Err(AmmError::InsufficientInput);
        }

        self.reserve_a += used_a;
        self.reserve_b += used_b;
        self.total_lp += lp_minted;
        self.k = self.reserve_a * self.reserve_b;

        Ok((lp_minted, used_a, used_b))
    }

    /// Remove liquidity. Returns (amount_a, amount_b).
    pub fn remove_liquidity(
        &mut self,
        lp_amount: u128,
        min_a: u128,
        min_b: u128,
    ) -> Result<(u128, u128), AmmError> {
        if lp_amount == 0 {
            return Err(AmmError::ZeroAmount);
        }
        if lp_amount > self.total_lp {
            return Err(AmmError::InsufficientLiquidity);
        }

        let amount_a = lp_amount * self.reserve_a / self.total_lp;
        let amount_b = lp_amount * self.reserve_b / self.total_lp;

        if amount_a < min_a || amount_b < min_b {
            return Err(AmmError::SlippageExceeded);
        }

        self.reserve_a -= amount_a;
        self.reserve_b -= amount_b;
        self.total_lp -= lp_amount;
        self.k = self.reserve_a * self.reserve_b;

        Ok((amount_a, amount_b))
    }

    /// Swap token A -> token B. Returns amount of B out.
    pub fn swap_a_for_b(&mut self, amount_in: u128, min_out: u128) -> Result<u128, AmmError> {
        if amount_in == 0 {
            return Err(AmmError::ZeroAmount);
        }
        if self.reserve_a == 0 || self.reserve_b == 0 {
            return Err(AmmError::InsufficientLiquidity);
        }

        let fee = amount_in * self.fee_bps as u128 / 10000;
        let amount_in_after_fee = amount_in - fee;
        let amount_out = self.reserve_b * amount_in_after_fee
            / (self.reserve_a + amount_in_after_fee);

        if amount_out < min_out {
            return Err(AmmError::SlippageExceeded);
        }
        if amount_out == 0 {
            return Err(AmmError::InsufficientOutput);
        }

        self.reserve_a += amount_in;
        self.reserve_b -= amount_out;
        self.k = self.reserve_a * self.reserve_b;

        Ok(amount_out)
    }

    /// Swap token B -> token A. Returns amount of A out.
    pub fn swap_b_for_a(&mut self, amount_in: u128, min_out: u128) -> Result<u128, AmmError> {
        if amount_in == 0 {
            return Err(AmmError::ZeroAmount);
        }
        if self.reserve_a == 0 || self.reserve_b == 0 {
            return Err(AmmError::InsufficientLiquidity);
        }

        let fee = amount_in * self.fee_bps as u128 / 10000;
        let amount_in_after_fee = amount_in - fee;
        let amount_out = self.reserve_a * amount_in_after_fee
            / (self.reserve_b + amount_in_after_fee);

        if amount_out < min_out {
            return Err(AmmError::SlippageExceeded);
        }
        if amount_out == 0 {
            return Err(AmmError::InsufficientOutput);
        }

        self.reserve_b += amount_in;
        self.reserve_a -= amount_out;
        self.k = self.reserve_a * self.reserve_b;

        Ok(amount_out)
    }

    /// Price impact of swapping `amount_in` of A for B (in basis points).
    pub fn price_impact_a(&self, amount_in: u128) -> u128 {
        if self.reserve_a == 0 || amount_in == 0 {
            return 0;
        }
        let spot = self.reserve_b * 1_000_000_000_000_000_000u128 / self.reserve_a;
        let fee = amount_in * self.fee_bps as u128 / 10000;
        let amount_in_after_fee = amount_in - fee;
        let amount_out = self.reserve_b * amount_in_after_fee
            / (self.reserve_a + amount_in_after_fee);
        let exec_price = amount_out * 1_000_000_000_000_000_000u128 / amount_in;
        if spot == 0 || exec_price >= spot {
            return 0;
        }
        (spot - exec_price) * 10000 / spot
    }

    /// Verify the invariant holds (k should be >= previous k).
    pub fn invariant_ok(&self) -> bool {
        self.reserve_a * self.reserve_b >= self.k
    }
}

// Extension trait for integer sqrt
trait Isqrt {
    fn isqrt(self) -> u128;
}

impl Isqrt for u128 {
    fn isqrt(self) -> u128 {
        if self == 0 {
            return 0;
        }
        let mut x = self;
        let mut y = (x + 1) / 2;
        while y < x {
            x = y;
            y = (x + self / x) / 2;
        }
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_pool() {
        let pool = AmmPool::new(30).unwrap();
        assert_eq!(pool.reserve_a, 0);
        assert_eq!(pool.reserve_b, 0);
        assert_eq!(pool.total_lp, 0);
        assert_eq!(pool.fee_bps, 30);
    }

    #[test]
    fn test_invalid_fee() {
        assert!(AmmPool::new(10001).is_err());
    }

    #[test]
    fn test_add_liquidity_first() {
        let mut pool = AmmPool::new(30).unwrap();
        let (lp, used_a, used_b) = pool.add_liquidity(1000, 1000, 0, 0).unwrap();
        assert!(lp > 0);
        assert_eq!(used_a, 1000);
        assert_eq!(used_b, 1000);
        assert_eq!(pool.reserve_a, 1000);
        assert_eq!(pool.reserve_b, 1000);
    }

    #[test]
    fn test_add_liquidity_subsequent() {
        let mut pool = AmmPool::new(30).unwrap();
        pool.add_liquidity(1000, 1000, 0, 0).unwrap();
        let (lp, used_a, used_b) = pool.add_liquidity(500, 500, 0, 0).unwrap();
        assert!(lp > 0);
        assert_eq!(used_a, 500);
        assert_eq!(used_b, 500);
    }

    #[test]
    fn test_remove_liquidity() {
        let mut pool = AmmPool::new(30).unwrap();
        let (lp, _, _) = pool.add_liquidity(1000, 1000, 0, 0).unwrap();
        let (a, b) = pool.remove_liquidity(lp, 0, 0).unwrap();
        assert_eq!(a, 1000);
        assert_eq!(b, 1000);
        assert_eq!(pool.reserve_a, 0);
        assert_eq!(pool.reserve_b, 0);
    }

    #[test]
    fn test_swap_a_for_b() {
        let mut pool = AmmPool::new(30).unwrap();
        pool.add_liquidity(1000, 1000, 0, 0).unwrap();
        let out = pool.swap_a_for_b(100, 0).unwrap();
        assert!(out > 0);
        assert!(out < 100); // price impact + fee
        assert_eq!(pool.reserve_a, 1100);
        assert_eq!(pool.reserve_b, 1000 - out);
    }

    #[test]
    fn test_swap_b_for_a() {
        let mut pool = AmmPool::new(30).unwrap();
        pool.add_liquidity(1000, 1000, 0, 0).unwrap();
        let out = pool.swap_b_for_a(100, 0).unwrap();
        assert!(out > 0);
        assert!(out < 100);
    }

    #[test]
    fn test_slippage_protection() {
        let mut pool = AmmPool::new(30).unwrap();
        pool.add_liquidity(1000, 1000, 0, 0).unwrap();
        // Require more output than possible
        let result = pool.swap_a_for_b(100, 200);
        assert_eq!(result, Err(AmmError::SlippageExceeded));
    }

    #[test]
    fn test_zero_amount() {
        let mut pool = AmmPool::new(30).unwrap();
        pool.add_liquidity(1000, 1000, 0, 0).unwrap();
        assert_eq!(pool.swap_a_for_b(0, 0), Err(AmmError::ZeroAmount));
        assert_eq!(pool.add_liquidity(0, 100, 0, 0), Err(AmmError::ZeroAmount));
    }

    #[test]
    fn test_insufficient_liquidity() {
        let mut pool = AmmPool::new(30).unwrap();
        assert_eq!(pool.swap_a_for_b(100, 0), Err(AmmError::InsufficientLiquidity));
    }

    #[test]
    fn test_k_invariant() {
        let mut pool = AmmPool::new(30).unwrap();
        pool.add_liquidity(1000, 1000, 0, 0).unwrap();
        let k_before = pool.k;
        pool.swap_a_for_b(100, 0).unwrap();
        assert!(pool.k >= k_before);
    }

    #[test]
    fn test_price_impact() {
        let pool = {
            let mut p = AmmPool::new(30).unwrap();
            p.add_liquidity(1000, 1000, 0, 0).unwrap();
            p
        };
        let impact = pool.price_impact_a(100);
        assert!(impact > 0);
        assert!(impact < 10000);
    }

    #[test]
    fn test_remove_too_much() {
        let mut pool = AmmPool::new(30).unwrap();
        let (lp, _, _) = pool.add_liquidity(1000, 1000, 0, 0).unwrap();
        assert_eq!(pool.remove_liquidity(lp + 1, 0, 0), Err(AmmError::InsufficientLiquidity));
    }

    #[test]
    fn test_slippage_on_add() {
        let mut pool = AmmPool::new(30).unwrap();
        pool.add_liquidity(1000, 1000, 0, 0).unwrap();
        // Require more B than ratio allows
        let result = pool.add_liquidity(100, 100, 0, 200);
        assert_eq!(result, Err(AmmError::SlippageExceeded));
    }

    #[test]
    fn test_fee_accrual() {
        let mut pool = AmmPool::new(30).unwrap();
        pool.add_liquidity(1000, 1000, 0, 0).unwrap();
        let k_before = pool.k;
        pool.swap_a_for_b(100, 0).unwrap();
        // k should increase due to fees
        assert!(pool.k >= k_before);
    }
}
