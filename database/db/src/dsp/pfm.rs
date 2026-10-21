// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DSPPFM`: a member's records as characters or hexadecimal.

use std::fmt::Write as _;

use qsname::MemberSpec;
use store::obj::ObjectId;
use store::txn::Txn;

use crate::error::Result;
use crate::member::Member;
use crate::util::member_of;

/// One record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordView {
    /// Relative record number.
    pub rrn: u64,
    /// Whether the slot is deleted.
    pub deleted: bool,
    /// The record bytes.
    pub bytes: Vec<u8>,
}

impl RecordView {
    /// The bytes as text, CP037 decoded; unprintable bytes show as `.`.
    #[must_use]
    pub fn text(&self) -> String {
        self.bytes
            .iter()
            .map(|&b| {
                let c = ccsid::char_of(b);
                if c.is_control() || b < 0x40 { '.' } else { c }
            })
            .collect()
    }
}

/// Records from `from_rrn`, at most `max`; deleted slots too when
/// `with_deleted`.
pub fn dsppfm(
    txn: &mut Txn<'_>,
    file: ObjectId,
    member: MemberSpec,
    from_rrn: u64,
    max: usize,
    with_deleted: bool,
) -> Result<Vec<RecordView>> {
    let desc = crate::file::describe(txn, file)?;
    let entry = member_of(&desc, member)?;
    let m = Member::open(txn, &desc, &entry)?;
    let mut out = Vec::new();
    if m.is_logical() {
        // Logical records in arrival sequence of the first base.
        let order = crate::util::Order {
            from_rrn,
            ..crate::util::Order::default()
        };
        let mut m = m;
        crate::util::each_record(txn, &mut m, &order, &mut |_, s| {
            out.push(RecordView {
                rrn: s.rrn,
                deleted: false,
                bytes: s.record,
            });
            Ok(out.len() < max)
        })?;
        return Ok(out);
    }
    let mut rrn = from_rrn.max(1);
    while rrn <= m.attr().slots && out.len() < max {
        if let Some(s) = m.read(txn, rrn)?
            && (with_deleted || !s.deleted)
        {
            out.push(RecordView {
                rrn: s.rrn,
                deleted: s.deleted,
                bytes: s.record,
            });
        }
        rrn += 1;
    }
    Ok(out)
}

/// The character view: RRN, a deleted mark, the text.
#[must_use]
pub fn render_char(views: &[RecordView]) -> String {
    let mut out = String::new();
    for v in views {
        let _ = writeln!(
            out,
            "{:>8} {}{}",
            v.rrn,
            if v.deleted { "D " } else { "  " },
            v.text()
        );
    }
    out
}

/// The hexadecimal view: per record, lines of 16 bytes with the offset,
/// the hex and the characters.
#[must_use]
pub fn render_hex(views: &[RecordView]) -> String {
    let mut out = String::new();
    for v in views {
        let _ = writeln!(
            out,
            "Record {}{}",
            v.rrn,
            if v.deleted { " (deleted)" } else { "" }
        );
        let text = v.text();
        let chars: Vec<char> = text.chars().collect();
        for (i, chunk) in v.bytes.chunks(16).enumerate() {
            let mut hex = String::new();
            for (j, b) in chunk.iter().enumerate() {
                if j > 0 && j % 4 == 0 {
                    hex.push(' ');
                }
                let _ = write!(hex, "{b:02X}");
            }
            let at = i * 16;
            let shown: String = chars[at..at + chunk.len()].iter().collect();
            let _ = writeln!(out, "  {at:>6}  {hex:<35}  *{shown}*");
        }
    }
    out
}
