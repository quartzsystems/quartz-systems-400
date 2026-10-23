// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The system's displays as UIM panels: each `WRK*`/`DSP*` command
//! that an interactive job runs fetches its data through the storage
//! service, fills a dialog and shows a shipped panel group with the
//! engine (`crate::display::uim`). A job without a display (batch, a
//! CL program's `OUTPUT(*PRINT)`) keeps the text path the storage
//! service answers with.

pub mod objects;
pub mod system;
pub mod work;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use cmd::Resolved;
use mi::Job;
use stg::Client;

use crate::command::{Fail, Outcome};
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::UimDialog;

thread_local! {
    /// Panel groups opened so far, by `LIB/NAME`.
    static GROUPS: RefCell<HashMap<String, Rc<uim::PanelGroup>>> = RefCell::new(HashMap::new());
}

/// A panel group through the storage service, kept once opened.
#[allow(dead_code)] // the drivers' API
pub fn group(host: &Host, lib: &str, name: &str) -> Result<Rc<uim::PanelGroup>, Fail> {
    let key = format!("{}/{}", lib.to_ascii_uppercase(), name.to_ascii_uppercase());
    if let Some(g) = GROUPS.with(|g| g.borrow().get(&key).cloned()) {
        return Ok(g);
    }
    let (payload, _) = host
        .link
        .borrow_mut()
        .with(|l| l.pnlgrp_open(name.into(), lib.into()))
        .map_err(|e| Fail::new("CPF6A98", format!("panel group {lib}/{name}: {e}")))?;
    let p = uim::payload::decode(&payload)
        .map_err(|e| Fail::new("CPF6A98", format!("panel group {lib}/{name}: {e}")))?;
    let g = Rc::new(p.group);
    GROUPS.with(|m| m.borrow_mut().insert(key, Rc::clone(&g)));
    Ok(g)
}

/// A dialog over a system panel group, with the system name set.
#[allow(dead_code)] // the drivers' API
pub fn dialog(host: &Host, name: &str) -> Result<UimDialog, Fail> {
    let g = group(host, "QSYS", name)?;
    let system = crate::display::dds::system_name(host);
    Ok(UimDialog::new(g, &system))
}

/// Whether the job shows panels: it is interactive and the command
/// did not ask for printed output.
#[must_use]
pub fn shows_panels(host: &Host, r: &Resolved) -> bool {
    if !host.interactive() {
        return false;
    }
    !r.parms.iter().any(|p| {
        p.kwd == "OUTPUT"
            && p.value
                .as_ref()
                .is_some_and(|v| v.to_string().to_ascii_uppercase().contains("*PRINT"))
    })
}

/// Run a command's display when it has a panel driver; `None` when
/// the command is not a panel, or the job shows none.
pub fn run(job: &mut Job, host: &Host, r: &Resolved, out: &mut Outcome) -> Option<Result<String, Fail>> {
    if !shows_panels(host, r) {
        return None;
    }
    let mut surface = Surface::new(host.clone());
    // --- work management (work.rs)
    if let Some(res) = work::run(job, host, &mut surface, r, out) {
        return Some(res);
    }
    // --- messages and objects (objects.rs)
    if let Some(res) = objects::run(job, host, &mut surface, r, out) {
        return Some(res);
    }
    // --- system and security (system.rs)
    if let Some(res) = system::run(job, host, &mut surface, r, out) {
        return Some(res);
    }
    None
}
