// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What QS/400 computes for a case, in the oracle's output format.

use super::{Case, FillKind, Op, RESULT_SHAPE, WORDS, shape};
use crate::arith::{self, Mode, Typed};
use crate::decimal::{Decimal, Rounding, Shape};
use crate::edit::{EditOpts, Fill, edit_code, edit_word};
use crate::error::DecimalError;

/// One result row, ours or the machine's.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Row {
    /// The case.
    pub id: u32,
    /// `%CHAR` of the intermediate result, or `[edited]`.
    pub inter: String,
    /// The result assigned to the 15,5 field.
    pub r1: String,
    /// The same with half-adjust.
    pub rh: String,
    /// The same under `*RESDECPOS`.
    pub rr: String,
}

/// The token the oracle writes for a failed operation: `E` and the RPG
/// status code.
fn token(e: &DecimalError) -> String {
    match e {
        DecimalError::SizeOverflow { .. } => "E00103".to_string(),
        DecimalError::DivideByZero => "E00102".to_string(),
        DecimalError::DecimalData { .. } => "E00907".to_string(),
        _ => "E00105".to_string(),
    }
}

fn operation(op: Op, a: &Typed, b: &Typed, mode: Mode) -> Result<Typed, DecimalError> {
    match op {
        Op::Add => arith::add(a, b, mode),
        Op::Sub => arith::sub(a, b, mode),
        Op::Mul => arith::mul(a, b, mode),
        Op::Div => arith::div(a, b, mode),
        Op::EditCode { .. } | Op::EditWord { .. } => unreachable!("not arithmetic"),
    }
}

/// Our row for a case.
#[must_use]
pub fn expected(case: &Case) -> Row {
    let mut row = Row {
        id: case.id,
        ..Row::default()
    };
    let sa = shape(case.shape_a);
    let a = Decimal::parse(&case.a).and_then(|v| Typed::new(&v, sa));
    match case.op {
        Op::EditCode { code, fill } => {
            let opts = EditOpts {
                fill: match fill {
                    FillKind::Blank => Fill::Blank,
                    FillKind::Asterisk => Fill::Asterisk,
                    FillKind::Currency => Fill::Currency('$'),
                },
                ..EditOpts::default()
            };
            row.inter = match a.and_then(|a| edit_code(&a.value, sa, code, &opts)) {
                Ok(s) => format!("[{s}]"),
                Err(e) => token(&e),
            };
        }
        Op::EditWord { word } => {
            row.inter = match a.and_then(|a| edit_word(&a.value, sa, WORDS[word].1)) {
                Ok(s) => format!("[{s}]"),
                Err(e) => token(&e),
            };
        }
        op => {
            let sb = shape(case.shape_b);
            let b = Decimal::parse(&case.b).and_then(|v| Typed::new(&v, sb));
            let (Ok(a), Ok(b)) = (a, b) else {
                row.inter = "E00105".to_string();
                return row;
            };
            let result = Shape::new(RESULT_SHAPE.0, RESULT_SHAPE.1).expect("valid");
            // `%CHAR` of the expression: no result field.
            row.inter = match operation(op, &a, &b, Mode::default()) {
                Ok(t) => t.value.to_string(),
                Err(e) => token(&e),
            };
            let to_result = Mode {
                result_scale: RESULT_SHAPE.1,
                ..Mode::default()
            };
            let assign = |mode: Mode, rounding: Rounding| match operation(op, &a, &b, mode) {
                Ok(t) => t
                    .assign(result, rounding)
                    .map_or_else(|e| token(&e), |v| v.to_string()),
                Err(e) => token(&e),
            };
            row.r1 = assign(to_result, Rounding::Truncate);
            row.rh = assign(
                Mode {
                    half_adjust: true,
                    ..to_result
                },
                Rounding::HalfAdjust,
            );
            row.rr = assign(
                Mode {
                    resdecpos: true,
                    ..to_result
                },
                Rounding::Truncate,
            );
        }
    }
    row
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_look_like_the_oracle() {
        let c = Case {
            id: 1,
            op: Op::Div,
            shape_a: 1,
            a: "2".into(),
            shape_b: 1,
            b: "3".into(),
        };
        let r = expected(&c);
        assert!(r.inter.starts_with("0.666666"));
        assert_eq!(r.r1, "0.66666");
        assert_eq!(r.rh, "0.66667");
        assert_eq!(r.rr, "0.66666");
        let c = Case { b: "0".into(), ..c };
        let r = expected(&c);
        assert_eq!(r.inter, "E00102");
        assert_eq!(r.r1, "E00102");
        let c = Case {
            id: 3,
            op: Op::EditCode {
                code: 'J',
                fill: FillKind::Asterisk,
            },
            shape_a: 0,
            a: "-12.34".into(),
            shape_b: 0,
            b: "J*".into(),
        };
        assert_eq!(expected(&c).inter, "[*12.34-]");
        let c = Case {
            id: 4,
            op: Op::Mul,
            shape_a: 7,
            a: "9".repeat(31) + "." + &"9".repeat(32),
            shape_b: 7,
            b: "9".repeat(31) + "." + &"9".repeat(32),
        };
        let r = expected(&c);
        assert!(r.inter.starts_with("9999"));
        assert_eq!(r.r1, "E00103");
    }
}
