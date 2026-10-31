// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The system's displays as UIM panels: each `WRK*`/`DSP*` command
//! that an interactive job runs fetches its data through the storage
//! service, fills a dialog and shows a shipped panel group with the
//! engine (`crate::display::uim`). A job without a display (batch, a
//! CL program's `OUTPUT(*PRINT)`) keeps the text path the storage
//! service answers with. A driver whose read fails ends the command
//! with the service's message or the function check
//! ([`crate::command::Fail::read`]); it never hands the command back to
//! the text path.

/// A read over the link in a driver returning
/// `Option<Result<String, Fail>>`: its value, or the command's escape.
macro_rules! read {
    ($cmd:expr, $e:expr) => {
        match $e {
            Ok(v) => v,
            Err(e) => return Some(Err($crate::command::Fail::read($cmd, &e))),
        }
    };
}

pub mod backup;
pub mod certs;
pub mod ifs;
pub mod licpgm;
pub mod objects;
pub mod system;
pub mod system2;
pub mod tcp;
pub mod work;
pub mod work2;

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
    /// The commands whose panels are showing, the innermost last.
    static RUNNING: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// A command's panels showing; the command before it again on drop.
struct Running;

impl Running {
    fn enter(name: &str) -> Self {
        RUNNING.with(|r| r.borrow_mut().push(name.to_string()));
        Self
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        RUNNING.with(|r| r.borrow_mut().pop());
    }
}

thread_local! {
    /// Failure details a message line showed as a function check,
    /// for the job log.
    static DETAILS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// A failure's text for a panel's message line: the service's message
/// when `e` starts with one, else the function check's sentence, the
/// detail kept for the job log ([`take_details`]).
pub fn line(e: &str) -> String {
    if !sys_cpps::check::is_internal(e) {
        return e.trim().to_string();
    }
    DETAILS.with(|d| d.borrow_mut().push(e.to_string()));
    let name = RUNNING.with(|r| r.borrow().last().cloned().unwrap_or_else(|| "*N".into()));
    sys_cpps::check::text(&name)
}

/// The details [`line`] kept since the last call.
pub fn take_details() -> Vec<String> {
    DETAILS.with(|d| std::mem::take(&mut *d.borrow_mut()))
}

/// A driver's failure: the service's message when `e` starts with
/// one, else the function check of the command whose panels show,
/// `e` its detail for the job log.
pub fn failed(e: impl AsRef<str>) -> Fail {
    let name = RUNNING.with(|r| r.borrow().last().cloned().unwrap_or_else(|| "*N".into()));
    Fail::read(&name, e.as_ref())
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
        .map_err(|e| crate::panels::failed(format!("panel group {lib}/{name}: {e}")))?;
    let p = uim::payload::decode(&payload)
        .map_err(|e| crate::panels::failed(format!("panel group {lib}/{name}: {e}")))?;
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
    // Only a job that owns a 5250 device: the serial service console
    // (QSVCJOB) is interactive but answers in text.
    if !host.interactive() || host.display.borrow().is_none() {
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
    let _running = Running::enter(&r.name);
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
    // --- panels batch 2: work, spool, journals (work2.rs)
    if let Some(res) = work2::run(job, host, &mut surface, r, out) {
        return Some(res);
    }
    // --- programs, config, security, editors (system2.rs)
    if let Some(res) = system2::run(job, host, &mut surface, r, out) {
        return Some(res);
    }
    // --- TCP/IP configuration (tcp.rs)
    if let Some(res) = tcp::run(job, host, &mut surface, r, out) {
        return Some(res);
    }
    // --- the certificate displays (certs.rs)
    if let Some(res) = certs::run(job, host, &mut surface, r, out) {
        return Some(res);
    }
    // --- the licensed program displays (licpgm.rs)
    if let Some(res) = licpgm::run(job, host, &mut surface, r, out) {
        return Some(res);
    }
    // --- the backup display (backup.rs)
    if let Some(res) = backup::run(job, host, &mut surface, r, out) {
        return Some(res);
    }
    // --- the IFS displays (ifs.rs)
    if let Some(res) = ifs::run(job, host, &mut surface, r, out) {
        return Some(res);
    }
    None
}

/// Whether the job shows panels at all: interactive, with a 5250 device.
#[must_use]
pub fn shows_host_panels(host: &Host) -> bool {
    host.interactive() && host.display.borrow().is_some()
}
