// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `LODPTF`, `APYPTF` and `RMVPTF` over the store (`docs/PTF.md` §3).
//! Replaced objects wait in `QRPLOBJ` under generated names until the
//! apply is made permanent or the PTF removed.

use std::io::Cursor;

use savfmt::Reader;
use saverst::{AllowDiff, AllowRestore, Hooks, RestoreOptions};
use store::Store;
use store::obj::library::Library;
use store::obj::{JobHandle, ObjectId, ops, types};

use crate::db::{PtfDb, PtfRecord, Replaced};
use crate::header::PtfKind;
use crate::package::Package;
use crate::status::{Action, Status};
use crate::{Error, Result};

/// The library replaced objects wait in.
pub const QRPLOBJ: &str = "QRPLOBJ";

/// Messages.
pub mod msg {
    /// A prerequisite PTF is not applied.
    pub const PREREQ: &str = "CPF3612";
    /// The PTF is not loaded.
    pub const NOT_LOADED: &str = "CPF3606";
    /// The PTF is superseded.
    pub const SUPERSEDED: &str = "CPF3613";
    /// An applied PTF needs this one.
    pub const NEEDED: &str = "CPF3618";
    /// The PTF is for another product or release.
    pub const WRONG_PRODUCT: &str = "CPF3598";
    /// Already in that state.
    pub const ALREADY: &str = "CPF3601";
}

/// What the store side of an apply needs.
pub struct Ctx<'a> {
    /// The object store.
    pub store: &'a mut Store,
    /// The job doing it.
    pub job: JobHandle,
    /// `QSYS`.
    pub qsys: ObjectId,
    /// The security layer.
    pub hooks: &'a dyn Hooks,
    /// Unix seconds.
    pub now: u64,
}

/// `LODPTF`: record a PTF whose stream was opened (and so verified) and
/// kept as save file `savf`. PTFs it supersedes become Superseded.
pub fn load(db: &mut PtfDb, pkg: &Package, savf: &str, release: &str, now: u64) -> Result<()> {
    let h = &pkg.header;
    if h.release != release {
        return Err(Error::refused(
            msg::WRONG_PRODUCT,
            format!("{} is for {}", h.ptf, h.release),
        ));
    }
    if let Some(r) = db.ptfs.get(&h.ptf)
        && !matches!(r.status, Status::NotLoaded | Status::OnOrder | Status::PermRemoved | Status::SaveFileOnly)
    {
        return Err(Error::refused(msg::ALREADY, format!("{} is loaded", h.ptf)));
    }
    for s in &h.supersedes {
        if let Some(old) = db.ptfs.get_mut(&s.ptf) {
            old.status = Status::Superseded;
            old.superseded_by.clone_from(&h.ptf);
            old.changed = now;
        }
    }
    db.ptfs.insert(
        h.ptf.clone(),
        PtfRecord {
            ptf: h.ptf.clone(),
            product: h.product.clone(),
            option: h.option.clone(),
            release: h.release.clone(),
            status: Status::Loaded,
            action: Action::None,
            delayed: h.kind == PtfKind::Delayed,
            lic: h.lic,
            savf: savf.into(),
            superseded_by: String::new(),
            prereqs: h.prereqs.clone(),
            coreqs: h.coreqs.clone(),
            replaced: Vec::new(),
            changed: now,
        },
    );
    Ok(())
}

fn applied_or_superseded(db: &PtfDb, ptf: &str, batch: &[String]) -> bool {
    batch.iter().any(|b| b == ptf)
        || db
            .get(ptf)
            .is_some_and(|r| r.status.applied() || r.status == Status::Superseded)
}

/// Whether `ptf` may be applied: loaded, not superseded, its
/// prerequisites and co-requisites applied or in `batch`.
pub fn check_apply(db: &PtfDb, ptf: &str, batch: &[String]) -> Result<()> {
    let r = db
        .get(ptf)
        .ok_or_else(|| Error::refused(msg::NOT_LOADED, ptf.to_string()))?;
    match r.status {
        Status::Loaded | Status::TempRemoved => {}
        Status::Superseded => return Err(Error::refused(msg::SUPERSEDED, ptf.to_string())),
        Status::TempApplied | Status::PermApplied => {
            return Err(Error::refused(msg::ALREADY, ptf.to_string()));
        }
        _ => return Err(Error::refused(msg::NOT_LOADED, ptf.to_string())),
    }
    for p in r.prereqs.iter().chain(&r.coreqs) {
        if !applied_or_superseded(db, &p.ptf, batch) {
            return Err(Error::refused(msg::PREREQ, format!("{ptf} needs {}", p.ptf)));
        }
    }
    Ok(())
}

