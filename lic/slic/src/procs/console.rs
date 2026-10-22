// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The console's interactive job. `slic` is the workstation controller
//! of the console display `QSCONSOLE`: a sign-on routes through the
//! controlling subsystem's workstation entry for the console, the job
//! description's routing data and the routing entry it selects, and
//! starts the job; its panels and the keys typed at them pass through
//! here. When the job ends, the sign-on display comes back.
//! `MIGRATE(phase8)`: 5250 devices and their own subsystem jobs.

use std::sync::mpsc::Sender;
use std::thread;

use ::console::{Attr, Color, Screen};
use ipc::chan::Chan;
use stg::Client;
use wm::ctl::{FromJob, Panel, Start, ToJob};
use wm::obj::jobd::JobD;
use wm::obj::sbsd::{PoolKind, Sbsd};

use super::spawn::{QSJOB, spawn};
use crate::ipl::Event;
use crate::ipl::system::System;

/// The console's display device.
pub const DEVICE: &str = "QSCONSOLE";

/// The signed-on job.
pub struct ConsoleJob {
    /// Its process ID.
    pub pid: u32,
    chan: Chan,
    /// Its user.
    pub user: String,
    /// When the display was last used.
    pub used: std::time::Instant,
}

impl ConsoleJob {
    /// Pass Enter or a function key on.
    pub fn input(&mut self, key: u8, fields: Vec<String>) {
        self.used = std::time::Instant::now();
        let _ = self.chan.send(&ToJob::Input { key, fields });
    }

    /// Ask it to end.
    pub fn end(&mut self) {
        let _ = self.chan.send(&ToJob::End { immediate: false });
    }
}

/// The controlling subsystem.
pub fn controlling(system: &System) -> String {
    system
        .sysvals
        .as_ref()
        .and_then(|v| v.get("QCTLSBSD").ok())
        .and_then(|v| v.text().split_whitespace().next().map(str::to_string))
        .unwrap_or_else(|| "QBASE".into())
}

/// Show the sign-on display, with a message when one is given.
pub fn sign_on(system: &System, message: &str) {
    // QDSIGNON2 at password level 2 and 3: the long password field.
    let long_password = system
        .sysvals
        .as_ref()
        .and_then(|v| v.get("QPWDLVL").ok())
        .and_then(|v| v.text().trim().parse::<u8>().ok())
        .is_some_and(|l| l >= 2);
    system.show(::console::Command::ShowSignOn {
        system: system.system_name(),
        subsystem: controlling(system),
        display: DEVICE.into(),
        long_password,
    });
    if !message.is_empty() {
        system.show(::console::Command::Message {
            text: message.into(),
            error: true,
        });
    }
}

/// What routing chose for the console job.
struct Routed {
    jobd: String,
    class: String,
    program: String,
    pool: String,
}

