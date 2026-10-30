// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The communications objects in `QSYS`: line descriptions (`*LIND`),
//! the network controllers (`*CTLD`) and devices (`*DEVD`) automatic
//! configuration makes for them, read, written and deleted in a
//! transaction of their own.

use cmncfg::Lind;
use cmncfg::names::{CTLD_TYPE, LIND_TYPE};
use qsname::Name;
use store::obj::library::Library;
use store::obj::{JobHandle, ObjectId, ops, types};
use store::txn::Txn;
use wm::obj::ctld::Ctld;
use wm::obj::devd::Devd;

use crate::ipl::system::{System, now_unix};

/// Run `f` over `QSYS` in a transaction.
pub(super) fn in_qsys<T>(
    system: &mut System,
    f: impl FnOnce(&mut Txn<'_>, ObjectId) -> Result<T, String>,
) -> Result<T, String> {
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    crate::svc::db::with_txn(&mut s.store, |txn| f(txn, qsys))
}

fn name(n: &str) -> Result<Name, String> {
    Name::parse(n).map_err(|e| format!("CPD0078 {n}: {e}"))
}

fn lookup(txn: &mut Txn<'_>, qsys: ObjectId, ty: u16, n: &str) -> Result<Option<ObjectId>, String> {
    let lib = Library::load(txn, qsys).map_err(|e| e.to_string())?;
    lib.lookup(txn, ty, name(n)?).map_err(|e| e.to_string())
}

fn list(txn: &mut Txn<'_>, qsys: ObjectId, ty: u16) -> Result<Vec<(String, ObjectId)>, String> {
    let lib = Library::load(txn, qsys).map_err(|e| e.to_string())?;
    Ok(lib
        .list(txn, Some(ty))
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|(k, id)| (k.name.to_string(), id))
        .collect())
}

fn stamp(system: &System) -> wm::obj::Stamp {
    wm::obj::Stamp {
        owner: system.svc_user.clone().unwrap_or_else(|| "QSYS".into()),
        system: "QS400".into(),
        now: now_unix(),
    }
}

/// Every line, and the controllers and devices attached to lines.
#[derive(Debug, Default)]
pub(super) struct All {
    /// Each line description by name.
    pub lines: Vec<(String, Lind)>,
    /// Each network controller and its line.
    pub ctls: Vec<(String, String)>,
    /// Each network device and its controller.
    pub devs: Vec<(String, String)>,
}

/// Read every line, network controller and network device.
pub(super) fn load(system: &mut System) -> Result<All, String> {
    in_qsys(system, |txn, qsys| {
        let mut all = All::default();
        for (n, id) in list(txn, qsys, LIND_TYPE)? {
            let b = wm::obj::read(txn, id).map_err(|e| e.to_string())?;
            match Lind::decode(&b) {
                Ok(l) => all.lines.push((n, l)),
                Err(e) => eprintln!("qsstg: line {n}: {e}"),
            }
        }
        for (n, id) in list(txn, qsys, CTLD_TYPE)? {
            let b = wm::obj::read(txn, id).map_err(|e| e.to_string())?;
            if let Ok(c) = Ctld::decode(&b)
                && !c.line.is_empty()
            {
                all.ctls.push((n, c.line));
            }
        }
        for (n, id) in list(txn, qsys, types::DEVD)? {
            let b = wm::obj::read(txn, id).map_err(|e| e.to_string())?;
            if let Ok(d) = Devd::decode(&b)
                && d.is_network()
            {
                all.devs.push((n, d.ctl));
            }
        }
        Ok(all)
    })
}

/// The line descriptions' names, texts and resources, in name order.
pub(super) fn lines(system: &mut System) -> Result<Vec<(String, Lind)>, String> {
    load(system).map(|a| a.lines)
}

/// Line `n`, `CPF2601` when there is none.
pub(super) fn read_lind(system: &mut System, n: &str) -> Result<Lind, String> {
    let n = n.to_string();
    in_qsys(system, |txn, qsys| {
        let id = lookup(txn, qsys, LIND_TYPE, &n)?.ok_or_else(|| {
            format!(
                "{} line description {n} not found",
                cmncfg::msgs::LINE_NOT_FOUND
            )
        })?;
        let b = wm::obj::read(txn, id).map_err(|e| e.to_string())?;
        Lind::decode(&b).map_err(|e| e.to_string())
    })
}

