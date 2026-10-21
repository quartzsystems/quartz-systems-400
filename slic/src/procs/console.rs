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
}

impl ConsoleJob {
    /// Pass Enter or a function key on.
    pub fn input(&mut self, key: u8, fields: Vec<String>) {
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
    system.show(::console::Command::ShowSignOn {
        system: system.system_name(),
        subsystem: controlling(system),
        display: DEVICE.into(),
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
/// description's routing data; the routing entry it selects.
fn route(system: &mut System, sbs: &str) -> Result<Routed, String> {
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
    let jobd = if wse.jobd.0.starts_with('*') {
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

/// Sign on: user, password (not checked yet, `MIGRATE(phase6)`),
/// program, menu, current library.
pub fn sign_on_entered(system: &mut System, fields: &[String], tx: Sender<Event>) {
    let user = fields.first().map_or("", |f| f.trim()).to_ascii_uppercase();
    if user.is_empty() {
        sign_on(system, "Type a user profile name.");
        return;
    }
    if qsname::Name::parse(&user).is_err() {
        sign_on(system, &format!("{user} is not a valid user profile name."));
        return;
    }
    match start(system, &user, fields, tx) {
        Ok(()) => {}
        Err(e) => {
            system.out.serial(&format!("slic: sign-on: {e}\n"));
            sign_on(system, &e);
        }
    }
}

fn start(
    system: &mut System,
    user: &str,
    fields: &[String],
    tx: Sender<Event>,
) -> Result<(), String> {
    let sbs = controlling(system);
    let r = route(system, &sbs)?;
    let number = system
        .procs
        .stg
        .as_mut()
        .ok_or("the storage service is not running")?
        .job_number()
        .map_err(|e| e.to_string())??;
    let program = fields.get(2).map_or("", |f| f.trim()).to_ascii_uppercase();
    let (pid, mut chan) = spawn(QSJOB, &[])?;
    super::cgroup_place(&r.pool, pid);
    chan.send(&ToJob::Start {
        start: Box::new(Start {
            name: DEVICE.into(),
            user: user.into(),
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
        let _ = tx.send(Event::ConsoleGone);
    });
    system.procs.console = Some(ConsoleJob { pid, chan });
    if let Some(lib) = fields.get(4).map(|f| f.trim()).filter(|f| !f.is_empty()) {
        system.out.serial(&format!(
            "slic: sign-on current library {lib} is applied by CHGCURLIB in the job\n"
        ));
    }
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
        if !value.is_empty() {
            let a = if *hidden {
                Attr::INPUT_HIDDEN
            } else {
                Attr::INPUT
            };
            s.put(r, c, value, a);
        }
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
        FromJob::Ending { .. } | FromJob::Ready { .. } | FromJob::Done { .. } => {}
    }
}

/// The console job is gone: back to the sign-on display.
pub fn gone(system: &mut System) {
    if system.procs.console.take().is_some() {
        sign_on(system, "");
    }
}
