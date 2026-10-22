// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! CL source into statements: lines of a source member (80 columns,
//! the data part), comments gone, continuations joined, each statement
//! tokenized by the command tokenizer with its label and line.

use cmd::token::{self, Options, Tokens};

use crate::diag::Diag;

/// One statement's tokens and where it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Raw {
    /// Statement number, 1-based.
    pub n: usize,
    /// First source line.
    pub line: usize,
    /// Last source line.
    pub last: usize,
    /// The joined text.
    pub text: String,
    /// The tokens.
    pub toks: Tokens,
}

/// Tokenize a source; errors are collected per statement.
pub fn statements(source: &str) -> (Vec<Raw>, Vec<Diag>) {
    let lines: Vec<&str> = source.lines().map(str::trim_end).collect();
    let mut out = Vec::new();
    let mut errs = Vec::new();
    for (i, j) in token::join(&lines).into_iter().enumerate() {
        match token::tokenize(&j.text, Options::default()) {
            Ok(toks) => out.push(Raw {
                n: i + 1,
                line: j.first,
                last: j.last,
                text: j.text,
                toks,
            }),
            Err(e) => {
                for d in e.diags {
                    errs.push(Diag::at(j.first, &d.msgid, d.detail));
                }
            }
        }
    }
    (out, errs)
}