/// Create line `n`; `CPF2606` when one exists.
pub(super) fn create_lind(system: &mut System, n: &str, l: &Lind) -> Result<(), String> {
    let st = stamp(system);
    let n = n.to_string();
    in_qsys(system, |txn, qsys| {
        if lookup(txn, qsys, LIND_TYPE, &n)?.is_some() {
            return Err(format!(
                "{} line description {n} exists already",
                cmncfg::msgs::LINE_EXISTS
            ));
        }
        wm::obj::create(txn, qsys, name(&n)?, LIND_TYPE, &l.text, &st, &l.encode())
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
}

/// Replace line `n`'s payload and text.
pub(super) fn write_lind(system: &mut System, n: &str, l: &Lind) -> Result<(), String> {
    let n = n.to_string();
    in_qsys(system, |txn, qsys| {
        let id = lookup(txn, qsys, LIND_TYPE, &n)?.ok_or_else(|| {
            format!(
                "{} line description {n} not found",
                cmncfg::msgs::LINE_NOT_FOUND
            )
        })?;
        wm::obj::write(txn, id, &l.encode()).map_err(|e| e.to_string())?;
        ops::change_text(txn, id, &l.text, now_unix()).map_err(|e| e.to_string())
    })
}

/// Delete line `n` and the controllers and devices automatic
/// configuration made for it: what was deleted, controllers first.
pub(super) fn delete_lind(system: &mut System, n: &str) -> Result<Vec<(String, bool)>, String> {
    let n = n.to_string();
    in_qsys(system, |txn, qsys| {
        let id = lookup(txn, qsys, LIND_TYPE, &n)?.ok_or_else(|| {
            format!(
                "{} line description {n} not found",
                cmncfg::msgs::LINE_NOT_FOUND
            )
        })?;
        let mut gone = Vec::new();
        for (c, cid) in list(txn, qsys, CTLD_TYPE)? {
            let b = wm::obj::read(txn, cid).map_err(|e| e.to_string())?;
            let Ok(ctl) = Ctld::decode(&b) else { continue };
            if ctl.line != n || !ctl.auto {
                continue;
            }
            for (d, did) in list(txn, qsys, types::DEVD)? {
                let b = wm::obj::read(txn, did).map_err(|e| e.to_string())?;
                if Devd::decode(&b).is_ok_and(|dev| dev.is_network() && dev.ctl == c) {
                    ops::delete(txn, JobHandle(0), did).map_err(|e| e.to_string())?;
                    gone.push((d, false));
                }
            }
            ops::delete(txn, JobHandle(0), cid).map_err(|e| e.to_string())?;
            gone.push((c, true));
        }
        ops::delete(txn, JobHandle(0), id).map_err(|e| e.to_string())?;
        Ok(gone)
    })
}

/// What [`ensure_net`] found or made.
#[derive(Debug)]
pub(super) struct Net {
    /// The network controller.
    pub ctl: String,
    /// The TCP/IP device under it.
    pub dev: String,
    /// The controller was made now.
    pub made_ctl: bool,
    /// The device was made now.
    pub made_dev: bool,
}

/// Line `line`'s network controller and its TCP/IP device, made by
/// automatic configuration when missing, named as IBM names them
/// (`cmncfg::names`): a name is taken when a controller or device of it
/// exists or `attached(name)` says another line has it.
pub(super) fn ensure_net(
    system: &mut System,
    line: &str,
    attached: &dyn Fn(&str) -> bool,
) -> Result<Net, String> {
    let st = stamp(system);
    let line = line.to_string();
    in_qsys(system, |txn, qsys| {
        let mut ctl = None;
        for (c, cid) in list(txn, qsys, CTLD_TYPE)? {
            let b = wm::obj::read(txn, cid).map_err(|e| e.to_string())?;
            if Ctld::decode(&b).is_ok_and(|x| x.line == line && x.ctltype == cmncfg::names::NET_CTL)
            {
                ctl = Some(c);
                break;
            }
        }
        let mut taken_names = Vec::new();
        for ty in [CTLD_TYPE, types::DEVD] {
            taken_names.extend(list(txn, qsys, ty)?.into_iter().map(|(n, _)| n));
        }
        let taken = |n: &str| taken_names.iter().any(|t| t == n) || attached(n);
        let made_ctl = ctl.is_none();
        let ctl = if let Some(c) = ctl {
            c
        } else {
            let c = cmncfg::names::controller(&line, taken).ok_or_else(|| {
                format!(
                    "{} no free controller name for line {line}",
                    cmncfg::msgs::AUTO_CTL_FAILED
                )
            })?;
            let x = Ctld::network(&line);
            wm::obj::create(txn, qsys, name(&c)?, CTLD_TYPE, &x.text, &st, &x.encode())
                .map_err(|e| e.to_string())?;
            c
        };
        let mut dev = None;
        for (d, did) in list(txn, qsys, types::DEVD)? {
            let b = wm::obj::read(txn, did).map_err(|e| e.to_string())?;
            if Devd::decode(&b).is_ok_and(|x| x.is_network() && x.ctl == ctl) {
                dev = Some(d);
                break;
            }
        }
        let made_dev = dev.is_none();
        let dev = if let Some(d) = dev {
            d
        } else {
            let taken = |n: &str| taken_names.iter().any(|t| t == n) || n == ctl || attached(n);
            let d = cmncfg::names::device(&line, taken).ok_or_else(|| {
                format!(
                    "{} no free device name for line {line}",
                    cmncfg::msgs::AUTO_DEV_FAILED
                )
            })?;
            let x = Devd::network(&ctl);
            wm::obj::create(txn, qsys, name(&d)?, types::DEVD, &x.text, &st, &x.encode())
                .map_err(|e| e.to_string())?;
            d
        };
        Ok(Net {
            ctl,
            dev,
            made_ctl,
            made_dev,
        })
    })
}

/// A controller or device that is not a line's (`QPACTL01`, displays,
/// printers): its description, for the status displays.
pub(super) fn ctlds(system: &mut System) -> Result<Vec<(String, Ctld)>, String> {
    in_qsys(system, |txn, qsys| {
        let mut out = Vec::new();
        for (n, id) in list(txn, qsys, CTLD_TYPE)? {
            let b = wm::obj::read(txn, id).map_err(|e| e.to_string())?;
            if let Ok(c) = Ctld::decode(&b) {
                out.push((n, c));
            }
        }
        Ok(out)
    })
}

/// Every device description.
pub(super) fn devds(system: &mut System) -> Result<Vec<(String, Devd)>, String> {
    in_qsys(system, |txn, qsys| {
        let mut out = Vec::new();
        for (n, id) in list(txn, qsys, types::DEVD)? {
            let b = wm::obj::read(txn, id).map_err(|e| e.to_string())?;
            if let Ok(d) = Devd::decode(&b) {
                out.push((n, d));
            }
        }
        Ok(out)
    })
}

/// Vary a controller that is not a line's (`QPACTL01`) on or off.
pub(super) fn vary_plain_ctl(system: &mut System, n: &str, on: bool) -> Result<(), String> {
    let n = n.to_string();
    in_qsys(system, |txn, qsys| {
        let id = lookup(txn, qsys, CTLD_TYPE, &n)?
            .ok_or_else(|| format!("{} controller {n} not found", cmncfg::msgs::CTL_NOT_FOUND))?;
        let b = wm::obj::read(txn, id).map_err(|e| e.to_string())?;
        let mut c = Ctld::decode(&b).map_err(|e| e.to_string())?;
        c.online = on;
        wm::obj::write(txn, id, &c.encode()).map_err(|e| e.to_string())
    })
}

/// Vary a device that is not a line's on or off.
pub(super) fn vary_plain_dev(system: &mut System, n: &str, on: bool) -> Result<(), String> {
    let n = n.to_string();
    in_qsys(system, |txn, qsys| {
        let id = lookup(txn, qsys, types::DEVD, &n)?.ok_or_else(|| {
            format!(
                "{} device description {n} not found",
                cmncfg::msgs::DEV_NOT_FOUND
            )
        })?;
        let b = wm::obj::read(txn, id).map_err(|e| e.to_string())?;
        let mut d = Devd::decode(&b).map_err(|e| e.to_string())?;
        d.online = on;
        d.status = if on {
            wm::obj::devd::DevStatus::VariedOn
        } else {
            wm::obj::devd::DevStatus::VariedOff
        };
        wm::obj::write(txn, id, &d.encode()).map_err(|e| e.to_string())
    })
}
