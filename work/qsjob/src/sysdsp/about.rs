// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! About QS/400 (panel group `QUIABOUT`), from the Information
//! Assistant's F14 through program `QSYS/QUIABOUT`: the operating
//! system's product and release as `DSPSFWRSC` lists them, the build,
//! and the serial number, model and processor feature.

use stg::Client;

use crate::console::host::Host;
use crate::display::surface::Surface;

/// The release field of a `DSPSFWRSC` row.
const RELEASE: usize = 6;

/// A system value as text.
fn sysval(host: &Host, name: &str) -> String {
    host.link
        .borrow_mut()
        .with(|l| l.sysval(name.into()))
        .map(|v| ccsid::from_ebcdic(&v.data).trim().to_string())
        .unwrap_or_default()
}

/// The panel's values.
#[must_use]
pub fn values(host: &Host) -> Vec<(&'static str, String)> {
    let rows = host.link.borrow_mut().with(Client::sfwrsc_panel);
    let os = rows
        .iter()
        .find(|r| r.first().is_some_and(|id| id == release::PRODUCT_ID))
        .or_else(|| rows.first());
    let rel = os
        .and_then(|r| r.get(RELEASE).cloned())
        .unwrap_or_else(|| release::RELEASE.to_string());
    vec![
        ("PRDTEXT", release::PRODUCT_TEXT.to_string()),
        ("PRODUCT", release::PRODUCT_ID.to_string()),
        ("RELEASE", rel),
        ("BUILD", release::build_id().to_string()),
        ("SYSNAME", sysval(host, "QSYSNAME")),
        ("SERIAL", sysval(host, "QSRLNBR")),
        ("MODEL", sysval(host, "QMODEL")),
        ("PRCFEAT", sysval(host, "QPRCFEAT")),
    ]
}

/// Show About QS/400 until it is left.
pub fn show(host: &Host) {
    let Ok(mut d) = crate::panels::dialog(host, "QUIABOUT") else {
        return;
    };
    for (k, v) in values(host) {
        d.set(k, &v);
    }
    let mut surface = Surface::new(host.clone());
    let _ = d.show(&mut surface, "ABOUT");
}
