// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The copy members QS/400 ships in `QOAR/QRPGLESRC` for Open Access
//! handlers (`QRNOPENACC`), `DATA-INTO` parsers (`QRNDTAINTO`),
//! `DATA-GEN` generators (`QRNDTAGEN`) and their shared constants
//! (`QRNTYPES`): Quartz's own source with IBM's names and layouts.
//! A `/COPY` that the source's own members do not answer finds them.

use rpgfront::{Includes, Member};

/// The members, by name.
pub const MEMBERS: &[(&str, &str)] = &[
    ("QRNTYPES", include_str!("../qoar/QRNTYPES.rpgle")),
    ("QRNDTAINTO", include_str!("../qoar/QRNDTAINTO.rpgle")),
    ("QRNDTAGEN", include_str!("../qoar/QRNDTAGEN.rpgle")),
    ("QRNOPENACC", include_str!("../qoar/QRNOPENACC.rpgle")),
];

/// A shipped member, when `lib/file,name` names one.
#[must_use]
pub fn member(lib: &str, file: &str, name: &str) -> Option<Member> {
    if !(lib.eq_ignore_ascii_case("QOAR") || lib == "*LIBL")
        || !file.eq_ignore_ascii_case("QRPGLESRC")
    {
        return None;
    }
    MEMBERS
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(n, text)| Member::from_text("QOAR", "QRPGLESRC", n, text))
}

/// Includes that fall back to the shipped members.
pub struct WithQoar<'a>(pub &'a mut dyn Includes);

impl Includes for WithQoar<'_> {
    fn member(&mut self, lib: &str, file: &str, name: &str) -> Option<Member> {
        self.0
            .member(lib, file, name)
            .or_else(|| member(lib, file, name))
    }
}
