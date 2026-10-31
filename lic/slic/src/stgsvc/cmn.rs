// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Communications objects in the storage service. The status of every
//! line, network controller and device lives here (`cmncfg::State`),
//! behind a lock a thread shares that reads the kernel's link events
//! (`kernel::follow`); vary requests drive netlink, automatic
//! configuration makes the `*NET` controller and device a line needs,
//! and the messages the status machine answers go to the line's queue
//! from the serve loop's tick. `docs/TCPIP.md`, "Communications
//! objects", has the design.

mod kernel;
mod objects;
mod panel;

pub(crate) use panel::{cfg_status, cfgsts_rows, cfgsts_text, panel};

use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::Duration;

use cmncfg::{Effect, Kind, Lind, State, msgs};
use wm::obj::msgq::{MsgKind, QMsg};

use crate::ipl::system::System;

/// What the service's requests and the link events thread share.
#[derive(Debug, Default)]
pub(crate) struct Shared {
    /// Lines, controllers and devices with their status.
    pub(crate) state: State,
    /// Messages the events thread produced, for the service to send.
    pub(crate) pending: Vec<Effect>,
}

fn shared() -> &'static Arc<Mutex<Shared>> {
    static S: OnceLock<Arc<Mutex<Shared>>> = OnceLock::new();
    S.get_or_init(|| Arc::new(Mutex::new(Shared::default())))
}

/// The shared state, locked.
pub(crate) fn lock() -> MutexGuard<'static, Shared> {
    shared().lock().unwrap_or_else(PoisonError::into_inner)
}

/// `QCMNRCYLMT` (count, minutes) and the configuration message queue
/// (`QSYS/QSYSOPR`; this release has no `QCFGMSGQ`).
fn defaults(system: &System) -> ((u16, u16), String) {
    let text = system
        .sysvals
        .as_ref()
        .and_then(|v| v.get("QCMNRCYLMT").ok())
        .map(|v| v.text())
        .unwrap_or_default();
    let mut n = text
        .split_whitespace()
        .map(|w| w.parse::<u16>().unwrap_or(0));
    let rcy = (n.next().unwrap_or(0), n.next().unwrap_or(0));
    (rcy, "QSYS/QSYSOPR".into())
}

/// Line `name` as TCP/IP sees it: varied on, and over which interface.
pub(crate) fn line_info(name: &str) -> Option<tcpcfg::line::LineInfo> {
    lock().state.line(name).map(|l| tcpcfg::line::LineInfo {
        on: l.status.is_on(),
        ifname: l.ifname.clone(),
    })
}

/// Put line `name` described by `lind` in the state (or give it its
/// new description).
pub(crate) fn describe(system: &System, name: &str, lind: &Lind) {
    let (rcy, q) = defaults(system);
    lock().state.add_line(name, lind.line_config(rcy, &q));
}

/// At service start: the lines and what hangs off them into the state,
/// the kernel's boot-time bond removed, the link events followed and
/// the lines described `ONLINE(*YES)` varied on.
pub(crate) fn start(system: &mut System) {
    match objects::load(system) {
        Ok(all) => {
            for (name, lind) in &all.lines {
                describe(system, name, lind);
            }
            let mut sh = lock();
            for (ctl, line) in &all.ctls {
                if let Some(l) = sh.state.line_mut(line) {
                    l.attach_ctl(ctl);
                }
            }
            for (dev, ctl) in &all.devs {
                let line = sh.state.line_of_ctl(ctl).map(|l| l.name.clone());
                if let Some(l) = line.and_then(|n| sh.state.line_mut(&n)) {
                    l.attach_dev(ctl, dev);
                }
            }
        }
        Err(e) => eprintln!("qsstg: line descriptions: {e}"),
    }
    kernel::remove_boot_bond();
    kernel::follow(Arc::clone(shared()));
    vary_online_lines(system);
}

/// Vary on, with what hangs off them, the lines described
/// `ONLINE(*YES)`, without waiting for their carrier.
pub(crate) fn vary_online_lines(system: &mut System) {
    let names = lock().state.online_lines();
    for n in names {
        if let Err(e) = vary_line(system, &n, true, true, Some(0)) {
            eprintln!("qsstg: vary on {n} at IPL: {e}");
        }
    }
}

/// Send what the events thread left.
pub(crate) fn tick(system: &mut System) {
    let effects = std::mem::take(&mut lock().pending);
    send(system, effects);
}

