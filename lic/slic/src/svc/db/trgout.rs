// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DSPFD TYPE(*TRG) OUTPUT(*OUTFILE)`: one `QWHFDTRG` record per trigger,
//! IBM's `QAFDTRG` format field for field as the primary's `DSPFFD` shows
//! it, filled as its records are
//! (`tools/conformance/sql/oracle/trigger/primary-facts.md`, `p1.out`,
//! `p8.out`).

use std::fmt::Write as _;

use db::trg::model::{Event, MtAction, Threadsafe, Time, Trigger, UpdateCondition};

use super::{Params, db, db_err, file_id, with_txn};
use crate::cpp::savout::{Fld, Row, stamp, write_to};
use crate::ipl::system::{System, now_unix};

/// `QWHFDTRG`, 32 fields, 1378 bytes.
const QWHFDTRG: &[Fld] = &[
    ("TRRCEN", 'A', 1, 0),
    ("TRRDAT", 'A', 6, 0),
    ("TRRTIM", 'A', 6, 0),
    ("TRFILE", 'A', 10, 0),
    ("TRLIB", 'A', 10, 0),
    ("TRFTYP", 'A', 1, 0),
    ("TRFILA", 'A', 4, 0),
    ("TRMXD", 'A', 3, 0),
    ("TRFATR", 'A', 6, 0),
    ("TRSYSN", 'A', 8, 0),
    ("TRASP", 'P', 3, 0),
    ("TRRES", 'A', 4, 0),
    ("TRTRTM", 'A', 1, 0),
    ("TRTREV", 'A', 1, 0),
    ("TRTRCN", 'A', 1, 0),
    ("TRTRIG", 'A', 10, 0),
    ("TRTRLB", 'A', 10, 0),
    ("TRTARC", 'A', 1, 0),
    ("TRTTSF", 'A', 1, 0),
    ("TRTMJA", 'A', 1, 0),
    ("TRNAM", 'A', 258, 0),
    ("TRLB", 'A', 10, 0),
    ("TRST", 'A', 1, 0),
    ("TRSTAT", 'A', 1, 0),
    ("TRTYP", 'A', 1, 0),
    ("TRMOD", 'A', 1, 0),
    ("TRORNT", 'A', 1, 0),
    ("TRCEN", 'A', 1, 0),
    ("TRDATE", 'A', 6, 0),
    ("TRTIME", 'A', 6, 0),
    ("TRUCN", 'P', 5, 0),
    ("TRUC", 'V', 1000, 0),
];

fn flag(b: bool, yes: &str, no: &str) -> String {
    (if b { yes } else { no }).to_string()
}

/// The order the primary lists triggers in: by event, then time.
fn rank(t: &Trigger) -> (u8, u8, u32) {
    let e = match t.event {
        Event::Insert => 0,
        Event::Update => 1,
        Event::Delete => 2,
        Event::Read => 3,
    };
    let w = match t.time {
        Time::Before => 0,
        Time::After => 1,
        Time::InsteadOf => 2,
    };
    (e, w, t.number)
}