/// Route a sign-on: the controlling subsystem's workstation name entry
/// for the console, else its `*CONS` or `*ALL` type entry; the job
/// description (`*USRPRF` is the profile's) and its routing data; the
/// routing entry it selects.
fn route(system: &mut System, sbs: &str, profile_jobd: &str) -> Result<Routed, String> {
    let stg = system
        .procs
        .stg
        .as_mut()
        .ok_or("the storage service is not running")?;
    let b = stg
        .wm_object(format!("QSYS/{sbs}"), "*SBSD".into())
        .map_err(|e| e.to_string())??;
    let d = Sbsd::decode(&b).map_err(|e| e.to_string())?;
    let wse = d
        .wsns
        .iter()
        .find(|w| w.name == DEVICE)
        .or_else(|| {
            d.wsts
                .iter()
                .find(|w| w.name == "*CONS" || w.name == "*ALL")
        })
        .ok_or_else(|| format!("CPF1109 subsystem {sbs} has no work station entry for {DEVICE}"))?;
    let jobd = if wse.jobd.0 == "*USRPRF" && !profile_jobd.is_empty() {
        if profile_jobd.contains('/') {
            profile_jobd.to_string()
        } else {
            format!("QGPL/{profile_jobd}")
        }
    } else if wse.jobd.0.starts_with('*') {
        "QGPL/QDFTJOBD".to_string()
    } else {
        format!("{}/{}", wse.jobd.1, wse.jobd.0)
    };
    let jb = stg
        .wm_object(jobd.clone(), "*JOBD".into())
        .map_err(|e| e.to_string())??;
    let jd = JobD::decode(&jb).map_err(|e| e.to_string())?;
    let r = d
        .route(&jd.rtgdta)
        .ok_or_else(|| format!("CPF1226 no routing entry in {sbs} for '{}'", jd.rtgdta))?;
    let kind = d.pool(r.pool).map_or(PoolKind::Base, |p| p.kind);
    Ok(Routed {
        jobd,
        class: format!("{}/{}", r.cls.1, r.cls.0),
        program: format!("{}/{}", r.pgm.1, r.pgm.0),
        pool: match kind {
            PoolKind::Private => format!("{sbs}/{}", r.pool),
            k => k.name(),
        },
    })
}

/// Sign on: the user and password checked by the storage service, then
/// the job started with the program, menu and current library typed,
/// as far as `LMTCPB` lets them change the profile's.
pub fn sign_on_entered(system: &mut System, fields: &[String], tx: Sender<Event>) {
    let user = fields.first().map_or("", |f| f.trim()).to_ascii_uppercase();
    if user.is_empty() {
        sign_on(system, "Type a user profile name.");
        return;
    }
    if qsname::Name::parse(&user).is_err() {
        sign_on(system, "Password or user profile is not correct.");
        return;
    }
    let password = fields.get(1).map_or("", |f| f.trim()).to_string();
    let user_typed = user.clone();
    let Some(stg) = system.procs.stg.as_mut() else {
        sign_on(system, "Sign-on is not available until the IPL completes.");
        return;
    };
    let checked = stg
        .sign_on(user, password, DEVICE.into(), true)
        .map_err(|e| e.to_string());
    let signed = match checked {
        Ok(Ok(s)) => s,
        // The password was right; the one session is the disconnected job.
        Ok(Err(r)) if r.msgid == "CPF1220" && reconnect(system, &user_typed) => return,
        Ok(Err(r)) => {
            sign_on(system, &r.text);
            return;
        }
        Err(e) => {
            sign_on(system, &e);
            return;
        }
    };
    if reconnect(system, &signed.user) {
        return;
    }
    match start(system, signed, fields, tx) {
        Ok(()) => {}
        Err(e) => {
            system.out.serial(&format!("slic: sign-on: {e}\n"));
            sign_on(system, &e);
        }
    }
}

/// What the sign-on display's fields may change: `LMTCPB(*PARTIAL)`
/// keeps the program and current library, `*YES` all three.
fn effective(s: &mut sec::signon::SignedOn, fields: &[String]) -> String {
    let typed = |i: usize| {
        fields
            .get(i)
            .map(|f| f.trim().to_ascii_uppercase())
            .filter(|f| !f.is_empty())
    };
    let (program, menu, curlib) = match s.lmtcpb.as_str() {
        "*YES" => (false, false, false),
        "*PARTIAL" => (false, true, false),
        _ => (true, true, true),
    };
    if let (true, Some(m)) = (menu, typed(3)) {
        s.inlmnu = m;
    }
    if let (true, Some(c)) = (curlib, typed(4)) {
        s.curlib = c;
    }
    match (program, typed(2)) {
        (true, Some(p)) => p,
        _ if s.inlpgm == "*NONE" => String::new(),
        _ => s.inlpgm.clone(),
    }
}

