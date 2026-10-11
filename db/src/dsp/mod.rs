// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The display commands as data plus a text rendering: `DSPFD`,
//! `DSPFFD`, `DSPPFM` and `DSPDBR`. The text is Quartz's; the names
//! and parameter values are IBM's.

pub mod dbr;
pub mod fd;
pub mod ffd;
pub mod pfm;

pub use dbr::{DbRelations, Relation, dspdbr};
pub use fd::{FdType, FileDescription, MemberInfo, dspfd};
pub use ffd::{FieldDescription, FieldInfo, FormatFields, dspffd};
pub use pfm::{RecordView, dsppfm, render_char, render_hex};

use std::fmt::Write as _;

use store::obj::{ObjectId, ops};
use store::txn::Txn;

use crate::file::Wait;
use crate::record::datetime::{Date, Time};

/// `label . . . :  value`, the label padded to 34 columns.
pub(crate) fn kv(out: &mut String, label: &str, value: impl std::fmt::Display) {
    let mut l = String::from(label);
    let mut dot = false;
    while l.len() < 33 {
        l.push(if dot { '.' } else { ' ' });
        dot = !dot;
    }
    if l.ends_with('.') {
        l.push(' ');
    }
    let _ = writeln!(out, "{l}:  {value}");
}

/// A Unix time as `yyyy-mm-dd hh:mm:ss`, or `*NONE` for zero.
#[must_use]
pub fn timestamp(secs: u64) -> String {
    if secs == 0 {
        return "*NONE".into();
    }
    let d = Date::from_unix(secs);
    let t = Time::from_unix(secs);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        d.year, d.month, d.day, t.hour, t.minute, t.second
    )
}

/// `*YES` or `*NO`.
pub(crate) fn yes_no(b: bool) -> &'static str {
    if b { "*YES" } else { "*NO" }
}

/// A wait as the parameter spells it.
pub(crate) fn wait_text(w: Wait, file: bool) -> String {
    match w {
        Wait::Immed => "*IMMED".into(),
        Wait::NoMax if file => "*CLS".into(),
        Wait::NoMax => "*NOMAX".into(),
        Wait::Seconds(s) => s.to_string(),
    }
}

/// The library name of an object, or its ID when the header is gone.
pub(crate) fn library_name(txn: &mut Txn<'_>, id: ObjectId) -> String {
    ops::read_header(txn, id).map_or_else(|_| id.to_string(), |(_, h)| h.name.to_string())
}
