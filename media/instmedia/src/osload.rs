// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Installing a product load from install media into a store: the
//! operating system (`QS00SS1`) at install time, the licensed programs
//! after it. Through the RSTLICPGM planning (`licpgm::plan`) and the
//! save engine (`lpstore::SaveRestorer`), so a fresh install, a slip
//! install and an upgrade are the same restore:
//!
//! - every option of the product whose code is on the media (or the
//!   ones asked for), `*BASE` first;
//! - over an installed load the product is replaced object by object:
//!   each object of the new load restored over the old one, objects
//!   only the old load listed deleted, nothing else touched;
//! - every object's signature checked against the system key as it is
//!   restored (`verified`);
//! - after an upgrade, programs in user libraries translated again
//!   from their IR (the caller's `retranslate`).

use std::cell::{Cell as StdCell, RefCell};
use std::collections::BTreeSet;

use licpgm::model::load::ObjEntry;
use licpgm::plan::restore::{RstLicPgm, execute_restore, plan_restore};
use licpgm::plan::{Media, NoExits, Objects, RestoreError, Restorer};
use licpgm::{Date, LoadKey, LoadType, OptionNo, ProductId, ProductStore, SystemInfo};
use lpstore::{Products, SaveRestorer, SavfMedia, stamp};
use qsname::Name;
use release::InstallKind;
use saverst::Hooks;
use store::Store;
use store::obj::{JobHandle, ObjectId, ops, types};
use store::txn::Txn;

use crate::{Error, Result};

/// Verifies object signatures against the system key while restoring.
struct InstallHooks {
    key: Option<[u8; 32]>,
    verified: StdCell<usize>,
    bad: StdCell<usize>,
}

impl Hooks for InstallHooks {
    fn verify(&self, key_id: [u8; 8], digest: &[u8; 32], sig: &[u8; 64]) -> Option<bool> {
        let k = self.key?;
        if crate::sign::key_id(&k) != key_id {
            return None;
        }
        let ok = saverst::sign::verify(&k, digest, sig);
        if ok {
            self.verified.set(self.verified.get() + 1);
        } else {
            self.bad.set(self.bad.get() + 1);
        }
        Some(ok)
    }

    fn system_key(&self, key_id: [u8; 8]) -> bool {
        self.key.is_some_and(|k| crate::sign::key_id(&k) == key_id)
    }
}

/// Restores each library of the stream once: every load of a product
/// names the same stream, which holds the objects of all of them.
struct Once<R> {
    inner: R,
    done: Vec<(String, Vec<ObjEntry>)>,
}

impl<R: Restorer> Restorer for Once<R> {
    fn restore(&mut self, stream: &[u8], library: &str) -> std::result::Result<Vec<ObjEntry>, RestoreError> {
        if let Some((_, o)) = self.done.iter().find(|(l, _)| l == library) {
            return Ok(o.clone());
        }
        let o = self.inner.restore(stream, library)?;
        self.done.push((library.to_string(), o.clone()));
        Ok(o)
    }

    fn restore_directory(
        &mut self,
        stream: &[u8],
        path: &str,
    ) -> std::result::Result<Vec<String>, RestoreError> {
        self.inner.restore_directory(stream, path)
    }
}

/// What installing one product did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// The loads installed, in order.
    pub loads: Vec<LoadKey>,
    /// Objects restored.
    pub objects: usize,
    /// Of those, over an object of an installed load.
    pub replaced: usize,
    /// Objects only the old load had, deleted.
    pub removed: usize,
    /// Signatures checked good against the system key.
    pub verified: usize,
    /// Signatures that did not check out (the object was refused).
    pub bad_signatures: usize,
    /// Modules translated again (after an upgrade).
    pub retranslated: u32,
}

/// One install.
pub struct Request<'a> {
    /// The product's SAVLICPGM stream from the media.
    pub stream: Vec<u8>,
    /// The product.
    pub product: &'a str,
    /// Options to install; empty for every option with code on the media.
    pub options: Vec<OptionNo>,
    /// The system key object signatures are checked with.
    pub key: Option<[u8; 32]>,
    /// The system, for the planning.
    pub system: SystemInfo,
    /// Who accepts the software agreements (the install is the
    /// operator's acceptance; docs/INSTALL.md).
    pub accepted_by: &'a str,
    /// Seconds since the epoch.
    pub now: u64,
}

fn key_name(k: &LoadKey) -> String {
    format!("{} {} {}", k.product, k.option, k.release)
}

