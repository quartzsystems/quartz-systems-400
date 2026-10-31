// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `SNDPTFORD`'s work over a client: the order, each PTF fetched and
//! checked against the signed catalog, handed to a sink (the save files
//! in `QGPL`), and recorded in the PTF database; and `QSCHKUPD`'s
//! comparison of a catalog with what the system has.

use ptf::grpdb::GroupDb;
use ptf::status::{Action, Status};
use ptf::{PtfDb, PtfRecord, PtfRef};

use crate::catalog::{Catalog, CatalogPtf};
use crate::client::Client;
use crate::order::{OrderRequest, OrderResult};
use crate::{Error, Result};

/// Where delivered PTFs go.
pub trait Sink {
    /// Keep a PTF's stream; the save file it went to, `lib/name`.
    fn put(&mut self, entry: &CatalogPtf, bytes: &[u8]) -> Result<String>;
}

/// What an order delivered.
#[derive(Debug, Clone)]
pub struct Delivery {
    /// The catalog it was checked against.
    pub catalog: Catalog,
    /// The service's answer.
    pub order: OrderResult,
    /// Each PTF and its save file.
    pub delivered: Vec<(String, String)>,
    /// Each group's signed manifest.
    pub manifests: Vec<(String, String)>,
}

/// The PTFs a system has in a state that counts for `REORDER(*NO)`.
#[must_use]
pub fn have(db: &PtfDb) -> Vec<String> {
    db.ptfs
        .values()
        .filter(|r| {
            matches!(
                r.status,
                Status::Loaded | Status::TempApplied | Status::PermApplied | Status::Superseded | Status::SaveFileOnly
            )
        })
        .map(|r| r.ptf.clone())
        .collect()
}

/// Place `req` and deliver it into `sink`. Every PTF the service names
/// must be in the catalog, and every stream must match its entry.
pub fn send_order(
    c: &Client,
    trusted: &[[u8; 32]],
    min_sequence: u64,
    req: &OrderRequest,
    sink: &mut dyn Sink,
) -> Result<Delivery> {
    let catalog = c.catalog(trusted, min_sequence)?;
    let order = c.order(req)?;
    for (p, _) in &order.ptfs {
        if catalog.ptf(p).is_none() {
            return Err(Error::Verify(format!("{p} is not in the catalog")));
        }
    }
    let mut delivered = Vec::new();
    for (p, _) in &order.ptfs {
        let entry = catalog.ptf(p).expect("checked");
        let bytes = c.ptf(entry)?;
        delivered.push((p.clone(), sink.put(entry, &bytes)?));
    }
    let mut manifests = Vec::new();
    for (g, _) in &order.groups {
        manifests.push((g.clone(), c.group(g, trusted)?));
    }
    Ok(Delivery {
        catalog,
        order,
        delivered,
        manifests,
    })
}

/// Record a delivered PTF: Save file only (Cover letter only for
/// `PTFPART(*CVRLTR)`), unless the system has it in a later state.
pub fn record(db: &mut PtfDb, e: &CatalogPtf, savf: &str, cover_only: bool, now: u64) {
    if let Some(r) = db.ptfs.get_mut(&e.id) {
        if matches!(r.status, Status::NotLoaded | Status::OnOrder | Status::PermRemoved | Status::CoverLetterOnly) {
            r.status = if cover_only { Status::CoverLetterOnly } else { Status::SaveFileOnly };
            r.savf = savf.into();
            r.changed = now;
        }
        return;
    }
    let refs = |v: &[String]| {
        v.iter()
            .map(|p| PtfRef {
                ptf: p.clone(),
                product: e.product.clone(),
            })
            .collect()
    };
    db.ptfs.insert(
        e.id.clone(),
        PtfRecord {
            ptf: e.id.clone(),
            product: e.product.clone(),
            option: e.option.clone(),
            release: e.release.clone(),
            status: if cover_only { Status::CoverLetterOnly } else { Status::SaveFileOnly },
            action: Action::None,
            delayed: e.kind != crate::catalog::PtfKind::Immediate,
            lic: e.kind == crate::catalog::PtfKind::Lic,
            savf: savf.into(),
            superseded_by: String::new(),
            prereqs: refs(&e.prereqs),
            coreqs: refs(&e.coreqs),
            replaced: Vec::new(),
            changed: now,
        },
    );
}

/// What `QSCHKUPD` found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Findings {
    /// Groups the system has at a lower level: name, its level, the newest.
    pub newer: Vec<(String, u32, u32)>,
    /// HIPER PTFs of the release the system does not have.
    pub hiper: Vec<String>,
    /// A cumulative package newer than any the system has.
    pub cumulative: Option<String>,
}

impl Findings {
    /// Whether anything is worth a message.
    #[must_use]
    pub fn any(&self) -> bool {
        !self.newer.is_empty() || !self.hiper.is_empty() || self.cumulative.is_some()
    }
}

/// Compare `cat` with the system and note the newest levels in `groups`.
pub fn compare(cat: &Catalog, db: &PtfDb, groups: &mut GroupDb, release: &str, trusted: &[[u8; 32]]) -> Findings {
    let mut f = Findings::default();
    let mut known_cum: Vec<release::Cumulative> = Vec::new();
    let names: Vec<String> = groups.groups.keys().cloned().collect();
    for name in names {
        let level = groups.groups[&name].group(trusted).map_or(0, |(g, _)| g.level);
        if let Ok(c) = name.parse::<release::Cumulative>() {
            known_cum.push(c);
        }
        if let Some(g) = cat.group(&name) {
            groups.offer(&name, g.level);
            if g.level > level {
                f.newer.push((name, level, g.level));
            }
        }
    }
    let has = |p: &str| {
        db.get(p)
            .is_some_and(|r| !matches!(r.status, Status::NotLoaded | Status::OnOrder | Status::PermRemoved))
    };
    let mut hiper: Vec<String> = cat
        .ptfs
        .iter()
        .filter(|p| p.release == release && p.hiper)
        .map(|p| p.id.clone())
        .chain(cat.hiper(release).into_iter().flat_map(|g| g.ptfs.iter().cloned()))
        .filter(|p| !has(p))
        .collect();
    hiper.sort();
    hiper.dedup();
    f.hiper = hiper;
    if let Some(c) = cat.cumulative(release)
        && let Ok(cc) = c.name.parse::<release::Cumulative>()
        && known_cum.iter().all(|k| (k.year, k.day) < (cc.year, cc.day))
    {
        f.cumulative = Some(c.name.clone());
    }
    f
}
