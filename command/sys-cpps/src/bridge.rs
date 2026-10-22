// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The bridge: a processing program that unpacks the parameter block
//! into the keyword form the library functions of Phases 2 to 6 take
//! (`word KWD(value) ...`), which they parse with `wm::cmd::Params`.
//! Only parameters the caller gave or that have a default appear.

use cmd::block::Parms;
use cmd::value::Value;

/// The line for `parms` as the function behind `word` reads it.
#[must_use]
pub fn line(word: &str, parms: &Parms) -> String {
    let mut s = word.to_string();
    for b in &parms.values {
        let Some(v) = &b.value else {
            continue;
        };
        if matches!(v, Value::Null) {
            continue;
        }
        s.push(' ');
        s.push_str(&b.kwd);
        s.push('(');
        s.push_str(&v.parm_source());
        s.push(')');
    }
    s
}

/// The line with only the parameters the caller specified.
#[must_use]
pub fn specified_line(word: &str, parms: &Parms) -> String {
    let mut s = word.to_string();
    for b in &parms.values {
        let (true, Some(v)) = (b.specified, &b.value) else {
            continue;
        };
        s.push(' ');
        s.push_str(&b.kwd);
        s.push('(');
        s.push_str(&v.parm_source());
        s.push(')');
    }
    s
}
