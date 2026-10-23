// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Secondary and group jobs from the job's side. The TN5250 session
//! holding the display moves it between the jobs at it
//! (`FromJob::SecondaryJob`, `GroupTransfer`, `GroupStart`); the job
//! that gave the display away waits until a `ToJob::DeviceReset` gives
//! it back. Group membership and `*GDA` are the storage service's
//! (requests 200-205).

use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

use stg::Client;
use wm::ctl::{FromJob, ToJob};

use super::keys::{self, Act};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::Response;

/// Give the display away with `m` and wait until it comes back.
fn hand_over(host: &Host, m: &FromJob) -> Act {
    if host.ctl.borrow_mut().send(m).is_err() {
        return Act::Gone;
    }
    wait_back(host)
}

/// Wait until the display is the job's again. Records typed at the
/// other job never reach this one; anything else waits for later.
pub fn wait_back(host: &Host) -> Act {
    let mut kept = Vec::new();
    let act = loop {
        match host.inbox.recv_timeout(Duration::from_secs(5)) {
            Ok(ToJob::DeviceReset { device }) => {
                if let Some(d) = host.display.borrow_mut().as_mut() {
                    d.reset(device);
                }
                break Act::Back;
            }
            Ok(ToJob::End { .. } | ToJob::DeviceGone) | Err(RecvTimeoutError::Disconnected) => {
                break Act::Gone;
            }
            Ok(ToJob::Record { .. }) | Err(RecvTimeoutError::Timeout) => {}
            Ok(m) => kept.push(m),
        }
    };
    host.pending.borrow_mut().extend(kept);
    act
}

/// System Request option 1 and `TFRSECJOB`: the other job at the
/// display, or a sign-on for one.
pub fn secondary(host: &Host) -> Act {
    hand_over(host, &FromJob::SecondaryJob)
}

/// System Request option 80 and `DSCJOB`: the sign-on display comes
/// back; the job waits for its user to sign on again.
pub fn disconnect(host: &Host) -> Act {
    hand_over(host, &FromJob::Disconnect)
}

/// The job's group: number, own name, members.
fn info(host: &Host) -> Option<stg::proto::GroupInfo> {
    host.link.borrow_mut().with(Client::grp_info)
}

/// Transfer to a group job: `*PRV`, `*SELECT` or a name; a job the
/// group does not have yet starts with `program`.
pub fn transfer_to(host: &Host, which: &str, program: &str) -> Result<Act, String> {
    let (group, me, members) =
        info(host).ok_or_else(|| "CPF1311 The job is not a group job.".to_string())?;
    match which {
        "*PRV" => Ok(hand_over(host, &FromJob::GroupTransfer { job: 0 })),
        "*SELECT" => {
            let others: Vec<_> = members.into_iter().filter(|m| m.0 != me).collect();
            if others.is_empty() {
                return Err("No other job is in the group.".into());
            }
            match select(host, &me, &others) {
                Some(n) => Ok(hand_over(host, &FromJob::GroupTransfer { job: n })),
                None if keys::unwinding() => Ok(Act::Gone),
                None => Ok(Act::Back),
            }
        }
        name => {
            if name == me {
                return Err(format!("CPF1310 Group job {name} is this job."));
            }
            if let Some(m) = members.iter().find(|m| m.0 == name) {
                return Ok(hand_over(host, &FromJob::GroupTransfer { job: m.1 }));
            }
            Ok(hand_over(
                host,
                &FromJob::GroupStart {
                    group,
                    name: name.to_string(),
                    text: String::new(),
                    program: program.to_string(),
                },
            ))
        }
    }
}

/// Transfer to Group Job: the job number picked, `None` when left.
fn select(host: &Host, me: &str, others: &[(String, u32, String)]) -> Option<u32> {
    let mut d = crate::panels::dialog(host, "QWCTFRGJ").ok()?;
    d.set("CURGRP", me);
    for (name, number, text) in others {
        d.add_row(
            "GRPJOBS",
            vec![
                ("GRPJOB".into(), name.clone()),
                ("JOBNBR".into(), format!("{number:06}")),
                ("GRPTEXT".into(), text.clone()),
            ],
        );
    }
    d.list_mut("GRPJOBS").complete = true;
    let mut surface = Surface::new(host.clone());
    loop {
        match d.show(&mut surface, "TFRGRPJOB").ok()? {
            Response::Options(c) => {
                if let Some(pick) = c.iter().find(|c| c.option == "1") {
                    return others.get(pick.row).map(|o| o.1);
                }
                d.message = Some(("Type 1 next to the job to transfer to.".into(), true));
            }
            Response::Return(_) => {}
            _ => return None,
        }
    }
}

/// The text of a command's escape.
fn fail(m: &str) -> Fail {
    let (id, text) = m.split_once(' ').unwrap_or(("CPF1309", m));
    Fail::new(id, text)
}

/// `TFRSECJOB`.
#[allow(clippy::unnecessary_wraps)] // the commands' signature
pub fn tfrsecjob(host: &Host) -> Result<String, Fail> {
    if secondary(host) == Act::Gone {
        keys::gone();
    }
    Ok(String::new())
}

/// `CHGGRPA GRPJOB(name|*NONE|*SAME) GRPJOBTXT(text)`.
pub fn chggrpa(host: &Host, name: &str, text: &str) -> Result<String, Fail> {
    let name = if name == "*SAME" || name.is_empty() {
        match info(host) {
            Some((_, me, _)) => me,
            None if text.is_empty() || text.eq_ignore_ascii_case("*SAME") => {
                return Ok(String::new());
            }
            None => return Err(fail("CPF1311 The job is not a group job: name it first.")),
        }
    } else {
        name.to_string()
    };
    let text = if text.eq_ignore_ascii_case("*SAME") { "" } else { text };
    host.link
        .borrow_mut()
        .with(|l| l.grp_chg(name, text.to_string()))
        .map(|_| String::new())
        .map_err(|e| fail(&e))
}

/// `TFRGRPJOB GRPJOB(name|*PRV|*SELECT) INLGRPPGM(lib/pgm|*NONE)`.
pub fn tfrgrpjob(host: &Host, which: &str, program: &str) -> Result<String, Fail> {
    let which = if which.is_empty() { "*SELECT" } else { which };
    let program = if program.is_empty() || program.starts_with('*') { "" } else { program };
    match transfer_to(host, which, program) {
        Ok(Act::Gone) => {
            keys::gone();
            Ok(String::new())
        }
        Ok(_) => Ok(String::new()),
        Err(m) => Err(fail(&m)),
    }
}

/// `ENDGRPJOB GRPJOB(name|*)`: another job of the group ends; this one
/// signs off, and the display goes to another job of the group.
pub fn endgrpjob(host: &Host, which: &str) -> Result<String, Fail> {
    let (_, me, members) = info(host)
        .ok_or_else(|| fail("CPF1311 The job is not a group job."))?;
    if which.is_empty() || which == "*" || which == me {
        keys::sign_off();
        return Ok(String::new());
    }
    let m = members
        .iter()
        .find(|m| m.0 == which)
        .ok_or_else(|| fail(&format!("CPF1310 Group job {which} not found in the group.")))?;
    host.ctl
        .borrow_mut()
        .send(&FromJob::GroupEnd { job: m.1 })
        .map_err(|e| fail(&e.to_string()))?;
    Ok(String::new())
}
