// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The command analyzer's oracle kit: `conformance/cmd/strings.txt`
//! holds command strings, one per line; each is analyzed against the
//! shipped definitions as a CL program's statement would be (every
//! `ALLOW` setting on, `&VARIABLE`s declared), and the outcome is one
//! line: `OK` and the command as the analyzer spells it, or the escape
//! and the diagnostics. `expected.txt` is this system's own outcome;
//! `ORACLE.CLLE` and `README.md` say how a real box produces the same
//! file for comparison.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use cmd::analyze::{Env, MemSource, Setting, VarInfo, VarType};

/// Where the kit lives.
#[must_use]
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("cmd")
}

/// The strings, in file order.
pub fn strings() -> std::io::Result<Vec<String>> {
    let text = std::fs::read_to_string(root().join("strings.txt"))?;
    Ok(text
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect())
}

/// The shipped definitions.
#[must_use]
pub fn defs() -> MemSource {
    let mut s = MemSource::default();
    for (name, _) in sys_cpps::defs::DEFS {
        if let Ok(o) = sys_cpps::install::object(name, 0) {
            s.put(o.def);
        }
    }
    s
}

/// The variables the strings name: numbers for `&P` (a priority) and
/// `&RC` (a return code), ten characters otherwise.
#[allow(clippy::unnecessary_wraps)] // the analyzer's lookup may say "not declared"
fn var(name: &str) -> Option<VarInfo> {
    Some(match name {
        "P" => VarInfo {
            ty: VarType::Dec,
            len: 2,
            dec: 0,
        },
        "RC" => VarInfo {
            ty: VarType::Int,
            len: 4,
            dec: 0,
        },
        _ => VarInfo {
            ty: VarType::Char,
            len: 10,
            dec: 0,
        },
    })
}

/// Analyze every string; the outcome lines.
#[must_use]
pub fn run(strings: &[String]) -> Vec<String> {
    let mut src = defs();
    let mut out = Vec::with_capacity(strings.len());
    for s in strings {
        let mut env = Env::new(&mut src);
        env.setting = Setting {
            interactive: true,
            program: true,
            exec: false,
            module: false,
        };
        env.vars = Some(&var);
        let line = match cmd::analyze(&mut env, s) {
            Ok(r) => format!("OK {}", r.text()),
            Err(e) => {
                let mut l = e.escape.msgid.clone();
                for d in &e.diags {
                    let _ = write!(l, " {}", d.msgid);
                }
                l
            }
        };
        out.push(line);
    }
    out
}
