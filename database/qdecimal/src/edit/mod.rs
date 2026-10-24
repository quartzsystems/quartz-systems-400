// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Editing: a number to the characters a report or screen shows, by
//! edit code (`%EDITC`, `EDTCDE`) or edit word (`%EDITW`, `EDTWRD`).

mod code;
mod word;

pub use code::{code_width, edit_code};
pub use word::edit_word;

use crate::decimal::{Decimal, Rounding, Shape};
use crate::error::{DecimalError, Result};

/// What fills the positions of suppressed leading zeros.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Fill {
    /// Blanks.
    #[default]
    Blank,
    /// `*ASTFILL`: asterisks.
    Asterisk,
    /// `*CURSYM`: blanks, with the symbol just left of the first digit.
    Currency(char),
}

/// How edit code `Y` splits the digits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DateEditFmt {
    /// Month, day, year.
    #[default]
    Mdy,
    /// Day, month, year.
    Dmy,
    /// Year, month, day.
    Ymd,
}

/// Options of an edit code.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct EditOpts {
    /// Fill for suppressed zeros.
    pub fill: Fill,
    /// `DATEDIT` format for edit code `Y`.
    pub date_fmt: DateEditFmt,
    /// `DATEDIT` separator for edit code `Y`.
    pub date_sep: Option<char>,
    /// `DECEDIT(',')`: the decimal point is a comma and the thousands
    /// separator a period.
    pub decimal_comma: bool,
    /// `DECEDIT('0.')` or `('0,')`: a zero before the point when the
    /// integer part is zero.
    pub lead_zero: bool,
}

impl EditOpts {
    /// The separator for edit code `Y`.
    #[must_use]
    pub fn separator(&self) -> char {
        self.date_sep.unwrap_or('/')
    }
}

fn bad(what: String) -> DecimalError {
    DecimalError::Edit { what }
}

/// The value's digits, most significant first, padded to the shape.
fn digits_of(value: &Decimal, shape: Shape) -> Result<(bool, Vec<u8>)> {
    let v = value.fit(shape, Rounding::Truncate)?;
    let mut out = vec![0u8; usize::from(shape.digits)];
    let n = out.len();
    for (i, &d) in v.digits_le().iter().enumerate() {
        out[n - 1 - i] = d;
    }
    Ok((v.is_negative(), out))
}

fn digit_char(d: u8) -> char {
    char::from(b'0' + d)
}
