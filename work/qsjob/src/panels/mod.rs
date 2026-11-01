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
pub mod info;
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

/// A failure for a panel's message line: the service's message when
/// `e` starts with one (its identifier, in the service's wording),
/// else the function check, the detail kept for the job log
/// ([`take_details`]).
pub fn line(e: &str) -> uim::engine::LineMsg {
    if !sys_cpps::check::is_internal(e) {
        let t = e.trim();
        let mut m = uim::engine::LineMsg::error(&t[..7], &[]);
        let rest = t[7..].trim_start_matches(':').trim();
        if !rest.is_empty() {
            m.text = Some(rest.to_string());
        }
        return m;
    }
    DETAILS.with(|d| d.borrow_mut().push(e.to_string()));
    let name = RUNNING.with(|r| r.borrow().last().cloned().unwrap_or_else(|| "*N".into()));
    uim::engine::LineMsg::error(sys_cpps::check::ESCAPE, &[&sys_cpps::check::text(&name)])
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

/// A service answer's line for the message line: `MSGID` then each
/// value after a tab, or `MSGID text` in the service's wording; `None`
/// for a line that is no message.
pub fn said(line: &str) -> Option<uim::engine::LineMsg> {
    let t = line.trim_end();
    let id = t.get(..7)?;
    if sys_cpps::check::is_internal(id) {
        return None;
    }
    let rest = &t[7..];
    if let Some(vals) = rest.strip_prefix('\t') {
        let vals: Vec<&str> = vals.split('\t').collect();
        return Some(uim::engine::LineMsg::info(id, &vals));
    }
    let mut m = uim::engine::LineMsg::info(id, &[]);
    if !rest.trim().is_empty() {
        m.text = Some(rest.trim().to_string());
    }
    Some(m)
}

/// Run a command from a panel: what it leaves on the message line
/// ([`crate::display::msgline::run`]); nothing when its prompt was
/// cancelled.
pub fn exec(job: &mut Job, host: &Host, text: &str) -> Option<uim::engine::LineMsg> {
    let (out, m) = crate::display::msgline::run(job, host, text);
    if out.escape.as_ref().is_some_and(|f| f.msgid == "CPF6801") {
        return None;
    }
    m
}

/// Panel data `what` from the storage service (request 270); a failed
/// read is command `cmd`'s escape ([`Fail::read`]).
pub fn panel_data(host: &Host, cmd: &str, what: &str, args: &[&str]) -> Result<stg::proto::Sys2Data, Fail> {
    let args: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
    host.link
        .borrow_mut()
        .with(|l| l.sys2_panel(what.into(), args))
        .map_err(|e| Fail::read(cmd, &e))
}

/// The entries of list `name` in panel data, empty when it has none.
pub fn data_list(data: &stg::proto::Sys2Data, name: &str) -> Vec<Vec<(String, String)>> {
    data.1
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, rows)| rows.clone())
        .unwrap_or_default()
}

/// A variable of panel data, blank when missing.
pub fn data_var(data: &stg::proto::Sys2Data, name: &str) -> String {
    data.0
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}

/// An option's command with the parameters typed on the command line
/// when the options were (`2` and `OUTQ(PRT01)` on `WRKSPLF`).
#[must_use]
pub fn with_parms(d: &UimDialog, cmd: String) -> String {
    let parms = d.get(uim::engine::OPTION_PARMS_VAR);
    let parms = parms.trim();
    if parms.is_empty() { cmd } else { format!("{} {parms}", cmd.trim_end()) }
}

/// The command of an option typed on a list's extended action entry,
/// with the entry's values: a parameter the entry left blank (`KWD()`,
/// `KWD('')`, `KWD(/)`) is left out for the prompt to ask, and a name typed
/// without its library goes to `*CURLIB`, as a create command's
/// default.
#[must_use]
pub fn ext_command(d: &UimDialog, c: &uim::engine::Choice, text: &str) -> String {
    let full = d.substitute(text, Some((&c.list, c.row)));
    let mut out = String::new();
    for word in full.split(' ') {
        if let Some(open) = word.find('(')
            && word.ends_with(')')
        {
            let value = &word[open + 1..word.len() - 1];
            if value.is_empty() || value == "/" || value == "''" {
                continue;
            }
            if let Some(name) = value.strip_prefix('/') {
                out.push_str(&word[..open]);
                out.push_str("(*CURLIB/");
                out.push_str(name);
                out.push_str(") ");
                continue;
            }
        }
        out.push_str(word);
        out.push(' ');
    }
    out.trim_end().to_string()
}

/// Whether the job shows panels: it is interactive and the command
/// did not ask for printed output or an outfile.
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
                .is_some_and(|v| {
                    let v = v.to_string().to_ascii_uppercase();
                    v.contains("*PRINT") || v.contains("*OUTFILE")
                })
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
    // --- the displays the CPP audit gave panels (info.rs)
    if let Some(res) = info::run(job, host, &mut surface, r, out) {
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

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use uim::engine::Choice;
    use uim::render::EXT_ENTRY;

    use super::ext_command;
    use crate::display::uim::UimDialog;

    fn entry(values: &[(&str, &str)]) -> (UimDialog, Choice) {
        let c = uim::compile(":PNLGRP.\n:EPNLGRP.\n", &uim::NoImbeds, &uim::NoImports, &uim::CompileOpts::default());
        let mut d = UimDialog::new(Rc::new(c.group.expect("a group")), "QS400");
        for (k, v) in values {
            d.list_mut("L").set_ext(k, v);
        }
        let c = Choice {
            list: "L".into(),
            row: EXT_ENTRY,
            option: "1".into(),
            action: None,
        };
        (d, c)
    }

    #[test]
    fn add_entry_commands() {
        let text = "?CRTCLS CLS(&LIB./&CLS.) TEXT(&TEXT.)";
        let (d, c) = entry(&[("CLS", "MINE"), ("LIB", "QGPL")]);
        assert_eq!(ext_command(&d, &c, text), "?CRTCLS CLS(QGPL/MINE)");
        let (d, c) = entry(&[("CLS", "MINE")]);
        assert_eq!(ext_command(&d, &c, text), "?CRTCLS CLS(*CURLIB/MINE)");
        let (d, c) = entry(&[]);
        assert_eq!(ext_command(&d, &c, text), "?CRTCLS");
        // WRKOPTVOL's option 1: ADDOPTCTG names no volume, so the one
        // typed is not passed (7.1's prompt shows none).
        let (d, c) = entry(&[("VOL", "VOL01")]);
        assert_eq!(ext_command(&d, &c, "?ADDOPTCTG"), "?ADDOPTCTG");
        let (d, c) = entry(&[]);
        assert_eq!(ext_command(&d, &c, "?ADDTCPHTE INTNETADR('&HTEADR.')"), "?ADDTCPHTE");
    }
}
