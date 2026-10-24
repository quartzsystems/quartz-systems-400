// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The lines IBM's precompiler adds to the source it hands the RPG
//! compiler, counted so that our listing numbers statements as the
//! oracle's does (`docs/RPG.md`, embedded SQL). Before the main
//! source's first calculation: the SQLCA, the runtime's prototypes and
//! constants ([`SQLCA_LINES`]); before each scope's first calculation,
//! a structure per statement of the scope (a `DS` line, five header
//! fields, a field per host variable, at least one), which a fully
//! free member sets off with `**END-FREE` and `**FREE`; after each
//! statement, the code that calls the runtime and the `WHENEVER`
//! tests.

use crate::{Op, Precompiled, Stmt};

/// The SQLCA, the four prototypes and the constants.
pub const SQLCA_LINES: u32 = 65;

/// The lines of a statement's declarations, and whether they are a
/// fixed-form structure (set off in a fully free member).
#[must_use]
pub fn ds_lines(s: &Stmt) -> (u32, bool) {
    if !s.ds {
        return (0, false);
    }
    match s.desc.op {
        // A prototype of the call: DCL-PR, the SQLCA, the text, END-PR.
        Op::Prepare | Op::ExecImm => (4, false),
        // A free-form header and a prototype naming the array.
        Op::Fetch if s.rows.is_some() => (13, false),
        _ => (1 + 5 + (s.n_in + s.n_out).max(1), true),
    }
}

fn is_commit(s: &Stmt) -> bool {
    s.desc.op == Op::Exec && matches!(s.verb.as_str(), "COMMIT" | "ROLLBACK")
}

/// The lines of code after a statement.
#[must_use]
pub fn code_lines(s: &Stmt) -> u32 {
    let w = &s.whenever;
    let tests = 3 * u32::try_from([&w.error, &w.warning, &w.not_found].iter().filter(|x| x.is_some()).count()).unwrap_or(0);
    let base = match s.desc.op {
        Op::Nothing => return 0,
        // SQLER5, SQLER6, the call with the array (5 lines).
        Op::Fetch if s.rows.is_some() => 7,
        // The inputs, SQLER6, the call (4 lines), IF data okay ...
        // ENDIF with an EVAL per output.
        Op::Query | Op::Fetch | Op::Diag => 7 + s.n_out + s.n_in,
        // SQLER6, IF/OR, the call, ELSE, the open call, ENDIF.
        Op::Open => 13 + s.n_in,
        // SQLER6, IF, the call, ELSE, the close or commit call, ENDIF.
        Op::Close => 12,
        _ if is_commit(s) => 12,
        // SQLER6 and the call with the text.
        Op::Prepare | Op::ExecImm => 5,
        // The USING values go in and come back.
        Op::Execute if s.n_in > 0 => 5 + 2 + 2 * s.n_in,
        _ if s.n_out > 0 => 7 + s.n_out + s.n_in,
        _ => 5 + s.n_in,
    };
    base + tests
}

/// Lines generated before source lines: `(line, count)`. `first_calc`
/// gives each scope's first calculation (the main source is scope 0);
/// `fully_free` a `**FREE` member.
#[must_use]
pub fn generated(p: &Precompiled, first_calc: &dyn Fn(usize) -> u32, fully_free: bool) -> Vec<(u32, u32)> {
    if p.stmts.is_empty() {
        return Vec::new();
    }
    let mut heads: std::collections::BTreeMap<usize, u32> = std::collections::BTreeMap::new();
    heads.insert(0, SQLCA_LINES);
    for s in &p.stmts {
        let (n, fixed) = ds_lines(s);
        if n > 0 {
            *heads.entry(s.scope).or_insert(0) += n + if fixed && fully_free { 2 } else { 0 };
        }
    }
    let mut out: Vec<(u32, u32)> = heads.into_iter().map(|(scope, n)| (first_calc(scope), n)).collect();
    for s in &p.stmts {
        let n = code_lines(s);
        if n > 0 {
            out.push((s.last_line + 1, n));
        }
    }
    out
}
