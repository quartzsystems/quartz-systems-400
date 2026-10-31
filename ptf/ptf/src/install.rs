// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `INSPTF`: load every PTF of a package (a cumulative package, a group,
//! what `SNDPTFORD` delivered) and apply them in requisite order, now or
//! at the next IPL as `INSTYPE` says.

use std::collections::{BTreeMap, BTreeSet};

use crate::apply::{self, Ctx};
use crate::db::PtfDb;
use crate::package::{self, Package};
use crate::status::{Action, Status};
use crate::{Error, Result};

/// `INSTYPE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsType {
    /// `*DLYALL`: every PTF at the next IPL.
    DelayAll,
    /// `*DLYIPL`: every PTF at the next IPL, and the IPL now.
    DelayIpl,
    /// `*IMMDLY`: immediate PTFs now, delayed ones at the next IPL.
    ImmediateDelayed,
    /// `*IMMONLY`: immediate PTFs now; delayed ones stay loaded.
    ImmediateOnly,
}

impl InsType {
    /// From the parameter value.
    #[must_use]
    pub fn parse(v: &str) -> Option<Self> {
        Some(match v {
            "*DLYALL" => Self::DelayAll,
            "*DLYIPL" => Self::DelayIpl,
            "*IMMDLY" => Self::ImmediateDelayed,
            "*IMMONLY" => Self::ImmediateOnly,
            _ => return None,
        })
    }
}

/// One stream to install: the save file it is in and its bytes.
pub struct Item {
    /// `lib/name`.
    pub savf: String,
    /// The stream.
    pub bytes: Vec<u8>,
}

/// What `INSPTF` did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// Loaded now.
    pub loaded: Vec<String>,
    /// Temporarily applied now.
    pub applied: Vec<String>,
    /// Marked Apply at next IPL.
    pub at_ipl: Vec<String>,
    /// LIC PTFs among `at_ipl`: their images go to the other slot.
    pub lic: Vec<String>,
    /// Left loaded (`*IMMONLY` delayed PTFs).
    pub left: Vec<String>,
    /// Refused or failed, with the message.
    pub failed: Vec<(String, &'static str, String)>,
}

impl Report {
    /// Whether an IPL is needed for the PTFs to take effect.
    #[must_use]
    pub fn ipl_needed(&self) -> bool {
        !self.at_ipl.is_empty()
    }
}

/// Requisite order over `ids`: prerequisites and co-requisites in the
/// set come first, ties by ID.
#[must_use]
pub fn order(db: &PtfDb, ids: &BTreeSet<String>) -> Vec<String> {
    fn visit(db: &PtfDb, id: &str, ids: &BTreeSet<String>, state: &mut BTreeMap<String, bool>, out: &mut Vec<String>) {
        if state.contains_key(id) {
            return;
        }
        state.insert(id.to_string(), false);
        if let Some(r) = db.get(id) {
            let mut req: Vec<&str> = r
                .prereqs
                .iter()
                .map(|p| p.ptf.as_str())
                .filter(|p| ids.contains(*p))
                .collect();
            req.sort_unstable();
            for p in req {
                visit(db, p, ids, state, out);
            }
        }
        state.insert(id.to_string(), true);
        out.push(id.to_string());
    }
    let mut state = BTreeMap::new();
    let mut out = Vec::new();
    for id in ids {
        visit(db, id, ids, &mut state, &mut out);
    }
    out
}

/// Install `items`: open each (every signature checked against
/// `trusted`), load what is not loaded, then apply as `ins` says. PTFs
/// in `omit` are left alone. One PTF failing does not stop the rest.
pub fn install(
    cx: &mut Ctx<'_>,
    db: &mut PtfDb,
    items: &[Item],
    trusted: &[[u8; 32]],
    release: &str,
    ins: InsType,
    omit: &[String],
) -> Result<Report> {
    let mut rep = Report::default();
    let mut pkgs: BTreeMap<String, (Package, usize)> = BTreeMap::new();
    for (i, it) in items.iter().enumerate() {
        let pkg = match package::open(&it.bytes, trusted) {
            Ok(p) => p,
            Err(e) => {
                rep.failed.push((it.savf.clone(), e.msgid(), e.to_string()));
                continue;
            }
        };
        let id = pkg.header.ptf.clone();
        if omit.contains(&id) {
            continue;
        }
        let loaded = db
            .get(&id)
            .is_some_and(|r| !matches!(r.status, Status::NotLoaded | Status::OnOrder | Status::PermRemoved | Status::SaveFileOnly | Status::CoverLetterOnly));
        if !loaded {
            match apply::load(db, &pkg, &it.savf, release, cx.now) {
                Ok(()) => rep.loaded.push(id.clone()),
                Err(e) => {
                    rep.failed.push((id, e.msgid(), e.to_string()));
                    continue;
                }
            }
        }
        pkgs.insert(id, (pkg, i));
    }
    // What can still be applied: loaded or removed, not superseded.
    let candidates: BTreeSet<String> = pkgs
        .keys()
        .filter(|p| {
            db.get(p)
                .is_some_and(|r| matches!(r.status, Status::Loaded | Status::TempRemoved) && r.action == Action::None)
        })
        .cloned()
        .collect();
    let later = |db: &PtfDb, p: &str| db.get(p).is_some_and(|r| r.delayed || r.lic);
    let now_set: BTreeSet<String> = match ins {
        InsType::DelayAll | InsType::DelayIpl => BTreeSet::new(),
        InsType::ImmediateDelayed | InsType::ImmediateOnly => {
            candidates.iter().filter(|p| !later(db, p)).cloned().collect()
        }
    };
    let batch: Vec<String> = if ins == InsType::ImmediateOnly {
        now_set.iter().cloned().collect()
    } else {
        candidates.iter().cloned().collect()
    };
    for p in order(db, &candidates) {
        if ins == InsType::ImmediateOnly && !now_set.contains(&p) {
            rep.left.push(p);
            continue;
        }
        let r = if now_set.contains(&p) {
            let (pkg, i) = &pkgs[&p];
            apply::apply_temp(cx, db, pkg, &items[*i].bytes, &batch, false).map(|_| ())
        } else {
            apply::check_apply(db, &p, &batch).map(|()| {
                let rec = db.ptfs.get_mut(&p).expect("checked");
                rec.action = Action::ApplyAtIpl;
                rec.changed = cx.now;
            })
        };
        match r {
            Ok(()) => {
                let rec = db.get(&p).expect("loaded");
                if rec.action == Action::ApplyAtIpl {
                    if rec.lic {
                        rep.lic.push(p.clone());
                    }
                    rep.at_ipl.push(p);
                } else {
                    rep.applied.push(p);
                }
            }
            Err(e) => rep.failed.push((p, e.msgid(), e.to_string())),
        }
    }
    if rep.loaded.is_empty() && rep.applied.is_empty() && rep.at_ipl.is_empty() && !rep.failed.is_empty() {
        let (p, id, text) = &rep.failed[0];
        return Err(Error::refused(id, format!("{p}: {text}")));
    }
    Ok(rep)
}
