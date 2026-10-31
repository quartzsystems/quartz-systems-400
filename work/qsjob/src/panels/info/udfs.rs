// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DSPUDFS` (`QUIDSPUDFS`): the file system read through the service's
//! IFS (`ifscmd::display::udfs`), its mounts as a list.

use cmd::Resolved;

use super::{dialog, fill, show};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::Response;

/// `DSPUDFS UDFS()`; a missing one is the IFS's escape (`CPFA0A9`).
pub fn dspudfs(host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let path = r
        .get("UDFS")
        .and_then(|b| b.value.as_ref())
        .map(|v| v.source().trim().trim_matches('\'').to_string())
        .unwrap_or_default();
    let c = crate::panels::ifs::client(host);
    let v = ifscmd::display::udfs(&c, &path).map_err(|e| crate::panels::ifs::fail(e, &path))?;
    let mut d = dialog(host, "QUIDSPUDFS")?;
    for (k, val) in &v.vars {
        d.set(k, val);
    }
    let rows = v.mounts.into_iter().map(|m| vec![("MNTDIR".to_string(), m)]).collect();
    fill(&mut d, "MOUNTS", rows);
    loop {
        match show(&mut d, surface, "DSPUDFS")? {
            Response::Print => d.error("CPD9B84", &[]),
            Response::Exit | Response::Cancel | Response::Gone | Response::Return(_) => return Ok(String::new()),
            _ => {}
        }
    }
}
