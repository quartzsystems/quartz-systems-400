// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! C6001010: load system values from the `*SYSVAL` object (or the LIC
//! record when there is no store), migrate the LIC copy into the store
//! on the first boot after the store exists, and set the ones that
//! describe this IPL. Saves write through to the object and keep the
//! IPL subset in the LIC record.

use std::fs;

use store::obj::sysval as sysval_obj;
use sysval::{HostInfo, SysVals, Value};

use crate::ipl::system::{System, now_unix};

pub(crate) fn host_info() -> HostInfo {
    // QEMU publishes no UUID unless given one, so fall back to the serials.
    HostInfo {
        product_uuid: [
            "/sys/class/dmi/id/product_uuid",
            "/sys/class/dmi/id/product_serial",
            "/sys/class/dmi/id/chassis_serial",
        ]
        .iter()
        .filter_map(|p| fs::read_to_string(p).ok())
        .map(|s| s.trim().to_string())
        .find(|s| !s.is_empty() && !s.starts_with("00000000-")),
    }
}

/// Load, or report why not and continue without system values.
pub fn load(system: &mut System) {
    let host = host_info();
    let from_store = match load_from_store(system, &host) {
        Ok(v) => v,
        Err(e) => {
            system
                .out
                .serial(&format!("slic: system values from the store: {e}\n"));
            None
        }
    };
    let mut vals = if let Some(v) = from_store {
        v
    } else {
        let Some((_, record)) = system.lic.as_ref() else {
            system
                .out
                .serial("slic: no LIC record; system values not loaded\n");
            return;
        };
        match SysVals::from_record(record, &host) {
            Ok(v) => v,
            Err(e) => {
                system.out.serial(&format!("slic: system values: {e}\n"));
                return;
            }
        }
    };
    let abnormal = if system.abnormal { "1" } else { "0" };
    for (name, value) in [("QABNORMSW", abnormal), ("QIPLSTS", "0")] {
        if let Err(e) = vals.set_system(name, Value::Char(value.into())) {
            system.out.serial(&format!("slic: {e}\n"));
        }
    }
    system.out.serial(&format!(
        "slic: {} system values, {} overridden, QSRLNBR {}, QTIMZON {}\n",
        vals.catalog().len(),
        vals.overridden().count(),
        vals.get("QSRLNBR").map(|v| v.text()).unwrap_or_default(),
        vals.get("QTIMZON").map(|v| v.text()).unwrap_or_default(),
    ));
    system.sysvals = Some(vals);
}

/// From `QSYSVAL` when the store is open. Creates the object from the
/// LIC record's values when it does not exist yet, and moves the LIC
/// record to the IPL subset.
fn load_from_store(system: &mut System, host: &HostInfo) -> Result<Option<SysVals>, String> {
    let Some(storage) = system.storage.as_mut() else {
        return Ok(None);
    };
    let Some(qsys) = storage.qsys else {
        return Ok(None);
    };
    let sysval_id = storage.store.asp().superblock().sysval;
    if !sysval_id.is_none() {
        let blob = {
            let mut t = storage.store.begin().map_err(|e| e.to_string())?;
            let b = sysval_obj::read(&mut t, sysval_id).map_err(|e| e.to_string())?;
            t.abort();
            b
        };
        return SysVals::from_blob(&blob, host)
            .map(Some)
            .map_err(|e| e.to_string());
    }
    // Migration: what the LIC record holds becomes the object.
    let vals = match system.lic.as_ref() {
        Some((_, record)) => SysVals::from_record(record, host).map_err(|e| e.to_string())?,
        None => SysVals::empty(host).map_err(|e| e.to_string())?,
    };
    let migrated = vals.overridden().count();
    let id = {
        let mut t = storage.store.begin().map_err(|e| e.to_string())?;
        let id = sysval_obj::create(&mut t, qsys, now_unix(), "QS400", &vals.to_blob())
            .map_err(|e| e.to_string())?;
        t.commit().map_err(|e| e.to_string())?;
        id
    };
    storage.store.asp_superblock_set_sysval(id);
    storage.store.checkpoint().map_err(|e| e.to_string())?;
    if let Some((part, record)) = system.lic.as_mut() {
        vals.store_ipl(record).map_err(|e| e.to_string())?;
        part.write(record).map_err(|e| e.to_string())?;
    }
    system.out.serial(&format!(
        "slic: {migrated} system value override(s) migrated from the LIC partition to QSYSVAL\n"
    ));
    Ok(Some(vals))
}

/// Persist the current values: the object when the store is open, and
/// the IPL subset (or everything, without a store) in the LIC record.
pub fn save(system: &mut System) -> Result<(), String> {
    let System {
        sysvals,
        storage,
        lic,
        ..
    } = system;
    let Some(vals) = sysvals.as_ref() else {
        return Err("system values not loaded".into());
    };
    match storage.as_mut() {
        Some(s) => {
            let id = s.store.asp().superblock().sysval;
            if id.is_none() {
                return Err("QSYSVAL does not exist".into());
            }
            let mut t = s.store.begin().map_err(|e| e.to_string())?;
            sysval_obj::write(&mut t, id, &vals.to_blob()).map_err(|e| e.to_string())?;
            t.commit().map_err(|e| e.to_string())?;
            if let Some((part, record)) = lic.as_mut() {
                vals.store_ipl(record).map_err(|e| e.to_string())?;
                part.write(record).map_err(|e| e.to_string())?;
            }
            Ok(())
        }
        None => match lic.as_mut() {
            Some((part, record)) => vals.save(part, record).map_err(|e| e.to_string()),
            None => Err("LIC partition not attached".into()),
        },
    }
}