/// Install `req.product` from its stream into `store`.
pub fn install(store: &mut Store, qsys: ObjectId, req: &Request<'_>) -> Result<Report> {
    let media = SavfMedia::read(req.stream.clone()).map_err(|e| Error::Install(e.to_string()))?;
    let pid: ProductId = req
        .product
        .parse()
        .map_err(|_| Error::Install(format!("{} is not a product ID", req.product)))?;
    let mut options: Vec<OptionNo> = if req.options.is_empty() {
        media
            .loads()
            .iter()
            .filter(|m| m.load.key.product == pid && m.load.key.load_type == LoadType::Code)
            .map(|m| m.load.key.option)
            .collect()
    } else {
        req.options.clone()
    };
    options.sort();
    options.dedup();
    if options.is_empty() {
        return Err(Error::Install(format!("{pid} is not on the media")));
    }
    let requests: Vec<RstLicPgm> = options
        .iter()
        .map(|o| RstLicPgm {
            objects: Objects::All,
            ..RstLicPgm::new(pid.clone(), *o)
        })
        .collect();
    let hooks = InstallHooks {
        key: req.key,
        verified: StdCell::new(0),
        bad: StdCell::new(0),
    };
    let cell = RefCell::new(store);
    let mut report = Report::default();
    let mut p = Products::new(&cell, qsys, stamp(req.now));
    let plan = plan_restore(&p, &media, &req.system, &requests)
        .map_err(|e| Error::Install(e.to_string()))?;
    if !plan.agreements.is_empty() {
        let mut book = p.agreements();
        let today = Date::from_days(i64::try_from(req.now / 86_400).unwrap_or(0));
        for (prd, rel) in &plan.agreements {
            book.accept(prd, *rel, req.accepted_by, today);
        }
        p.save_agreements(&book)
            .map_err(|e| Error::Install(e.to_string()))?;
    }
    // What the installed loads had, for the replacement.
    let old: BTreeSet<(String, String, String)> = plan
        .steps
        .iter()
        .filter_map(|s| s.replaces.as_ref())
        .flat_map(|(_, l)| l.objects.clone())
        .map(|o| (o.lib, o.name, o.typ))
        .collect();
    let mut r = Once {
        inner: SaveRestorer::new(&cell, qsys, JobHandle(0), &hooks, req.now),
        done: Vec::new(),
    };
    let done = execute_restore(&plan, &mut p, &media, &mut r, &mut NoExits, req.now)
        .map_err(|e| Error::Install(e.to_string()))?;
    let new: BTreeSet<(String, String, String)> = done
        .iter()
        .flat_map(|d| d.objects.iter())
        .map(|o| (o.lib.clone(), o.name.clone(), o.typ.clone()))
        .collect();
    report.objects = new.len();
    report.replaced = old.intersection(&new).count();
    let gone: Vec<ObjEntry> = old
        .difference(&new)
        .map(|(l, n, t)| ObjEntry::new(l, n, t))
        .collect();
    drop(p);
    if !gone.is_empty() {
        let mut s = cell.borrow_mut();
        let mut t = s.begin().map_err(|e| Error::Install(e.to_string()))?;
        for o in &gone {
            if delete_object(&mut t, qsys, o).map_err(|e| Error::Install(e.to_string()))? {
                report.removed += 1;
            }
        }
        t.commit().map_err(|e| Error::Install(e.to_string()))?;
    }
    report.loads = done.into_iter().map(|d| d.key).collect();
    report.verified = hooks.verified.get();
    report.bad_signatures = hooks.bad.get();
    Ok(report)
}

/// Delete `o` if it is there; whether it was.
fn delete_object(t: &mut Txn<'_>, qsys: ObjectId, o: &ObjEntry) -> store::Result<bool> {
    let (Ok(lib), Ok(name)) = (Name::parse(&o.lib), Name::parse(&o.name)) else {
        return Ok(false);
    };
    let Some(ty) = types::type_by_name(&o.typ) else {
        return Ok(false);
    };
    let Ok(l) = ops::find_library(t, qsys, lib) else {
        return Ok(false);
    };
    let hit = ops::list(t, l.id, Some(ty.code))?
        .into_iter()
        .find(|(k, _)| k.name == name)
        .map(|(_, id)| id);
    match hit {
        Some(id) => {
            ops::delete(t, JobHandle(0), id)?;
            Ok(true)
        }
        None => Ok(false),
    }
}

/// Libraries whose programs are the products' (replaced with them).
const NOT_USER: [&str; 5] = ["QSYS", "QSYS2", "QTEMP", "QSPL", "QHLPSYS"];

/// Program objects in the libraries an upgrade translates again.
pub const PROGRAM_TYPES: [u16; 3] = [types::PGM, types::SRVPGM, types::MODULE];

/// After an install of `kind`: every program, service program and
/// module outside the system libraries through `retranslate(txn, id,
/// all)` (`all` after an upgrade, so the new release's code generator
/// makes every native copy). The number of modules translated.
pub fn retranslate_user_programs(
    store: &mut Store,
    qsys: ObjectId,
    kind: InstallKind,
    retranslate: &mut dyn FnMut(&mut Txn<'_>, ObjectId, bool) -> std::result::Result<u32, String>,
) -> Result<u32> {
    let all = matches!(kind, InstallKind::Upgrade { .. });
    let mut t = store.begin().map_err(|e| Error::Install(e.to_string()))?;
    let libs = ops::libraries(&mut t, qsys).map_err(|e| Error::Install(e.to_string()))?;
    let mut n = 0;
    for (name, lib) in libs {
        let text = name.to_string();
        let text = text.trim();
        if NOT_USER.contains(&text) {
            continue;
        }
        for ty in PROGRAM_TYPES {
            let ids = ops::list(&mut t, lib, Some(ty)).map_err(|e| Error::Install(e.to_string()))?;
            for (_, id) in ids {
                n += retranslate(&mut t, id, all).map_err(Error::Install)?;
            }
        }
    }
    t.commit().map_err(|e| Error::Install(e.to_string()))?;
    Ok(n)
}

/// The rule for installing `media` over what the store says it is.
pub fn install_kind(store: &Store, media: release::Release) -> Result<InstallKind> {
    Ok(release::check_upgrade(store.asp_release(), media)?)
}

/// One line for a log: the loads and counts.
#[must_use]
pub fn summary(r: &Report) -> String {
    format!(
        "{}: {} objects ({} replaced, {} removed), {} signatures verified, {} modules translated again",
        r.loads.iter().map(key_name).collect::<Vec<_>>().join(", "),
        r.objects,
        r.replaced,
        r.removed,
        r.verified,
        r.retranslated
    )
}
