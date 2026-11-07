// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DSPLICKEY` over the license repository (`lic/slic/src/cpp/lpcmd.rs`),
//! one key to a display (`QUILICKEY`); Enter shows the next.
//! `OUTPUT(*LICKEYFILE)` writes the key file in the service.

use cmd::Resolved;
use mi::Job;

use super::{datetime, dialog, fetch, parm, show};
use crate::command::{Fail, Outcome};
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::Response;

/// `DSPLICKEY PRDID() LICTRM() FEATURE() SERIAL() OUTPUT()`.
pub fn dsplickey(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
) -> Result<String, Fail> {
    if parm(r, "OUTPUT") == "*LICKEYFILE" {
        let mut out = Outcome::default();
        return crate::command::service(job, host, r, &mut out);
    }
    let args = ["PRDID", "LICTRM", "FEATURE", "SERIAL"].map(|k| parm(r, k));
    let (_, lists) = fetch(host, "LICKEY", &args)?;
    let keys = lists
        .into_iter()
        .find(|(n, _)| n == "KEYS")
        .map(|(_, rows)| rows)
        .unwrap_or_default();
    let mut d = dialog(host, "QUILICKEY")?;
    let mut i = 0;
    while let Some(key) = keys.get(i) {
        for (k, v) in key {
            d.set(k, v);
        }
        d.set("*DATETIME", &datetime(host));
        match show(&mut d, surface, "DSPLICKEY")? {
            Response::Exit | Response::Cancel | Response::Gone => break,
            Response::Return(_) => i += 1,
            _ => {}
        }
    }
    Ok(String::new())
}
