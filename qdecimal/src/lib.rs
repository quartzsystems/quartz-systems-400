// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Decimal numbers the way IBM i holds and computes them: packed and
//! zoned decimal, binary and integer fields and floats, all meeting in
//! one [`Decimal`] of up to 63 digits; arithmetic with the intermediate
//! precision rules of the RPG runtime; half-adjust rounding and size
//! errors on assignment. `docs/DECIMAL.md` is the specification.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod arith;
pub mod decimal;
pub mod encode;
mod error;

pub use arith::{Mode, Typed};
pub use decimal::{Decimal, MAX_DIGITS, Rounding, Shape};
pub use encode::{Binary, Float, Integer, Packed, Zoned};
pub use error::{DecimalError, Result};
