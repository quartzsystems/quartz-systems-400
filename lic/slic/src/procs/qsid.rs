// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `slic`'s side of the partition service channel (`/init qsid`,
//! `guestagent::qsid`, `docs/QSID.md`): under QS/VS, with the service
//! port there, the daemon starts before the first reference code and
//! is told of each one as it is posted. It asks for function 21 on the
//! same channel; a power down arrives as the guest agent's does (the
//! request file and `SIGUSR1`). Started again should it end.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use guestagent::qsid::{Ask, Notice};
use ipc::chan::Chan;

use super::spawn::{SELF, spawn};
use crate::ipl::Event;
use crate::ipl::system::System;

/// How long a send to the daemon may take before it is given up as hung.
const SEND_LIMIT: Duration = Duration::from_secs(2);
/// How long a halt or a power off waits for the last codes to go out.
const FLUSH_LIMIT: Duration = Duration::from_secs(3);

/// The running daemon.
pub struct QsidProc {
    /// Its process ID.
    pub pid: u32,
    chan: Chan,
    /// The last [`Ask::Flushed`] token seen.
    flushed: Arc<AtomicU32>,
    token: u32,
}

/// A copy of the channel for the panic hook, which owns nothing.
static EMERGENCY: OnceLock<Mutex<Option<Chan>>> = OnceLock::new();

/// Whether this system is a partition of QS/VS.
pub fn is_partition() -> bool {
    let roots = hwqual::Roots::default();
    let dmi = hwrsrc::dmi::Dmi::read_from(&roots.sys.join("class/dmi/id"));
    hwqual::checks::qsvs(&dmi, &roots)
}

/// Start the daemon when this is a partition with its service port:
/// as early as the state file can be kept.
pub fn start(system: &mut System, tx: &Sender<Event>) {
    if !system.qsvs {
        return;
    }
    let sys = std::path::Path::new("/sys");
    let Some(node) = guestagent::port::find(sys, qsipart::SERVICE_PORT) else {
        system.out.serial(&format!(
            "slic: partition of QS/VS without its service port {}\n",
            qsipart::SERVICE_PORT
        ));
        return;
    };
    match spawn(SELF, &["qsid"]) {
        Ok((pid, chan)) => {
            let _ = chan.stream().set_write_timeout(Some(SEND_LIMIT));
            let flushed = Arc::new(AtomicU32::new(0));
            if let Ok(reader) = chan.try_clone() {
                let tx = tx.clone();
                let seen = Arc::clone(&flushed);
                std::thread::spawn(move || read_asks(reader, &tx, &seen));
            }
            if let Ok(copy) = chan.try_clone() {
                let slot = EMERGENCY.get_or_init(|| Mutex::new(None));
                if let Ok(mut s) = slot.lock() {
                    *s = Some(copy);
                }
            }
            system.out.serial(&format!(
                "slic: partition service channel {pid} on /dev/{node}\n"
            ));
            system.procs.qsid = Some(QsidProc {
                pid,
                chan,
                flushed,
                token: 0,
            });
            // Its first hello needs the state before any code is posted.
            changed(system);
        }
        Err(e) => system
            .out
            .serial(&format!("slic: partition service channel: {e}\n")),
    }
}

/// The daemon's asks, as events; flush answers kept for [`flush`].
fn read_asks(mut chan: Chan, tx: &Sender<Event>, flushed: &AtomicU32) {
    while let Ok(a) = chan.recv::<Ask>() {
        match a {
            Ask::Flushed { token } => flushed.store(token, Ordering::SeqCst),
            Ask::Dst { .. } => {
                if tx.send(Event::Qsid(a)).is_err() {
                    break;
                }
            }
        }
    }
}

/// Send `n`; a daemon that does not take it is ended (and started again
/// when reaped).
fn send(system: &mut System, n: &Notice) {
    let Some(q) = system.procs.qsid.as_mut() else {
        return;
    };
    if q.chan.send(n).is_err() {
        let pid = q.pid;
        system.out.serial(&format!(
            "slic: partition service channel {pid} not answering; ending it\n"
        ));
        if let Some(p) = rustix::process::Pid::from_raw(i32::try_from(pid).unwrap_or(0)) {
            let _ = rustix::process::kill_process(p, rustix::process::Signal::KILL);
        }
    }
}

/// A code was posted.
pub fn posted(system: &mut System, src: &crate::src::Src) {
    if system.procs.qsid.is_none() {
        return;
    }
    let status = system.status(Some(src)).render();
    send(system, &Notice::Posted { status });
}

/// The state changed without a code: halted, or ended.
pub fn changed(system: &mut System) {
    if system.procs.qsid.is_none() {
        return;
    }
    let status = system.status(None).render();
    send(system, &Notice::Changed { status });
}

/// The answer to function 21: `None` accepted, else the refusal.
pub fn answer(system: &mut System, id: u32, refusal: Option<(&str, &str)>) {
    let refusal = refusal.map(|(m, t)| (m.to_string(), t.to_string()));
    send(system, &Notice::Answer { id, refusal });
}

/// Wait until the daemon has written everything so far to the port, or
/// a few seconds: before a halt or a power off.
pub fn flush(system: &mut System) {
    let Some(q) = system.procs.qsid.as_mut() else {
        return;
    };
    q.token = q.token.wrapping_add(1).max(1);
    let token = q.token;
    let seen = Arc::clone(&q.flushed);
    send(system, &Notice::Flush { token });
    let until = Instant::now() + FLUSH_LIMIT;
    while seen.load(Ordering::SeqCst) != token && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// From the panic hook: the terminal code, then a moment for it to go
/// out. Owns nothing of the IPL's state.
pub fn emergency(src: &crate::src::Src) {
    let Some(slot) = EMERGENCY.get() else {
        return;
    };
    let Ok(mut guard) = slot.lock() else {
        return;
    };
    let Some(chan) = guard.as_mut() else {
        return;
    };
    let mut s = guestagent::status::Status::parse(
        &std::fs::read_to_string(guestagent::status::PATH).unwrap_or_default(),
    );
    s.src = src.code.to_string();
    s.src_text = src.text.to_string();
    s.src_time = crate::ipl::system::now_ms();
    let _ = chan.send(&Notice::Posted { status: s.render() });
    s.state = "down".into();
    let _ = chan.send(&Notice::Changed { status: s.render() });
    let _ = chan.send(&Notice::Flush { token: u32::MAX });
    std::thread::sleep(Duration::from_secs(1));
}

/// After a child exited: the daemon again if it was the one.
pub fn reaped(system: &mut System, pid: u32, tx: &Sender<Event>) {
    if system.procs.qsid.as_ref().is_some_and(|p| p.pid == pid) {
        system.procs.qsid = None;
        system.out.serial(&format!(
            "slic: partition service channel ({pid}) ended; starting it again\n"
        ));
        start(system, tx);
    }
}