fn start(
    system: &mut System,
    mut signed: sec::signon::SignedOn,
    fields: &[String],
    tx: Sender<Event>,
) -> Result<(), String> {
    let sbs = controlling(system);
    let r = route(system, &sbs, &signed.jobd)?;
    let number = system
        .procs
        .stg
        .as_mut()
        .ok_or("the storage service is not running")?
        .job_number()
        .map_err(|e| e.to_string())??;
    let program = effective(&mut signed, fields);
    let user = signed.user.clone();
    let user_name = user.clone();
    let (pid, mut chan) = spawn(QSJOB, &[])?;
    super::cgroup_place(&r.pool, pid);
    chan.send(&ToJob::Start {
        start: Box::new(Start {
            name: DEVICE.into(),
            user,
            number,
            handle: u64::from(number),
            kind: 'I',
            subsystem: sbs,
            role: "INTER".into(),
            jobd: r.jobd,
            class: r.class,
            program: r.program,
            rqsdta: program,
            pool: r.pool,
            lda: Vec::new(),
            signon: Some(signed),
        }),
    })
    .map_err(|e| e.to_string())?;
    match chan.recv::<FromJob>() {
        Ok(FromJob::Ready { .. }) => {}
        Ok(FromJob::Ending { reason }) => return Err(reason),
        Ok(other) => return Err(format!("unexpected {other:?}")),
        Err(e) => return Err(e.to_string()),
    }
    let mut reader = chan.try_clone().map_err(|e| e.to_string())?;
    thread::spawn(move || {
        while let Ok(m) = reader.recv::<FromJob>() {
            if tx.send(Event::ConsoleJob(m)).is_err() {
                return;
            }
        }
        let _ = tx.send(Event::ConsoleGone(pid));
    });
    system.procs.console = Some(ConsoleJob {
        pid,
        chan,
        user: user_name,
        used: std::time::Instant::now(),
    });
    Ok(())
}

/// A panel as a console screen.
#[allow(clippy::many_single_char_names)] // panel, screen, attribute, row, column
pub fn screen(p: &Panel) -> Screen {
    let mut s = Screen::new();
    for (row, col, attr, text) in &p.texts {
        let a = match attr {
            1 => Attr::WHITE,
            2 => Attr::color(Color::Blue),
            3 => Attr::RED,
            _ => Attr::GREEN,
        };
        s.put(usize::from(*row), usize::from(*col), text, a);
    }
    for (row, col, len, hidden, value) in &p.fields {
        let (r, c) = (usize::from(*row), usize::from(*col));
        s.field(r, c, usize::from(*len), *hidden);
        let reverse = p.reverse.contains(&(*row, *col));
        let a = if *hidden {
            Attr::INPUT_HIDDEN
        } else if reverse {
            Attr {
                reverse: true,
                ..Attr::INPUT
            }
        } else {
            Attr::INPUT
        };
        if !value.is_empty() || reverse {
            let shown = format!("{value:<width$}", width = usize::from(*len));
            s.put(r, c, &shown, a);
        }
    }
    if let Some((row, col)) = p.cursor {
        s.cursor = (usize::from(row), usize::from(col));
    }
    s
}

/// What the console job sent.
pub fn from_job(system: &mut System, m: FromJob) {
    match m {
        FromJob::Show { panel } => {
            system.show(::console::Command::ShowScreen(screen(&panel)));
            if !panel.message.is_empty() {
                system.show(::console::Command::Message {
                    text: panel.message,
                    error: panel.error,
                });
            }
        }
        FromJob::Output { text } => system.out.serial(&text),
        FromJob::PowerDown {
            immediate,
            delay,
            restart,
        } => crate::ipl::power::down_ending(system, restart, immediate, delay),
        FromJob::Disconnect => disconnect(system),
        FromJob::Ending { .. } | FromJob::Ready { .. } | FromJob::Done { .. } => {}
    }
}