/// `APYPTF APY(*TEMP)`: an immediate PTF is applied now; a delayed or
/// LIC one is marked Apply at next IPL. `bytes` is its stream.
pub fn apply_temp(
    cx: &mut Ctx<'_>,
    db: &mut PtfDb,
    pkg: &Package,
    bytes: &[u8],
    batch: &[String],
    force_now: bool,
) -> Result<Action> {
    let ptf = pkg.header.ptf.clone();
    check_apply(db, &ptf, batch)?;
    let r = db.ptfs.get_mut(&ptf).expect("checked");
    if (r.delayed || r.lic) && !force_now {
        r.action = Action::ApplyAtIpl;
        r.changed = cx.now;
        return Ok(Action::ApplyAtIpl);
    }
    let replaced = replace(cx, pkg, bytes)?;
    let r = db.ptfs.get_mut(&ptf).expect("checked");
    r.replaced = replaced;
    r.status = Status::TempApplied;
    r.action = Action::None;
    r.changed = cx.now;
    Ok(Action::None)
}

/// The library `QRPLOBJ`, made when missing.
fn qrplobj(cx: &mut Ctx<'_>) -> Result<ObjectId> {
    let mut t = cx.store.begin()?;
    let name = qsname::Name::parse(QRPLOBJ).expect("name");
    let id = match ops::find_library(&mut t, cx.qsys, name) {
        Ok(f) => f.id,
        Err(_) => ops::create(
            &mut t,
            &ops::CreateParams {
                library: cx.qsys,
                name,
                type_code: types::LIB,
                attribute: String::new(),
                text: "Objects replaced by PTFs".into(),
                owner: "QSYS".into(),
                system: String::new(),
                now: cx.now,
                by_system: true,
            },
        )?,
    };
    t.commit()?;
    Ok(id)
}

fn lookup(t: &mut store::Txn<'_>, qsys: ObjectId, lib: &str, name: &str, ty: u16) -> Result<Option<ObjectId>> {
    let Ok(n) = qsname::Name::parse(lib) else { return Ok(None) };
    let Ok(l) = ops::find_library(t, qsys, n) else { return Ok(None) };
    let n = qsname::Name::parse(name).map_err(|e| Error::NotPtf(e.to_string()))?;
    Ok(Library::load(t, l.id)?.lookup(t, ty, n)?)
}

/// Move the objects the PTF replaces into `QRPLOBJ`, then restore the
/// new ones; on failure every moved object goes back.
fn replace(cx: &mut Ctx<'_>, pkg: &Package, bytes: &[u8]) -> Result<Vec<Replaced>> {
    let rpl = qrplobj(cx)?;
    let mut moved = Vec::new();
    let mut t = cx.store.begin()?;
    let base = t.peek_serial();
    for (i, (lib, name, ty)) in pkg.objects.iter().enumerate() {
        let saved_as = match lookup(&mut t, cx.qsys, lib, name, *ty)? {
            Some(id) => {
                let n = format!("Q{:09}", (base + i as u64) % 1_000_000_000);
                ops::move_to(&mut t, id, rpl, cx.now)?;
                ops::rename(&mut t, id, qsname::Name::parse(&n).expect("name"), cx.now)?;
                n
            }
            None => String::new(),
        };
        moved.push(Replaced {
            library: lib.clone(),
            name: name.clone(),
            type_code: *ty,
            saved_as,
        });
    }
    t.commit()?;
    let opts = RestoreOptions {
        allow: AllowDiff {
            all: true,
            ..AllowDiff::default()
        },
        verify: 1,
        allow_restore: AllowRestore::default(),
        now: cx.now,
        ..RestoreOptions::default()
    };
    let mut r = Reader::new(Cursor::new(bytes))?;
    let rep = saverst::restore(cx.store, cx.job, cx.hooks, cx.qsys, &mut r, &opts);
    let failed = match &rep {
        Ok(rep) => rep.not_restored() > 0 || rep.restored() != pkg.objects.len(),
        Err(_) => true,
    };
    if failed {
        put_back(cx, &moved)?;
        return Err(match rep {
            Err(e) => e.into(),
            Ok(rep) => Error::NotPtf(format!(
                "{} objects of {} not restored",
                rep.not_restored(),
                pkg.header.ptf
            )),
        });
    }
    Ok(moved)
}

