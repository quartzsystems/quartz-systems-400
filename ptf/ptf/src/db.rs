// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The system's PTF database: every PTF it knows, its status, the save
//! file it was loaded from and the objects an apply moved aside. The
//! integrator keeps the encoded text (a user space in `QSYS`).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::header::PtfRef;
use crate::status::{Action, Status};

/// One object an apply replaced: where it was, and its name in
/// `QRPLOBJ` while the PTF is temporarily applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replaced {
    /// Library.
    pub library: String,
    /// Name.
    pub name: String,
    /// Type code.
    pub type_code: u16,
    /// Its name in `QRPLOBJ`; empty when the PTF added the object.
    pub saved_as: String,
}

/// One PTF.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PtfRecord {
    /// `QSnnnnn`.
    pub ptf: String,
    /// Product ID.
    pub product: String,
    /// Option.
    pub option: String,
    /// Release.
    pub release: String,
    /// Status.
    pub status: Status,
    /// What the next IPL does.
    pub action: Action,
    /// Delayed: applied at an IPL.
    pub delayed: bool,
    /// A LIC replacement.
    pub lic: bool,
    /// The save file it was loaded into, `lib/name`.
    pub savf: String,
    /// The PTF that supersedes it, when one does.
    pub superseded_by: String,
    /// Prerequisites.
    pub prereqs: Vec<PtfRef>,
    /// Co-requisites.
    pub coreqs: Vec<PtfRef>,
    /// The objects an apply replaced.
    pub replaced: Vec<Replaced>,
    /// Unix seconds of the last status change.
    pub changed: u64,
}

/// Every PTF, by ID.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PtfDb {
    /// The PTFs.
    pub ptfs: BTreeMap<String, PtfRecord>,
}

fn refs(v: &[PtfRef]) -> String {
    v.iter()
        .map(|r| format!("{}:{}", r.ptf, r.product))
        .collect::<Vec<_>>()
        .join(",")
}

fn parse_refs(s: &str) -> Vec<PtfRef> {
    s.split(',')
        .filter(|x| !x.is_empty())
        .filter_map(|x| {
            let (p, prod) = x.split_once(':')?;
            Some(PtfRef {
                ptf: p.into(),
                product: prod.into(),
            })
        })
        .collect()
}

impl PtfDb {
    /// The text the integrator stores: one line per PTF, fields
    /// separated by tabs.
    #[must_use]
    pub fn encode(&self) -> String {
        let mut out = String::from("QS400PTFDB 1\n");
        for r in self.ptfs.values() {
            let replaced = r
                .replaced
                .iter()
                .map(|x| format!("{}/{}/{:04X}/{}", x.library, x.name, x.type_code, x.saved_as))
                .collect::<Vec<_>>()
                .join(",");
            let action = match r.action {
                Action::None => "-",
                Action::ApplyAtIpl => "APPLY",
                Action::RemoveAtIpl => "REMOVE",
            };
            let _ = writeln!(
                out,
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                r.ptf,
                r.product,
                r.option,
                r.release,
                r.status.code(),
                action,
                u8::from(r.delayed),
                u8::from(r.lic),
                r.savf,
                r.superseded_by,
                refs(&r.prereqs),
                refs(&r.coreqs),
                replaced,
                r.changed
            );
        }
        out
    }

    /// Parse the stored text; lines that do not parse are dropped.
    #[must_use]
    pub fn decode(text: &str) -> Self {
        let mut db = Self::default();
        for line in text.lines().skip(1) {
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() != 14 {
                continue;
            }
            let Some(status) = Status::from_code(f[4]) else {
                continue;
            };
            let replaced = f[12]
                .split(',')
                .filter(|x| !x.is_empty())
                .filter_map(|x| {
                    let p: Vec<&str> = x.splitn(4, '/').collect();
                    Some(Replaced {
                        library: (*p.first()?).into(),
                        name: (*p.get(1)?).into(),
                        type_code: u16::from_str_radix(p.get(2)?, 16).ok()?,
                        saved_as: (*p.get(3)?).into(),
                    })
                })
                .collect();
            let r = PtfRecord {
                ptf: f[0].into(),
                product: f[1].into(),
                option: f[2].into(),
                release: f[3].into(),
                status,
                action: match f[5] {
                    "APPLY" => Action::ApplyAtIpl,
                    "REMOVE" => Action::RemoveAtIpl,
                    _ => Action::None,
                },
                delayed: f[6] == "1",
                lic: f[7] == "1",
                savf: f[8].into(),
                superseded_by: f[9].into(),
                prereqs: parse_refs(f[10]),
                coreqs: parse_refs(f[11]),
                replaced,
                changed: f[13].parse().unwrap_or(0),
            };
            db.ptfs.insert(r.ptf.clone(), r);
        }
        db
    }

    /// A PTF's record.
    #[must_use]
    pub fn get(&self, ptf: &str) -> Option<&PtfRecord> {
        self.ptfs.get(ptf)
    }

    /// The PTFs of a product, in ID order.
    pub fn of_product<'a>(&'a self, product: &'a str) -> impl Iterator<Item = &'a PtfRecord> + 'a {
        self.ptfs.values().filter(move |r| r.product == product)
    }
}
