// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The shipped display files: DDS sources under `sys/`, compiled into
//! `QSYS` at the first IPL that finds them missing. The record and
//! field names are the compatibility interface (a customized sign-on
//! keeps them); every word shown is Quartz's.

/// The sign-on display at password level 0 and 1.
pub const QDSIGNON: &str = include_str!("../sys/QDSIGNON.dspf");

/// The sign-on display at password level 2 and 3.
pub const QDSIGNON2: &str = include_str!("../sys/QDSIGNON2.dspf");

/// The System Request menu.
pub const QWSSYSRQ: &str = include_str!("../sys/QWSSYSRQ.dspf");

/// Every shipped display file: name and DDS.
pub const FILES: &[(&str, &str)] = &[
    ("QDSIGNON", QDSIGNON),
    ("QDSIGNON2", QDSIGNON2),
    ("QWSSYSRQ", QWSSYSRQ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CompileOpts, NoRefs, Source, compile_dspf};

    #[test]
    fn shipped_files_compile() {
        for (name, text) in FILES {
            let (src, diags) = Source::from_text(text);
            assert!(diags.is_empty(), "{name}: {diags:?}");
            let c = compile_dspf(&src, &NoRefs, &CompileOpts::default());
            let df = c.definition.unwrap_or_else(|| panic!("{name}:\n{}", c.listing));
            if !name.starts_with("QDSIGNON") {
                continue;
            }
            let rec = df.record("SIGNON").expect("the SIGNON format");
            for f in ["QSYSNAME", "SBSNAME", "DEVNAME", "USERID", "PASSWRD", "PROGRAM", "MENU", "CURLIB", "UBUFFER"] {
                assert!(rec.field(f).is_some(), "{name} lacks {f}");
            }
        }
    }
}
