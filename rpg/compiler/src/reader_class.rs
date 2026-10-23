// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the listing counts of the read source: specifications,
//! comments, compile-time data records, skipped lines.

use rpgfront::reader::{Class, Line, Read};

/// A line left out by conditional compilation.
#[must_use]
pub fn skipped(l: &Line) -> bool {
    l.class == Class::Skipped
}

/// Specifications and comments of the main member.
#[must_use]
pub fn counts(r: &Read) -> (usize, usize) {
    let mut specs = 0;
    let mut comments = 0;
    for l in r.lines.iter().filter(|l| l.member == 0) {
        match l.class {
            Class::Comment => comments += 1,
            Class::Spec(_) | Class::Free | Class::Sql | Class::Directive(_) | Class::FreeMarker => specs += 1,
            _ => {}
        }
    }
    (specs, comments)
}

/// Compile-time data records.
#[must_use]
pub fn data_records(r: &Read) -> usize {
    r.lines.iter().filter(|l| l.class == Class::CtData).count()
}
