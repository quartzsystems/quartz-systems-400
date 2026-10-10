// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! C6001010: load system values from the LIC record and set the ones that
//! describe this IPL.

use std::fs;

use sysval::{HostInfo, SysVals, Value};

use crate::ipl::system::System;

/// Load, or report why not and continue without system values.
pub fn load(system: &mut System) {
    let Some((_, record)) = system.lic.as_ref() else {
        system
            .out
            .serial("slic: no LIC record; system values not loaded\n");
        return;
    };
    // QEMU publishes no UUID unless given one, so fall back to the serials.
    let host = HostInfo {
        product_uuid: [
            "/sys/class/dmi/id/product_uuid",
            "/sys/class/dmi/id/product_serial",
            "/sys/class/dmi/id/chassis_serial",
        ]
        .iter()
        .filter_map(|p| fs::read_to_string(p).ok())
        .map(|s| s.trim().to_string())
        .find(|s| !s.is_empty() && !s.starts_with("00000000-")),
    };
    let mut vals = match SysVals::from_record(record, &host) {
        Ok(v) => v,
        Err(e) => {
            system.out.serial(&format!("slic: system values: {e}\n"));
            return;
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
