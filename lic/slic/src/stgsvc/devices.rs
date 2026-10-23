// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Display devices for TN5250E sessions: the virtual controller
//! `QPACTL01`, the virtual displays `QPADEVnnnn` made under `QAUTOVRT`,
//! named devices made under `QAUTOCFG`, and their status.

use store::obj::{ObjectId, types};
use store::txn::Txn;
use wm::obj::ctld::Ctld;
use wm::obj::devd::{DevStatus, Devd, VIRTUAL_CONTROLLER, VIRTUAL_PREFIX, virtual_name};

use crate::ipl::system::System;

/// The response codes of RFC 2877 a session gets instead of a device.
pub mod code {
    /// Device not available.
    pub const BUSY: &str = "8902";
    /// Device not valid for the session.
    pub const NOT_VALID: &str = "8903";
    /// Automatic configuration failed or not allowed.
    pub const NO_AUTOCFG: &str = "8940";
    /// Creation of the device failed.
    pub const CREATE_FAILED: &str = "8925";
}

fn in_store<T>(
    system: &mut System,
    f: impl FnOnce(&mut Txn<'_>, ObjectId) -> Result<T, String>,
) -> Result<T, String> {
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    crate::svc::db::with_txn(&mut s.store, |txn| f(txn, qsys))
}

fn find_devd(txn: &mut Txn<'_>, qsys: ObjectId, name: &str) -> Option<(ObjectId, Devd)> {
    let id = wm::obj::find(
        txn,
        qsys,
        &store::obj::ops::NameContext::default(),
        "QSYS",
        name,
        types::DEVD,
    )
    .ok()?;
    let b = wm::obj::read(txn, id).ok()?;
    Some((id, Devd::decode(&b).ok()?))
}

/// The virtual controller, made when missing.
fn ensure_controller(txn: &mut Txn<'_>, qsys: ObjectId) -> Result<(), String> {
    let ctx = store::obj::ops::NameContext::default();
    if wm::obj::find(txn, qsys, &ctx, "QSYS", VIRTUAL_CONTROLLER, 0x001C).is_ok() {
        return Ok(());
    }
    let c = Ctld::virtual_workstation();
    let name = qsname::Name::parse(VIRTUAL_CONTROLLER).map_err(|e| e.to_string())?;
    wm::obj::create(txn, qsys, name, 0x001C, &c.text, &stamp(), &c.encode())
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn sysval_text(system: &System, name: &str) -> String {
    super::security::sysval_text(system, name)
}

/// Devices the server makes belong to the security officer, who may
/// then sign on at them with `QLMTSECOFR` in force.
fn stamp() -> wm::obj::Stamp {
    wm::obj::Stamp {
        owner: "QSECOFR".into(),
        system: "QS400".into(),
        now: crate::ipl::system::now_unix(),
    }
}

fn create_devd(txn: &mut Txn<'_>, qsys: ObjectId, name: &str, d: &Devd) -> Result<(), String> {
    let n = qsname::Name::parse(name).map_err(|_| code::NOT_VALID.to_string())?;
    wm::obj::create(txn, qsys, n, types::DEVD, &d.text, &stamp(), &d.encode())
        .map(|_| ())
        .map_err(|_| code::CREATE_FAILED.to_string())
}

/// The output queue `QUSRSYS/<name>` of a virtual printer, made when
/// missing.
fn ensure_outq(txn: &mut Txn<'_>, qsys: ObjectId, name: &str) -> Result<(), String> {
    let lib = wm::obj::library(txn, qsys, "QUSRSYS").map_err(|e| e.to_string())?;
    let n = qsname::Name::parse(name).map_err(|_| code::NOT_VALID.to_string())?;
    let l = store::obj::library::Library::load(txn, lib).map_err(|e| e.to_string())?;
    if l.lookup(txn, types::OUTQ, n).map_err(|e| e.to_string())?.is_some() {
        return Ok(());
    }
    let q = wm::obj::outq::OutQ::new("Output queue of a TN5250E printer");
    wm::obj::create(txn, lib, n, types::OUTQ, "Output queue of a TN5250E printer", &stamp(), &q.encode())
        .map(|_| ())
        .map_err(|_| code::CREATE_FAILED.to_string())
}

/// A message to the system operator from the telnet server.
fn to_qsysopr(system: &mut System, msgid: &str, text: &str) {
    let m = wm::obj::msgq::QMsg {
        msgid: msgid.into(),
        msgf: ("QCPFMSG".into(), "QSYS".into()),
        severity: 40,
        text: text.into(),
        job: ("QTVTELNET".into(), "QTCP".into(), 0),
        from_pgm: "QTVTELNET".into(),
        ..wm::obj::msgq::QMsg::default()
    };
    let _ = super::msgs::send(system, "QSYS", "QSYSOPR", m);
}

/// A display for a session: the one it named (made under `QAUTOCFG`
/// when missing), else a free virtual display, else a new one within
/// `QAUTOVRT`. Its status becomes "sign-on display". A printer type
/// gets the named printer with its output queue.
#[allow(clippy::too_many_lines)] // the named, the free and the new device
pub fn allocate(
    system: &mut System,
    requested: &str,
    devtype: &str,
    model: &str,
    kbdtype: &str,
) -> Result<String, String> {
    let autocfg = sysval_text(system, "QAUTOCFG") == "1";
    let autovrt = sysval_text(system, "QAUTOVRT");
    let limit: Option<u32> = match autovrt.as_str() {
        "*NOMAX" => None,
        "*REGFAC" => Some(0),
        n => Some(n.parse().unwrap_or(0)),
    };
    let requested = requested.trim().to_ascii_uppercase();
    let printer = wm::obj::devd::is_printer_type(devtype);
    let r = in_store(system, |txn, qsys| {
        ensure_controller(txn, qsys)?;
        if printer {
            // A printer session names its device; it is made under
            // QAUTOCFG with an output queue of its own in QUSRSYS.
            if requested.is_empty() || qsname::Name::parse(&requested).is_err() {
                return Err(code::NOT_VALID.into());
            }
            return match find_devd(txn, qsys, &requested) {
                Some((id, mut d)) => {
                    if !d.is_printer() {
                        return Err(code::NOT_VALID.into());
                    }
                    if !d.online || d.status != DevStatus::VariedOn {
                        return Err(code::BUSY.into());
                    }
                    d.status = DevStatus::Active;
                    wm::obj::write(txn, id, &d.encode()).map_err(|e| e.to_string())?;
                    Ok(requested.clone())
                }
                None if autocfg => {
                    let mut d = Devd::virtual_printer(&requested, devtype, model);
                    d.status = DevStatus::Active;
                    ensure_outq(txn, qsys, &requested)?;
                    create_devd(txn, qsys, &requested, &d)?;
                    Ok(requested.clone())
                }
                None => Err(code::NO_AUTOCFG.into()),
            };
        }
        if !requested.is_empty() {
            if qsname::Name::parse(&requested).is_err() {
                return Err(code::NOT_VALID.into());
            }
            return match find_devd(txn, qsys, &requested) {
                Some((id, mut d)) => {
                    if !d.is_display() {
                        return Err(code::NOT_VALID.into());
                    }
                    if !d.online || d.status != DevStatus::VariedOn {
                        return Err(code::BUSY.into());
                    }
                    d.status = DevStatus::SignonDisplay;
                    wm::obj::write(txn, id, &d.encode()).map_err(|e| e.to_string())?;
                    Ok(requested.clone())
                }
                None if autocfg => {
                    let mut d = Devd::virtual_display(devtype, model, kbdtype, false);
                    d.status = DevStatus::SignonDisplay;
                    create_devd(txn, qsys, &requested, &d)?;
                    Ok(requested.clone())
                }
                None => Err(code::NO_AUTOCFG.into()),
            };
        }
        // A free virtual display, else a new one.
        let lib = store::obj::library::Library::load(txn, qsys).map_err(|e| e.to_string())?;
        let mut existing = 0u32;
        let mut free = None;
        let mut taken = Vec::new();
        for (key, id) in lib
            .list(txn, Some(types::DEVD))
            .map_err(|e| e.to_string())?
        {
            let n = key.name.to_string();
            if !n.starts_with(VIRTUAL_PREFIX) {
                continue;
            }
            taken.push(n.clone());
            let b = wm::obj::read(txn, id).map_err(|e| e.to_string())?;
            let Ok(d) = Devd::decode(&b) else {
                continue;
            };
            if !d.virtual_ {
                continue;
            }
            existing += 1;
            if free.is_none()
                && d.online
                && d.status == DevStatus::VariedOn
                && d.devtype == devtype
                && d.model == model
            {
                free = Some((id, d, n));
            }
        }
        if let Some((id, mut d, n)) = free {
            d.status = DevStatus::SignonDisplay;
            wm::obj::write(txn, id, &d.encode()).map_err(|e| e.to_string())?;
            return Ok(n);
        }
        if limit.is_some_and(|l| existing >= l) {
            return Err(code::NO_AUTOCFG.into());
        }
        let mut n = 1u32;
        let name = loop {
            let candidate = virtual_name(n);
            if !taken.contains(&candidate) {
                break candidate;
            }
            n += 1;
        };
        let mut d = Devd::virtual_display(devtype, model, kbdtype, true);
        d.status = DevStatus::SignonDisplay;
        create_devd(txn, qsys, &name, &d)?;
        Ok(name)
    });
    if r.as_ref().is_err_and(|e| e == code::NO_AUTOCFG) && requested.is_empty() {
        let text = format!(
            "A session asked for a virtual display beyond QAUTOVRT({autovrt}); none was created."
        );
        to_qsysopr(system, "CPF8955", &text);
    }
    r
}

/// Set a display's status by its byte (`DevStatus`).
pub fn set_status(system: &mut System, device: &str, status: u8) -> Result<(), String> {
    let name = device.trim().to_ascii_uppercase();
    in_store(system, |txn, qsys| {
        let (id, mut d) = find_devd(txn, qsys, &name)
            .ok_or_else(|| format!("CPF2702 device {name} not found"))?;
        d.status = match status {
            1 => DevStatus::VariedOn,
            2 => DevStatus::SignonDisplay,
            3 => DevStatus::Active,
            4 => DevStatus::RecoveryPending,
            _ => DevStatus::VariedOff,
        };
        wm::obj::write(txn, id, &d.encode()).map_err(|e| e.to_string())
    })
}
