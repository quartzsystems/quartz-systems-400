// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Display Attributes (`QUIWRKLNK` `DSPATRD` for a directory, `DSPATRF`
//! otherwise): `ifscmd::display::attributes` as the panel's variables,
//! paged by the engine; Enter, F3 and F12 leave.

use ifs::client::IfsClient;
use sec::profile::spcaut::SpecialAuth;
use stg::Client;

use super::fail;
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::Response;

/// Show `path`'s attributes until the operator leaves.
pub fn show(host: &Host, surface: &mut Surface, c: &dyn IfsClient, path: &str) -> Result<(), Fail> {
    let me = host.link.borrow_mut().with(Client::whoami);
    let audit = me.has(SpecialAuth::AUDIT) || me.has(SpecialAuth::ALLOBJ);
    let vars = ifscmd::display::attributes(c, path, audit).map_err(|e| fail(e, path))?;
    let mut d = crate::panels::dialog(host, "QUIWRKLNK")?;
    let mut dir = false;
    for (k, v) in &vars {
        match *k {
            // The panel's SYSTEM is the system name.
            "SYSTEM" => d.set("SYSON", v),
            "DIRFMT" => {
                dir = true;
                d.set(k, v);
            }
            _ => d.set(k, v),
        }
    }
    let panel = if dir { "DSPATRD" } else { "DSPATRF" };
    loop {
        match super::show(&mut d, surface, panel)? {
            Response::Return(22) => {
                d.message = Some((path.to_string(), false));
            }
            _ => return Ok(()),
        }
    }
}
