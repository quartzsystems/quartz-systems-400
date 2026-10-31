// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Tape, optical and media library device descriptions in `QSYS`:
//! made, changed and read here, varied on and off with IBM's statuses
//! (VARIED OFF, VARIED ON, ACTIVE for optical, UNAVAILABLE when another
//! varied-on device has the resource, FAILED when the resource is
//! gone), made for new hardware under `QAUTOCFG`, and shown by
//! `DSPDEVD`.

use std::fmt::Write as _;

use store::obj::{ObjectId, ops, types};
use store::txn::Txn;
use wm::obj::devd::{DevStatus, Devd};
use wm::obj::devmed::{MediaDevice, MediaKind, virtual_rsrc};

use crate::hwrsrc::{Kind, Status};
use crate::ipl::system::{System, now_unix};

/// Device description not found.
pub(crate) const NOT_FOUND: &str = "CPF2702";
/// Device description exists.
pub(crate) const EXISTS: &str = "CPF2605";
/// The device is not varied on.
pub(crate) const NOT_AVAILABLE: &str = "CPF6721";

fn with_qsys<T>(
    system: &mut System,
    f: impl FnOnce(&mut Txn<'_>, ObjectId) -> Result<T, String>,
) -> Result<T, String> {
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    crate::svc::db::with_txn(&mut s.store, |txn| f(txn, qsys))
}

fn find(txn: &mut Txn<'_>, qsys: ObjectId, name: &str) -> Result<Option<(ObjectId, Devd)>, String> {
    let n = qsname::Name::parse(name).map_err(|e| format!("CPD0078 {name}: {e}"))?;
    let lib = store::obj::library::Library::load(txn, qsys).map_err(|e| e.to_string())?;
    let Some(id) = lib.lookup(txn, types::DEVD, n).map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let b = wm::obj::read(txn, id).map_err(|e| e.to_string())?;
    Ok(Some((id, Devd::decode(&b).map_err(|e| e.to_string())?)))
}

/// Every device description, by name.
pub(crate) fn all(system: &mut System) -> Result<Vec<(String, Devd)>, String> {
    with_qsys(system, |txn, qsys| {
        let mut out = Vec::new();
        for (k, id) in ops::list(txn, qsys, Some(types::DEVD)).map_err(|e| e.to_string())? {
            if let Ok(d) = wm::obj::read(txn, id).map_err(|e| e.to_string()).and_then(|b| Devd::decode(&b).map_err(|e| e.to_string())) {
                out.push((k.name.to_string(), d));
            }
        }
        Ok(out)
    })
}

/// A media device's description; `CPF2702` when there is none or it
/// is not a media device.
pub(crate) fn read(system: &mut System, name: &str) -> Result<Devd, String> {
    let name = name.to_ascii_uppercase();
    with_qsys(system, |txn, qsys| {
        match find(txn, qsys, &name)? {
            Some((_, d)) if d.media.is_some() => Ok(d),
            Some(_) => Err(format!("{NOT_FOUND} device {name} is not a tape, optical or media library device")),
            None => Err(format!("{NOT_FOUND} device description {name} not found")),
        }
    })
}

/// Replace a media device's description.
pub(crate) fn write(system: &mut System, name: &str, d: &Devd) -> Result<(), String> {
    let name = name.to_ascii_uppercase();
    with_qsys(system, |txn, qsys| {
        let (id, _) = find(txn, qsys, &name)?.ok_or_else(|| format!("{NOT_FOUND} device description {name} not found"))?;
        wm::obj::write(txn, id, &d.encode()).map_err(|e| e.to_string())?;
        ops::change_text(txn, id, &d.text, now_unix()).map_err(|e| e.to_string())
    })
}

/// Make a media device's description; `CPF2602` when the name is taken.
pub(crate) fn create(system: &mut System, name: &str, d: &Devd, owner: &str) -> Result<(), String> {
    let name = name.to_ascii_uppercase();
    let st = wm::obj::Stamp {
        owner: owner.into(),
        system: "QS400".into(),
        now: now_unix(),
    };
    with_qsys(system, |txn, qsys| {
        if find(txn, qsys, &name)?.is_some() {
            return Err(format!("{EXISTS} device description {name} already exists"));
        }
        let n = qsname::Name::parse(&name).map_err(|e| format!("CPD0078 {name}: {e}"))?;
        wm::obj::create(txn, qsys, n, types::DEVD, &d.text, &st, &d.encode())
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
}

/// The next free virtual resource name of `kind`: `TAPVRT01`, `OPTVRT01`.
pub(crate) fn next_virtual(system: &mut System, kind: MediaKind) -> Result<String, String> {
    let used: Vec<String> = all(system)?
        .into_iter()
        .filter_map(|(_, d)| d.media.map(|m| m.rsrcname))
        .collect();
    (1..100)
        .map(|n| virtual_rsrc(kind, n))
        .find(|r| !used.contains(r))
        .ok_or_else(|| "CPF2605 no virtual resource name is free".to_string())
}

/// The type and model a real resource reports, as the description keeps them.
pub(crate) fn real_type(kind: MediaKind) -> (&'static str, &'static str) {
    match kind {
        MediaKind::Tape => ("3580", "004"),
        MediaKind::Optical => ("632C", "002"),
        MediaKind::TapeLibrary => ("3573", "001"),
    }
}

/// Whether hardware resource `rsrc` of `kind` is there now.
fn present(system: &System, kind: MediaKind, rsrc: &str) -> bool {
    let want = match kind {
        MediaKind::Tape => Kind::Tape,
        MediaKind::Optical => Kind::Optical,
        MediaKind::TapeLibrary => Kind::TapeLibrary,
    };
    system
        .hw
        .resources
        .iter()
        .any(|r| r.kind == want && r.name == rsrc && r.status == Status::Operational)
}

/// The host node of a real resource: `st0`, `sg3`, `sr0`.
pub(crate) fn node_of(system: &System, rsrc: &str) -> Option<String> {
    system
        .hw
        .resources
        .iter()
        .find(|r| r.name == rsrc)
        .map(|r| r.location.clone())
}

/// The status a media device takes when varied on.
fn on_status(system: &mut System, name: &str, d: &Devd) -> DevStatus {
    let Some(m) = &d.media else { return DevStatus::VariedOn };
    if !m.is_virtual() && !present(system, m.kind, &m.rsrcname) {
        return DevStatus::Failed;
    }
    let taken = all(system).unwrap_or_default().into_iter().any(|(n, o)| {
        n != name
            && !matches!(o.status, DevStatus::VariedOff | DevStatus::Failed | DevStatus::Unavailable)
            && o.media.as_ref().is_some_and(|x| x.rsrcname == m.rsrcname)
    });
    if taken {
        DevStatus::Unavailable
    } else if m.kind == MediaKind::Optical {
        DevStatus::Active
    } else {
        DevStatus::VariedOn
    }
}

/// Vary media device `name` on or off: the completion message, or the
/// refusal (`CPD26D4` when the resource is gone; the device is FAILED).
pub(crate) fn vary(system: &mut System, name: &str, on: bool) -> Result<Vec<crate::stgsvc::cmn::Note>, String> {
    let name = name.to_ascii_uppercase();
    let mut d = read(system, &name)?;
    d.status = if on { on_status(system, &name, &d) } else { DevStatus::VariedOff };
    write(system, &name, &d)?;
    if d.status == DevStatus::Failed {
        let rsrc = d.media.as_ref().map(|m| m.rsrcname.clone()).unwrap_or_default();
        return Err(format!("{} resource {rsrc} of device {name} not found", cmncfg::msgs::RSRC_NOT_FOUND));
    }
    Ok(vec![(
        if on {
            cmncfg::msgs::DEV_VARIED_ON
        } else {
            cmncfg::msgs::DEV_VARIED_OFF
        },
        name,
    )])
}

/// Whether device `d` is varied on and usable.
pub(crate) fn usable(d: &Devd) -> bool {
    matches!(d.status, DevStatus::VariedOn | DevStatus::Active)
}

/// At service start: every media device `ONLINE(*YES)` varied on, the
/// others off.
pub(crate) fn vary_online(system: &mut System) {
    let devs = all(system).unwrap_or_default();
    for (n, d) in devs.into_iter().filter(|(_, d)| d.media.is_some()) {
        let r = if d.online { vary(system, &n, true) } else { vary(system, &n, false) };
        if let Err(e) = r {
            eprintln!("qsstg: vary {n} at IPL: {e}");
        }
    }
}

/// Descriptions for the tape drives, changers and optical drives no
/// description names, when `QAUTOCFG` is on: named as the resource,
/// `ONLINE(*YES)`. How many were made.
pub(crate) fn autoconfigure(system: &mut System) -> usize {
    if crate::stgsvc::security::sysval_text(system, "QAUTOCFG") != "1" {
        return 0;
    }
    let devs = all(system).unwrap_or_default();
    let found: Vec<(String, MediaKind)> = system
        .hw
        .resources
        .iter()
        .filter(|r| r.status == Status::Operational)
        .filter_map(|r| match r.kind {
            Kind::Tape => Some((r.name.clone(), MediaKind::Tape)),
            Kind::TapeLibrary => Some((r.name.clone(), MediaKind::TapeLibrary)),
            Kind::Optical if r.location.starts_with("sr") => Some((r.name.clone(), MediaKind::Optical)),
            _ => None,
        })
        .collect();
    let mut made = 0;
    for (rsrc, kind) in found {
        let described = devs
            .iter()
            .any(|(n, d)| *n == rsrc || d.media.as_ref().is_some_and(|m| m.rsrcname == rsrc));
        if described {
            continue;
        }
        let (ty, model) = real_type(kind);
        let mut d = Devd::media_device(
            ty,
            model,
            MediaDevice {
                kind,
                assign: true,
                unload: true,
                rsrcname: rsrc.clone(),
                ..MediaDevice::default()
            },
        );
        d.online = true;
        d.text = "Created by automatic configuration".into();
        match create(system, &rsrc, &d, "QSYS") {
            Ok(()) => made += 1,
            Err(e) => eprintln!("qsstg: automatic configuration of {rsrc}: {e}"),
        }
    }
    made
}

/// `DSPDEVD` of a media device.
pub(crate) fn describe(name: &str, d: &Devd) -> String {
    let m = d.media.clone().unwrap_or_default();
    let yn = |b: bool| if b { "*YES" } else { "*NO" };
    let (ml, mn) = d.msgq.split_once('/').unwrap_or(("QSYS", "QSYSOPR"));
    let cat = match m.kind {
        MediaKind::Tape => "*TAP",
        MediaKind::Optical => "*OPT",
        MediaKind::TapeLibrary => "*TAPMLB",
    };
    let mut s = String::new();
    let mut row = |label: &str, kwd: &str, v: &str| {
        let _ = writeln!(s, " {label:.<35}:   {kwd:<11} {v}");
    };
    row("Device description ", "DEVD", name);
    row("Option ", "OPTION", "*ALL");
    row("Category of device ", "", cat);
    row("Device type  ", "TYPE", &d.devtype);
    row("Device model ", "MODEL", &d.model);
    row("Resource name  ", "RSRCNAME", &m.rsrcname);
    if m.kind == MediaKind::Optical {
        row("Local internet address ", "LCLINTNETA", "*NONE");
    }
    row("Online at IPL  ", "ONLINE", yn(d.online));
    match m.kind {
        MediaKind::Tape => {
            row("Assign device at vary on ", "ASSIGN", yn(m.assign));
            row("Unload device at vary off  ", "UNLOAD", yn(m.unload));
        }
        MediaKind::TapeLibrary => {
            let wait = |n: u32| if n == 0 { "*SYSGEN".to_string() } else { n.to_string() };
            row("Unload wait time ", "UNLOADWAIT", &wait(m.unload_wait));
            row("Maximum device wait time ", "MAXDEVTIME", &wait(m.max_dev_time));
            row("Generate cartridge ids ", "GENCTGID", if m.gen_ctg_sysgen { "*SYSGEN" } else { "*VOLID" });
        }
        MediaKind::Optical => {}
    }
    row("Status ", "", d.status.text());
    row("Message queue  ", "MSGQ", mn);
    row("  Library  ", "", ml);
    row("Text ", "TEXT", if d.text.is_empty() { "*BLANK" } else { &d.text });
    s
}