/// The record of trigger `t` of file `file` in `lib`.
fn row(t: &Trigger, (file, lib, attr): (&str, &str, &str), system: &str, now: &str) -> Row {
    let name = |n: qsname::Name| n.to_string().trim_end().to_string();
    let made = stamp(t.created);
    vec![
        ("TRRCEN", now[0..1].to_string()),
        ("TRRDAT", now[1..7].to_string()),
        ("TRRTIM", now[7..13].to_string()),
        ("TRFILE", file.to_string()),
        ("TRLIB", lib.to_string()),
        ("TRFTYP", "P".into()),
        ("TRFILA", "*PHY".into()),
        ("TRFATR", attr.to_string()),
        ("TRSYSN", system.to_string()),
        ("TRASP", "1".into()),
        (
            "TRTRTM",
            match t.time {
                Time::Before => "B",
                Time::After => "A",
                Time::InsteadOf => "I",
            }
            .into(),
        ),
        (
            "TRTREV",
            match t.event {
                Event::Insert => "I",
                Event::Update => "U",
                Event::Delete => "D",
                Event::Read => "R",
            }
            .into(),
        ),
        (
            "TRTRCN",
            match t.update_condition {
                UpdateCondition::None => " ",
                UpdateCondition::Always => "A",
                UpdateCondition::Change => "C",
            }
            .into(),
        ),
        ("TRTRIG", name(t.program.1)),
        ("TRTRLB", name(t.program.0)),
        ("TRTARC", flag(t.sql || t.alwrepchg, "Y", "N")),
        (
            "TRTTSF",
            if t.sql {
                "Y"
            } else {
                match t.threadsafe {
                    Threadsafe::Unknown => "U",
                    Threadsafe::Yes => "Y",
                    Threadsafe::No => "N",
                }
            }
            .into(),
        ),
        (
            "TRTMJA",
            if t.sql {
                "R"
            } else {
                match t.mt_action {
                    MtAction::Sysval => "S",
                    MtAction::Msg => "M",
                    MtAction::NoRun => "N",
                    MtAction::Run => "R",
                }
            }
            .into(),
        ),
        ("TRNAM", t.name.clone()),
        ("TRLB", name(t.library)),
        ("TRST", flag(t.enabled, "E", "D")),
        ("TRSTAT", flag(t.operative, "O", "I")),
        ("TRTYP", flag(t.sql, "S", "Y")),
        (
            "TRMOD",
            if t.sql {
                flag(t.db2sql, "S", "R")
            } else {
                " ".into()
            },
        ),
        ("TRORNT", flag(t.statement, "S", "R")),
        ("TRCEN", made[0..1].to_string()),
        ("TRDATE", made[1..7].to_string()),
        ("TRTIME", made[7..13].to_string()),
        ("TRUCN", t.columns.len().to_string()),
        (
            "TRUC",
            t.columns.iter().fold(String::new(), |mut s, c| {
                let _ = write!(s, "{:<10}", c.to_string().trim_end());
                s
            }),
        ),
    ]
}

/// `DSPFD LIB/FILE TYPE(*TRG) OUTPUT(*OUTFILE) OUTFILE(LIB/NAME)
/// [OUTMBR(*FIRST *REPLACE|*ADD)]`.
pub fn outfile(system: &mut System, p: &Params) -> Result<String, String> {
    if p.upper("TYPE").as_deref() != Some("*TRG") {
        // BLOCKED(rel): the other DSPFD outfile formats (QAFDBASI, QAFDMBR,
        // QAFDACCP, QAFDCSTD, ...) are not built.
        return Err("CPD0084 OUTPUT(*OUTFILE) is supported with TYPE(*TRG) only.".into());
    }
    let qual = p.pos_or(0, "FILE")?;
    let target = p
        .kw("OUTFILE")
        .ok_or("CPD0071 OUTFILE is required with OUTPUT(*OUTFILE)")?
        .to_ascii_uppercase();
    let (lib, name) = target
        .split_once('/')
        .map_or(("*LIBL", target.as_str()), |(l, n)| (l, n));
    let replace = p
        .upper("OUTMBR")
        .is_none_or(|m| m.split_whitespace().nth(1) != Some("*ADD"));
    let sysname = system.system_name();
    let now = stamp(now_unix());
    let rows = {
        let (store, ctx) = db(system)?;
        with_txn(store, |txn| {
            let id = file_id(txn, ctx, &qual)?;
            let d = db::file::describe(txn, id).map_err(|e| db_err(&e))?;
            let mut trgs = db::trg::list(txn, id).map_err(|e| db_err(&e))?;
            trgs.sort_by_key(rank);
            let file = d.name().to_string().trim_end().to_string();
            let flib = d.library_name.to_string().trim_end().to_string();
            let attr = ccsid::decode_trimmed(&d.header.attribute);
            Ok(trgs
                .iter()
                .map(|t| row(t, (&file, &flib, &attr), &sysname, &now))
                .collect::<Vec<_>>())
        })?
    };
    write_to(system, (name, lib, replace), "QWHFDTRG", QWHFDTRG, &rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_format_is_the_primarys() {
        // `DSPFFD QSYS/QAFDTRG` on the primary: 32 fields, TRUC at 377.
        let f = crate::cpp::savout::model("QWHFDTRG", QWHFDTRG);
        assert_eq!(f.fields.len(), 32);
        let at = |n: &str| {
            f.fields
                .iter()
                .find(|x| x.name.to_string().trim_end() == n)
                .map(|x| x.offset + 1)
                .unwrap()
        };
        assert_eq!(at("TRTRTM"), 62);
        assert_eq!(at("TRNAM"), 88);
        assert_eq!(at("TRLB"), 346);
        assert_eq!(at("TRUCN"), 374);
        assert_eq!(at("TRUC"), 377);
        assert_eq!(f.record_len, 1378);
    }
}