/// Delete what the PTF put in place and bring the replaced objects back.
fn put_back(cx: &mut Ctx<'_>, moved: &[Replaced]) -> Result<()> {
    let mut t = cx.store.begin()?;
    for m in moved {
        if let Some(id) = lookup(&mut t, cx.qsys, &m.library, &m.name, m.type_code)? {
            ops::delete(&mut t, cx.job, id)?;
        }
        if m.saved_as.is_empty() {
            continue;
        }
        if let Some(id) = lookup(&mut t, cx.qsys, QRPLOBJ, &m.saved_as, m.type_code)? {
            let lib = ops::find_library(&mut t, cx.qsys, qsname::Name::parse(&m.library).expect("name"))?.id;
            ops::rename(&mut t, id, qsname::Name::parse(&m.name).expect("name"), cx.now)?;
            ops::move_to(&mut t, id, lib, cx.now)?;
        }
    }
    t.commit()?;
    Ok(())
}

/// `APYPTF APY(*PERM)` of a temporarily applied PTF: the replaced
/// objects are deleted.
pub fn apply_perm(cx: &mut Ctx<'_>, db: &mut PtfDb, ptf: &str) -> Result<()> {
    let r = db
        .ptfs
        .get_mut(ptf)
        .ok_or_else(|| Error::refused(msg::NOT_LOADED, ptf.to_string()))?;
    if r.status != Status::TempApplied {
        return Err(Error::refused(msg::NOT_LOADED, format!("{ptf} is not temporarily applied")));
    }
    let mut t = cx.store.begin()?;
    for m in r.replaced.iter().filter(|m| !m.saved_as.is_empty()) {
        if let Some(id) = lookup(&mut t, cx.qsys, QRPLOBJ, &m.saved_as, m.type_code)? {
            ops::delete(&mut t, cx.job, id)?;
        }
    }
    t.commit()?;
    r.replaced.clear();
    r.status = Status::PermApplied;
    r.changed = cx.now;
    Ok(())
}

/// `RMVPTF RMV(*TEMP)`; `permanent` makes it `RMV(*PERM)` (the
/// integrator deletes the save file). Refused while an applied PTF
/// names it as a prerequisite.
pub fn remove(cx: &mut Ctx<'_>, db: &mut PtfDb, ptf: &str, permanent: bool) -> Result<()> {
    let needed = db.ptfs.values().any(|o| {
        o.ptf != ptf && o.status.applied() && o.prereqs.iter().chain(&o.coreqs).any(|p| p.ptf == ptf)
    });
    if needed {
        return Err(Error::refused(msg::NEEDED, ptf.to_string()));
    }
    let r = db
        .ptfs
        .get(ptf)
        .cloned()
        .ok_or_else(|| Error::refused(msg::NOT_LOADED, ptf.to_string()))?;
    match r.status {
        Status::TempApplied => {
            if r.delayed || r.lic {
                let rec = db.ptfs.get_mut(ptf).expect("found");
                rec.action = Action::RemoveAtIpl;
                rec.changed = cx.now;
                return Ok(());
            }
            put_back(cx, &r.replaced)?;
        }
        Status::Loaded | Status::TempRemoved if permanent => {}
        Status::PermApplied => {
            return Err(Error::refused(msg::ALREADY, format!("{ptf} is permanently applied")));
        }
        _ => return Err(Error::refused(msg::NOT_LOADED, ptf.to_string())),
    }
    let rec = db.ptfs.get_mut(ptf).expect("found");
    rec.replaced.clear();
    rec.status = if permanent {
        Status::PermRemoved
    } else {
        Status::TempRemoved
    };
    rec.changed = cx.now;
    Ok(())
}

/// The PTFs the next IPL applies or removes, in ID order.
#[must_use]
pub fn pending(db: &PtfDb) -> Vec<(String, Action)> {
    db.ptfs
        .values()
        .filter(|r| r.action != Action::None)
        .map(|r| (r.ptf.clone(), r.action))
        .collect()
}