/// The messages of `effects` to their queues.
fn send(system: &mut System, effects: Vec<Effect>) {
    for e in effects {
        let Effect::Message {
            msgid,
            msgq,
            data,
            inquiry,
            ..
        } = e;
        let severity = match msgid.as_str() {
            msgs::LINE_FAILED => 99,
            msgs::RECOVERY_STARTED => 40,
            _ => 0,
        };
        let m = QMsg {
            kind: if inquiry { MsgKind::Inq } else { MsgKind::Info },
            severity,
            text: super::hist::message_text(&msgid, &data),
            msgid,
            msgf: ("QCPFMSG".into(), "QSYS".into()),
            data,
            job: ("QSYSARB".into(), "QSYS".into(), 0),
            from_pgm: "QSYSARB".into(),
            dft: if inquiry { "C".into() } else { String::new() },
            ..QMsg::default()
        };
        let (lib, name) = msgq.split_once('/').unwrap_or(("QSYS", &msgq));
        if super::msgs::send(system, lib, name, m.clone()).is_err()
            && (lib, name) != ("QSYS", "QSYSOPR")
        {
            let _ = super::msgs::send(system, "QSYS", "QSYSOPR", m);
        }
    }
}

/// A completion or diagnostic message of a vary: identifier and the
/// object it names.
pub(crate) type Note = (&'static str, String);

/// Vary line `name` on or off. `RANGE(*NET)` takes its network
/// controller and device with it; `vrywait` (seconds, `None` for the
/// line's own `VRYWAIT`) is how long a vary on waits for the carrier.
/// The completion messages, or the refusal (`CPDnnnn` or `CPFnnnn`
/// first).
pub(crate) fn vary_line(
    system: &mut System,
    name: &str,
    on: bool,
    range_net: bool,
    vrywait: Option<u16>,
) -> Result<Vec<Note>, String> {
    let name = name.to_ascii_uppercase();
    let lind = objects::read_lind(system, &name)?;
    if !on {
        let ifname = lock().state.vary_off(&name, range_net)?;
        if !ifname.is_empty() {
            kernel::bring_down(&ifname, lind.is_aggregate())
                .map_err(|e| format!("{} line {name}: {e}", msgs::LINE_NOT_VARIED_ON))?;
        }
        return Ok(vec![(msgs::LINE_VARIED_OFF, name)]);
    }
    describe(system, &name, &lind);
    lock().state.vary_on_begin(&name)?;
    let fail = |e: String| -> String {
        let r = lock().state.vary_on_done(&name, "", Err(e), range_net);
        r.err().unwrap_or_default()
    };
    let mut notes = Vec::new();
    let net = objects::ensure_net(system, &name, &|n| lock().state.attached(n)).map_err(&fail)?;
    if net.made_ctl {
        notes.push((msgs::AUTO_CTL, net.ctl.clone()));
    }
    if net.made_dev {
        notes.push((msgs::AUTO_DEV, net.dev.clone()));
    }
    if let Some(l) = lock().state.line_mut(&name) {
        l.attach_dev(&net.ctl, &net.dev);
    }
    let ports = kernel::ports(system);
    let plan = cmncfg::link::plan(&lind, &name, |r| {
        ports.iter().find(|(n, _)| n == r).map(|(_, i)| i.clone())
    })
    .map_err(&fail)?;
    let wait = Duration::from_secs(u64::from(vrywait.unwrap_or(lind.vrywait)));
    kernel::bring_up(&plan, wait).map_err(&fail)?;
    // The carrier read under the lock: an event the thread saw while
    // the line was pending is not lost.
    let effects = {
        let mut sh = lock();
        let carrier = kernel::has_carrier(&plan.ifname);
        sh.state
            .vary_on_done(&name, &plan.ifname, Ok(carrier), range_net)?
    };
    send(system, effects);
    notes.push((msgs::LINE_VARIED_ON, name));
    Ok(notes)
}

/// Vary a controller on or off: a network controller through its
/// line's status, any other in its description.
pub(crate) fn vary_ctl(
    system: &mut System,
    name: &str,
    on: bool,
    range_net: bool,
) -> Result<Vec<Note>, String> {
    let name = name.to_ascii_uppercase();
    let network = lock().state.line_of_ctl(&name).is_some();
    if network {
        lock().state.vary_ctl(&name, on, range_net)?;
    } else {
        objects::vary_plain_ctl(system, &name, on)?;
    }
    Ok(vec![(
        if on {
            msgs::CTL_VARIED_ON
        } else {
            msgs::CTL_VARIED_OFF
        },
        name,
    )])
}

/// Vary a device on or off: a network device through its line's
/// status, any other in its description.
pub(crate) fn vary_dev(system: &mut System, name: &str, on: bool) -> Result<Vec<Note>, String> {
    let name = name.to_ascii_uppercase();
    let network = lock().state.line_of_dev(&name).is_some();
    if network {
        lock().state.vary_dev(&name, on)?;
    } else if super::media::devices::read(system, &name).is_ok() {
        return super::media::devices::vary(system, &name, on);
    } else {
        objects::vary_plain_dev(system, &name, on)?;
    }
    Ok(vec![(
        if on {
            msgs::DEV_VARIED_ON
        } else {
            msgs::DEV_VARIED_OFF
        },
        name,
    )])
}

/// Create line `name`; it starts varied off.
pub(crate) fn create_line(system: &mut System, name: &str, lind: &Lind) -> Result<(), String> {
    objects::create_lind(system, name, lind)?;
    describe(system, name, lind);
    Ok(())
}

/// The parameters `CHGLINETH` may change only while the line is
/// varied off.
const OFF_ONLY: &[&str] = &["RSRCNAME", "ADPTADR", "MAXFRAME", "AGGRSCL", "AGGPCY"];

/// Change line `name` with `change` (the parameters it was given); the
/// line must be varied off for [`OFF_ONLY`] parameters.
pub(crate) fn change_line(
    system: &mut System,
    name: &str,
    given: &[&str],
    change: impl FnOnce(&mut Lind) -> Result<(), String>,
) -> Result<(), String> {
    let name = name.to_ascii_uppercase();
    let mut lind = objects::read_lind(system, &name)?;
    let on = lock()
        .state
        .status_of(Kind::Line, &name)
        .is_some_and(cmncfg::Status::is_on);
    if on && given.iter().any(|g| OFF_ONLY.contains(g)) {
        return Err(format!("{} line {name} is varied on", msgs::MUST_VARY_OFF));
    }
    change(&mut lind)?;
    objects::write_lind(system, &name, &lind)?;
    describe(system, &name, &lind);
    Ok(())
}

/// Delete line `name` with its automatic controller and device; what
/// was deleted, for the completion messages.
pub(crate) fn delete_line(system: &mut System, name: &str) -> Result<Vec<Note>, String> {
    let name = name.to_ascii_uppercase();
    if lock()
        .state
        .status_of(Kind::Line, &name)
        .is_some_and(cmncfg::Status::is_on)
    {
        return Err(format!(
            "{} line {name} is varied on",
            msgs::DELETE_VARIED_ON
        ));
    }
    // The port goes back to the IPL's network unless another line has it.
    let port = objects::read_lind(system, &name)
        .ok()
        .filter(|l| !l.is_aggregate())
        .map(|l| l.rsrcname);
    let gone = objects::delete_lind(system, &name)?;
    lock().state.remove_line(&name);
    if let Some(rsrc) = port {
        let shared = objects::lines(system)?.iter().any(|(_, l)| l.rsrcname == rsrc);
        let ifname = kernel::ports(system).into_iter().find(|(r, _)| *r == rsrc).map(|(_, i)| i);
        if let (false, Some(i)) = (shared, ifname)
            && let Err(e) = kernel::release(&i)
        {
            eprintln!("qsstg: DLTLIND {name}: {e}");
        }
    }
    let mut notes: Vec<Note> = gone
        .into_iter()
        .map(|(n, ctl)| {
            (
                if ctl {
                    msgs::AUTO_CTL_DELETED
                } else {
                    msgs::AUTO_DEV_DELETED
                },
                n,
            )
        })
        .collect();
    notes.push((msgs::LINE_DELETED, name));
    Ok(notes)
}

/// The line descriptions whose names pass `wanted`, in name order.
pub(crate) fn line_names(
    system: &mut System,
    wanted: &dyn Fn(&str) -> bool,
) -> Result<Vec<String>, String> {
    Ok(objects::lines(system)?
        .into_iter()
        .map(|(n, _)| n)
        .filter(|n| wanted(n))
        .collect())
}

/// `DSPLIND` of line `name` as text, `CPF2601` when there is none.
pub(crate) fn read_and_show(system: &mut System, name: &str) -> Result<String, String> {
    let l = objects::read_lind(system, name)?;
    Ok(panel::dsplind_text(system, name, &l))
}

/// Every line, controller or device name `WRKCFGSTS` knows of `kind`,
/// for a generic `VRYCFG`.
pub(crate) fn cfgsts_names(system: &mut System, kind: Kind) -> Vec<String> {
    cfgsts_rows(system, kind, "*ALL", false, "*ALL")
        .map(|rows| rows.into_iter().map(|r| r.name).collect())
        .unwrap_or_default()
}
