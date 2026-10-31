// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Licensed programs in the service (`docs/LICPGM.md` §9): the catalog
//! seeded every IPL, the product store over the open store, the system
//! as the product functions see it, and the license public key.

use std::cell::RefCell;

use licpgm::{LoadType, OptionNo, ProductStore, SystemInfo};
use lpstore::{Cell, Products};
use store::obj::ObjectId;

use crate::ipl::system::{System, now_unix};

/// The build's public key (32 bytes; the first is the license key),
/// the same file the save engine trusts signatures with.
pub const LICENSE_KEY_PATH: &str = "/qs400/keys/system.pub";

/// The processor group license keys are bound to. QS/400 has one.
pub const PROCESSOR_GROUP: &str = "P05";

/// Seed the catalog's products; whether any was made.
pub fn seed(system: &mut System) -> bool {
    let Some(s) = system.storage.as_mut() else {
        return false;
    };
    let Some(qsys) = s.qsys else {
        return false;
    };
    match sec::guard::as_system(|| lpstore::seed(&mut s.store, qsys, now_unix())) {
        Ok(made) => made,
        Err(e) => {
            eprintln!("qsstg: licensed programs: {e}");
            false
        }
    }
}

/// Run `f` with the product store (and the shared store for the save
/// engine), as the system: commands check their own authority first.
pub fn with<T>(
    system: &mut System,
    f: impl FnOnce(&mut Products<'_, '_>, &Cell<'_>, ObjectId) -> Result<T, String>,
) -> Result<T, String> {
    let s = system.storage.as_mut().ok_or("CPF3700 object store not open")?;
    let qsys = s.qsys.ok_or("CPF3700 object store not initialized")?;
    let cell = RefCell::new(&mut s.store);
    sec::guard::as_system(|| {
        let mut p = Products::new(&cell, qsys, lpstore::stamp(now_unix()));
        f(&mut p, &cell, qsys)
    })
}

/// The system as the product functions see it.
pub fn system_info(system: &System) -> SystemInfo {
    let mut serial = crate::stgsvc::security::sysval_text(system, "QSRLNBR");
    serial.retain(|c| c.is_ascii_alphanumeric());
    if serial.is_empty() {
        serial = "QS40001".into();
    }
    SystemInfo::current(&serial.to_ascii_uppercase(), PROCESSOR_GROUP)
}

/// The Quartz license public key the image carries, if any.
pub fn quartz_key() -> Option<[u8; 32]> {
    let b = std::fs::read(LICENSE_KEY_PATH).ok()?;
    b.get(..32)?.try_into().ok()
}


/// Whether a product option's code is installed (state 90 or another
/// installed state). A store without product records counts as having
/// everything, so tests without a seeded store keep working.
pub fn installed(system: &mut System, product: &str, option: OptionNo) -> bool {
    with(system, |p, _, _| {
        let loads = p.loads();
        if loads.is_empty() {
            return Ok(true);
        }
        Ok(loads.iter().any(|(_, l)| {
            l.key.product.as_str() == product
                && l.key.option == option
                && l.key.load_type == LoadType::Code
                && l.state.is_installed()
        }))
    })
    .unwrap_or(true)
}
