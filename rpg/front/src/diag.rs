// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Diagnostics: an `RNF` message identifier, its severity, where it
//! points and the values its text substitutes. The identifiers and
//! severities are IBM's (`tools/conformance/rpg/oracle/qrpglemsg.psv`);
//! the texts are the compiler's own message file's.

use crate::ast::Pos;

/// One diagnostic.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Diag {
    /// Where it points.
    pub at: Pos,
    /// `RNFnnnn`.
    pub msgid: &'static str,
    /// Severity: 0, 10, 20, 30, 40 or 50.
    pub severity: u8,
    /// Values for the text, in order.
    pub args: Vec<String>,
}

impl Diag {
    /// A diagnostic with no values.
    #[must_use]
    pub fn new(msgid: &'static str, severity: u8, at: Pos) -> Self {
        Self {
            at,
            msgid,
            severity,
            args: Vec::new(),
        }
    }

    /// Add a value.
    #[must_use]
    pub fn arg(mut self, v: impl Into<String>) -> Self {
        self.args.push(v.into());
        self
    }
}

/// The identifiers the front end raises, by meaning.
pub mod ids {
    /// A character literal is missing its closing apostrophe.
    pub const MISSING_APOSTROPHE: &str = "RNF0267";
    /// A character not valid in RPG source.
    pub const INVALID_CHARACTER: &str = "RNF0201";
    /// A specification type out of sequence or not valid.
    pub const FORM_SEQUENCE: &str = "RNF0257";
    /// A free-form statement lacks its semicolon.
    pub const NO_SEMICOLON: &str = "RNF5507";
    /// The end of the expression was expected.
    pub const END_OF_EXPRESSION: &str = "RNF5377";
    /// An assignment operator was expected.
    pub const ASSIGNMENT_EXPECTED: &str = "RNF5347";
    /// The item cannot be the target of an assignment.
    pub const NOT_ASSIGNABLE: &str = "RNF0955";
    /// A float literal is not valid.
    pub const BAD_FLOAT: &str = "RNF0308";
    /// The operation code is not valid.
    pub const BAD_OPCODE: &str = "RNF5013";
    /// An operation code that free form does not allow.
    pub const NOT_IN_FREE: &str = "RNF5346";
    /// The /COPY or /INCLUDE member was not found.
    pub const COPY_NOT_FOUND: &str = "RNF0023";
    /// /COPY nesting too deep or recursive.
    pub const COPY_NESTING: &str = "RNF0024";
    /// A directive is not valid or its position is wrong.
    pub const BAD_DIRECTIVE: &str = "RNF0037";
    /// /ELSE, /ELSEIF or /ENDIF without /IF.
    pub const COND_NO_IF: &str = "RNF0039";
    /// /IF without /ENDIF at the end of the member.
    pub const COND_NO_ENDIF: &str = "RNF0036";
    /// A keyword is not valid.
    pub const BAD_KEYWORD: &str = "RNF0203";
    /// A parenthesis is missing.
    pub const PAREN: &str = "RNF0202";
    /// A token not expected here.
    pub const UNEXPECTED: &str = "RNF5358";
    /// An END-xx without its beginning, or a block left open.
    pub const BLOCK_MISMATCH: &str = "RNF5034";
    /// A name continued with `...` has no continuation.
    pub const NAME_CONTINUATION: &str = "RNF0280";
    /// Text in the comment columns of a free-form line.
    pub const FREE_COMMENT: &str = "RNF5501";
    /// Something follows a free-form statement on its line.
    pub const AFTER_STATEMENT: &str = "RNF5508";
    /// A numeric entry is not right-adjusted in its columns.
    pub const NOT_RIGHT_ADJUSTED: &str = "RNF0263";
    /// A right parenthesis is missing in an expression.
    pub const RPAREN_EXPECTED: &str = "RNF0312";
    /// An operand is missing in an expression.
    pub const MISSING_OPERAND: &str = "RNF0637";
    /// A procedure is not ended before the next one or the end.
    pub const PROC_END_MISSING: &str = "RNF1502";
    /// `END-PROC` without `DCL-PROC`.
    pub const PROC_END_UNMATCHED: &str = "RNF1509";
    /// `END-DS`, `END-PR` or `END-PI` without its group.
    pub const END_NOT_EXPECTED: &str = "RNF3551";
    /// A group of free-form declarations has no `END-xx`.
    pub const GROUP_END_MISSING: &str = "RNF3556";
    /// A main-procedure statement after the first procedure.
    pub const BETWEEN_PROCS: &str = "RNF0256";
    /// A definition name that is not valid; blanks are used.
    pub const BAD_DEF_NAME: &str = "RNF3401";
    /// An operator without its left operand.
    pub const OPERANDS_INCOMPATIBLE: &str = "RNF7421";
    /// A right parenthesis that closes nothing.
    pub const UNEXPECTED_RPAREN: &str = "RNF0633";
    /// An operation extender that is not valid.
    pub const BAD_EXTENDER: &str = "RNF5048";
    /// A fixed-form operation code that is not valid; the
    /// specification is ignored.
    pub const BAD_FIXED_OPCODE: &str = "RNF5014";
    /// A decimal-positions entry that is not blank or 0 to 63.
    pub const BAD_DECIMALS: &str = "RNF5044";
    /// A resulting indicator that is not valid.
    pub const BAD_RESULT_IND: &str = "RNF5051";
    /// `SETON` or `SETOFF` without an indicator.
    pub const NO_RESULT_IND: &str = "RNF5053";
    /// Text skipped after an error.
    pub const TEXT_SKIPPED: &str = "RNF0607";
    /// A second procedure interface for one procedure.
    pub const PI_TWICE: &str = "RNF3768";
}
