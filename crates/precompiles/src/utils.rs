use crate::error::{Result, TempoPrecompileError};

use alloy::primitives::{I256, U256};

/// Trait providing checked arithmetic operations with Result-based error handling.
pub trait MathUtils {
    /// Checked addition. Returns an error on overflow.
    ///
    /// # Example
    /// ```ignore
    /// // Before:
    /// let result = balance.checked_add(amount).ok_or(TempoPrecompileError::under_overflow())?;
    ///
    /// // After:
    /// let result = balance.add_checked(amount)?;
    /// ```
    fn add_checked(self, rhs: Self) -> Result<Self>
    where
        Self: Sized;

    /// Checked subtraction. Returns an error on underflow.
    ///
    /// # Example
    /// ```ignore
    /// // Before:
    /// let result = balance.checked_sub(amount).ok_or(TempoPrecompileError::under_overflow())?;
    ///
    /// // After:
    /// let result = balance.sub_checked(amount)?;
    /// ```
    fn sub_checked(self, rhs: Self) -> Result<Self>
    where
        Self: Sized;

    /// Checked multiplication. Returns an error on overflow.
    ///
    /// # Example
    /// ```ignore
    /// // Before:
    /// let result = amount.checked_mul(price).ok_or(TempoPrecompileError::under_overflow())?;
    ///
    /// // After:
    /// let result = amount.mul_checked(price)?;
    /// ```
    fn mul_checked(self, rhs: Self) -> Result<Self>
    where
        Self: Sized;

    /// Checked division. Returns an error on overflow or division by zero.
    ///
    /// # Example
    /// ```ignore
    /// // Before:
    /// let result = total.checked_div(count).ok_or(TempoPrecompileError::under_overflow())?;
    ///
    /// // After:
    /// let result = total.div_checked(count)?;
    /// ```
    fn div_checked(self, rhs: Self) -> Result<Self>
    where
        Self: Sized;

    /// Performs `(self * mul) / div` with checked arithmetic.
    ///
    /// Reduces complex 4-line chains into a single expressive call.
    ///
    /// # Example
    /// ```ignore
    /// // Before (4 lines):
    /// let delta_rpt = amount
    ///     .checked_mul(ACC_PRECISION)
    ///     .and_then(|v| v.checked_div(opted_in_supply))
    ///     .ok_or(TempoPrecompileError::under_overflow())?;
    ///
    /// // After (1 line):
    /// let delta_rpt = amount.mul_div(ACC_PRECISION, opted_in_supply)?;
    /// ```
    fn mul_div(self, mul: Self, div: Self) -> Result<Self>
    where
        Self: Sized;
}

impl MathUtils for U256 {
    #[inline]
    fn add_checked(self, rhs: Self) -> Result<Self> {
        self.checked_add(rhs)
            .ok_or(TempoPrecompileError::under_overflow())
    }

    #[inline]
    fn sub_checked(self, rhs: Self) -> Result<Self> {
        self.checked_sub(rhs)
            .ok_or(TempoPrecompileError::under_overflow())
    }

    #[inline]
    fn mul_checked(self, rhs: Self) -> Result<Self> {
        self.checked_mul(rhs)
            .ok_or(TempoPrecompileError::under_overflow())
    }

    #[inline]
    fn div_checked(self, rhs: Self) -> Result<Self> {
        self.checked_div(rhs)
            .ok_or(TempoPrecompileError::under_overflow())
    }

    #[inline]
    fn mul_div(self, mul: Self, div: Self) -> Result<Self> {
        self.checked_mul(mul)
            .and_then(|v| v.checked_div(div))
            .ok_or(TempoPrecompileError::under_overflow())
    }
}

impl MathUtils for I256 {
    #[inline]
    fn add_checked(self, rhs: Self) -> Result<Self> {
        self.checked_add(rhs)
            .ok_or(TempoPrecompileError::under_overflow())
    }

    #[inline]
    fn sub_checked(self, rhs: Self) -> Result<Self> {
        self.checked_sub(rhs)
            .ok_or(TempoPrecompileError::under_overflow())
    }

    #[inline]
    fn mul_checked(self, rhs: Self) -> Result<Self> {
        self.checked_mul(rhs)
            .ok_or(TempoPrecompileError::under_overflow())
    }

    #[inline]
    fn div_checked(self, rhs: Self) -> Result<Self> {
        self.checked_div(rhs)
            .ok_or(TempoPrecompileError::under_overflow())
    }

    #[inline]
    fn mul_div(self, mul: Self, div: Self) -> Result<Self> {
        self.checked_mul(mul)
            .and_then(|v| v.checked_div(div))
            .ok_or(TempoPrecompileError::under_overflow())
    }
}
