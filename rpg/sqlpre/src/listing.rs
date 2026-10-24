// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The precompile listing: the options, the source records, and the
//! diagnostics in the section and layout IBM's listing uses
//! (`MSG ID  SEV  RECORD  TEXT`), so one reader handles both. Each
//! record starts with its forms control character.

use crate::{Precompiled, msgs};

/// What the listing's heading names.
#[derive(Clone, Debug, Default)]
pub struct Heading {
    /// `LIB/OBJ`.
    pub object: String,
    /// `LIB/FILE`.
    pub source_file: String,
    /// The member.
    pub member: String,
    /// `*PGM`, `*MODULE` or `*SRVPGM`.
    pub objtype: String,
    /// The release, `V7R5M0`.
    pub release: String,
}

/// The listing; `records` are the source lines with their sequence
/// numbers.
#[must_use]
pub fn listing(p: &Precompiled, h: &Heading, records: &[(String, String)]) -> Vec<String> {
    let o = &p.options;
    let mut out = vec![format!("1QS/400 SQL precompiler {}  {}", h.release, h.object)];
    let rows = [
        ("Source type", "RPG".to_string()),
        ("Object name", h.object.clone()),
        ("Source file", h.source_file.clone()),
        ("Member", h.member.clone()),
        ("Commit", o.commit.clone()),
        ("Naming", o.naming.clone()),
        ("Close SQL cursor", o.closqlcsr.clone()),
        ("Generation level", o.genlvl.to_string()),
        ("Date format", o.datfmt.clone()),
        ("Date separator", o.datsep.clone()),
        ("Time format", o.timfmt.clone()),
        ("Time separator", o.timsep.clone()),
        ("Created object type", h.objtype.clone()),
        ("Target release", h.release.clone()),
    ];
    for (i, (k, v)) in rows.iter().enumerate() {
        let cc = if i == 0 { '-' } else { ' ' };
        out.push(format!("{cc}{k:.<26}{v}"));
    }
    out.push("1Record  *...+... 1 ...+... 2 ...+... 3 ...+... 4 ...+... 5 ...+... 6 ...+... 7 ...+... 8   SEQNBR".into());
    for (i, (text, seq)) in records.iter().enumerate() {
        let cc = if i == 0 { '0' } else { ' ' };
        out.push(format!("{cc}{:>5}   {text:<100}  {seq}", i + 1));
    }
    out.push("0                                * * * * *  E N D  O F  S O U R C E  * * * * *".into());
    if !p.diags.is_empty() {
        out.push("1                                             DIAGNOSTIC MESSAGES".into());
        out.push("0MSG ID  SEV  RECORD  TEXT".into());
        for d in &p.diags {
            out.push(format!(" {}  {:>2}  {:>6}  {}", d.msgid, d.sev, d.line, msgs::text(d)));
        }
    }
    let count = |lo: u8, hi: u8| p.diags.iter().filter(|d| d.sev >= lo && d.sev <= hi).count();
    out.push("-                    Message Summary".into());
    out.push("   Total     Info   Warning      Error   Severe  Terminal".into());
    out.push(format!(
        " {:>7}  {:>7}   {:>7}    {:>7}  {:>7}   {:>7}",
        p.diags.len(),
        count(0, 9),
        count(10, 19),
        count(20, 29),
        count(30, 39),
        count(40, 99)
    ));
    out.push(format!("-{} level severity errors found in source", p.max_severity));
    out.push(format!("0{:>5} Source records processed", records.len()));
    out.push("                                * * * * *  E N D  O F  L I S T I N G  * * * * *".into());
    out
}
