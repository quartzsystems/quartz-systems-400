// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DSPMFSINF` and `STATFS` (`QUIDSPMFS`): the attributes of the file
//! system an object is in, read through the service's IFS as the job
//! (`ifscmd::mfsinf`). `OUTPUT(*PRINT)` is the service's.

use cmd::Resolved;

use super::{dialog, show};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::Response;

/// `DSPMFSINF OBJ()`; an object that is not there is the IFS's escape.
pub fn dspmfsinf(host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let path = r
        .get("OBJ")
        .and_then(|b| b.value.as_ref())
        .map(|v| v.source().trim().trim_matches('\'').to_string())
        .unwrap_or_default();
    let c = crate::panels::ifs::client(host);
    let v = ifscmd::mfsinf::view(&c, &path).map_err(|e| crate::panels::ifs::fail(e, &path))?;
    let mut d = dialog(host, "QUIDSPMFS")?;
    d.set("OBJ", &v.object);
    d.set("FSTYPE", &v.fs_type);
    for row in &v.rows {
        d.set(row.var, &row.value);
    }
    loop {
        match show(&mut d, surface, &r.name)? {
            Response::Print => d.error("CPD9B84", &[]),
            Response::Exit | Response::Cancel | Response::Gone | Response::Return(_) => {
                return Ok(String::new());
            }
            _ => {}
        }
    }
}
