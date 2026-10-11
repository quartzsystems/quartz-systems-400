// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The differential fuzzer against a real IBM i: cases as CSV, the RPG
//! oracle that computes them on the machine, our expected rows, and the
//! diff. `conformance/oracle/qdecimal/README.md` has the procedure.

mod cases;
mod csv;
pub mod diff;
pub mod expect;
pub mod rpg;

pub use cases::generate;
pub use csv::{read_cases, read_rows, write_cases, write_rows};
pub use diff::{Mismatch, Report, diff};
pub use expect::{Row, expected};

use crate::decimal::Shape;

/// The operand shapes the oracle knows, by index. Integer digits stay at
/// most 31 so no product exceeds 63 digits at compile time on the machine.
pub const SHAPES: [(u8, u8); 8] = [
    (5, 2),
    (7, 0),
    (9, 4),
    (15, 5),
    (31, 10),
    (40, 20),
    (45, 14),
    (63, 32),
];

/// The field every arithmetic result is assigned to.
pub const RESULT_SHAPE: (u8, u8) = (15, 5);

/// Edit words the oracle applies, each to one shape of [`SHAPES`].
pub const WORDS: &[(usize, &str)] = &[
    (0, "   .  "),
    (0, "  0.  "),
    (0, "  $0.  "),
    (0, "**0.  "),
    (0, "   .  CR"),
    (0, "   .  -"),
    (0, "  0.  &NET"),
    (1, "    ,   "),
    (1, "    ,  0"),
    (1, "   ,   0-"),
    (1, "$   ,   "),
    (1, "**  ,   "),
    (1, "     0 &DAYS"),
    (3, " ,   ,   ,  0.     "),
    (3, " ,   ,   ,   .     CR"),
];

/// Shapes whose edit codes the oracle exercises.
pub const EDIT_SHAPES: [usize; 3] = [0, 1, 3];

/// Every edit code.
pub const EDIT_CODES: &str = "1234ABCDJKLMNOPQXYZ";

/// The shape at `index`.
#[must_use]
pub fn shape(index: usize) -> Shape {
    let (d, s) = SHAPES[index];
    Shape::new(d, s).expect("table shapes are valid")
}

/// The fill variant of an edit code case.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FillKind {
    /// Blanks.
    Blank,
    /// `*ASTFILL`.
    Asterisk,
    /// `*CURSYM`, the machine's default `$`.
    Currency,
}

impl FillKind {
    /// The suffix after the code letter in the case file.
    #[must_use]
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Blank => "",
            Self::Asterisk => "*",
            Self::Currency => "$",
        }
    }

    fn from_suffix(s: &str) -> Option<Self> {
        Some(match s {
            "" => Self::Blank,
            "*" => Self::Asterisk,
            "$" => Self::Currency,
            _ => return None,
        })
    }
}

/// What a case does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Op {
    /// `a + b`.
    Add,
    /// `a - b`.
    Sub,
    /// `a * b`.
    Mul,
    /// `a / b`.
    Div,
    /// `%EDITC(a : code [: fill])`.
    EditCode {
        /// The code.
        code: char,
        /// The fill.
        fill: FillKind,
    },
    /// `%EDITW(a : WORDS[word])`.
    EditWord {
        /// Index into [`WORDS`].
        word: usize,
    },
}

impl Op {
    /// The `op` column.
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Sub => "-",
            Self::Mul => "*",
            Self::Div => "/",
            Self::EditCode { .. } => "E",
            Self::EditWord { .. } => "W",
        }
    }

    /// Whether `b` is an operand (else it names the code or word).
    #[must_use]
    pub fn is_arithmetic(self) -> bool {
        matches!(self, Self::Add | Self::Sub | Self::Mul | Self::Div)
    }

    /// The `b` column of an edit case.
    #[must_use]
    pub fn edit_text(self) -> Option<String> {
        match self {
            Self::EditCode { code, fill } => Some(format!("{code}{}", fill.suffix())),
            Self::EditWord { word } => Some(word.to_string()),
            _ => None,
        }
    }

    /// From the `op` and `b` columns.
    #[must_use]
    pub fn parse(symbol: &str, b: &str) -> Option<Self> {
        Some(match symbol {
            "+" => Self::Add,
            "-" => Self::Sub,
            "*" => Self::Mul,
            "/" => Self::Div,
            "E" => {
                let mut chars = b.chars();
                let code = chars.next()?;
                let fill = FillKind::from_suffix(chars.as_str())?;
                Self::EditCode { code, fill }
            }
            "W" => Self::EditWord {
                word: b.parse().ok().filter(|w| *w < WORDS.len())?,
            },
            _ => return None,
        })
    }
}

/// One case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Case {
    /// Sequence number, from 1.
    pub id: u32,
    /// The operation.
    pub op: Op,
    /// Index into [`SHAPES`] for `a`.
    pub shape_a: usize,
    /// `a` as text, at its shape's scale.
    pub a: String,
    /// Index into [`SHAPES`] for `b`; 0 and unused for edits.
    pub shape_b: usize,
    /// `b` as text, or the edit code or word index.
    pub b: String,
}

impl Case {
    /// The case as the oracle sees it, for messages.
    #[must_use]
    pub fn describe(&self) -> String {
        let (da, sa) = SHAPES[self.shape_a];
        match self.op {
            Op::EditCode { code, fill } => format!(
                "%EDITC({} ({da},{sa}) : '{code}'{})",
                self.a,
                match fill {
                    FillKind::Blank => "",
                    FillKind::Asterisk => " : *ASTFILL",
                    FillKind::Currency => " : *CURSYM",
                }
            ),
            Op::EditWord { word } => {
                format!("%EDITW({} ({da},{sa}) : '{}')", self.a, WORDS[word].1)
            }
            op => {
                let (db, sb) = SHAPES[self.shape_b];
                format!(
                    "{} ({da},{sa}) {} {} ({db},{sb})",
                    self.a,
                    op.symbol(),
                    self.b
                )
            }
        }
    }
}