/// The console job is gone: back to the sign-on display.
pub fn gone(system: &mut System, pid: u32) {
    if system.procs.console.as_ref().is_some_and(|j| j.pid == pid) {
        system.procs.console = None;
        sign_on(system, "");
    } else if system
        .procs
        .disconnected
        .as_ref()
        .is_some_and(|d| d.job.pid == pid)
    {
        system.procs.disconnected = None;
    }
}

/// The job the display left by `DSCJOB` or inactivity, when it went.
pub struct Disconnected {
    /// The job.
    pub job: ConsoleJob,
    /// Since when.
    pub at: std::time::Instant,
}

/// Back to `user`'s disconnected job, when there is one: its display
/// shows again.
fn reconnect(system: &mut System, user: &str) -> bool {
    match system.procs.disconnected.take() {
        Some(d) if d.job.user == user => {
            let mut j = d.job;
            j.used = std::time::Instant::now();
            // An empty Enter makes the job show its panel again.
            j.input(0, Vec::new());
            system.procs.console = Some(j);
            true
        }
        other => {
            system.procs.disconnected = other;
            false
        }
    }
}

/// Free the display and keep the job (`DSCJOB`, `QINACTMSGQ(*DSCJOB)`).
pub fn disconnect(system: &mut System) {
    if let Some(j) = system.procs.console.take() {
        if let Some(old) = system.procs.disconnected.take() {
            let mut o = old.job;
            o.end();
        }
        system.procs.disconnected = Some(Disconnected {
            job: j,
            at: std::time::Instant::now(),
        });
        sign_on(system, "");
    }
}

/// Minutes a system value names, `None` for `*NONE` or unset.
fn minutes(system: &System, name: &str) -> Option<u64> {
    system
        .sysvals
        .as_ref()
        .and_then(|v| v.get(name).ok())
        .and_then(|v| v.text().trim().parse::<u64>().ok())
        .filter(|m| *m > 0)
}

/// `QINACTITV` and `QINACTMSGQ` for the console job, `QDSCJOBITV` for a
/// disconnected one.
pub fn inactivity(system: &mut System) {
    if let (Some(itv), Some(j)) = (minutes(system, "QINACTITV"), system.procs.console.as_ref())
        && j.used.elapsed().as_secs() >= itv * 60
    {
        let action = system
            .sysvals
            .as_ref()
            .and_then(|v| v.get("QINACTMSGQ").ok())
            .map(|v| v.text())
            .unwrap_or_default();
        let action = action.split_whitespace().collect::<Vec<_>>();
        match action.first().copied().unwrap_or("*ENDJOB") {
            "*ENDJOB" => {
                if let Some(mut j) = system.procs.console.take() {
                    system.out.serial(&format!("slic: {} inactive; job ended\n", j.user));
                    j.end();
                    sign_on(system, "The job ended: the display was not used.");
                }
            }
            "*DSCJOB" => {
                system.out.serial("slic: console inactive; job disconnected\n");
                disconnect(system);
            }
            queue => {
                let lib = action.get(1).copied().unwrap_or("*LIBL");
                let user = j.user.clone();
                if let Some(c) = system.procs.console.as_mut() {
                    c.used = std::time::Instant::now();
                }
                let msg = wm::obj::msgq::QMsg {
                    msgid: "CPI1126".into(),
                    msgf: ("QCPFMSG".into(), "QSYS".into()),
                    text: format!("Job {DEVICE} of user {user} has not been used for {itv} minutes."),
                    severity: 0,
                    ..wm::obj::msgq::QMsg::default()
                };
                if let Some(stg) = system.procs.stg.as_mut() {
                    let _ = stg.msgq_send(format!("{lib}/{queue}"), msg);
                }
            }
        }
    }
    if let (Some(itv), Some(d)) = (minutes(system, "QDSCJOBITV"), system.procs.disconnected.as_ref())
        && d.at.elapsed().as_secs() >= itv * 60
        && let Some(d) = system.procs.disconnected.take()
    {
        let mut j = d.job;
        j.end();
    }
}
