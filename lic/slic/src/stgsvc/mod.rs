// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The storage service, `/init qsstg`: the one process that owns the
//! object store, the temporary ASP, the database and the lock tables.
//! `slic` starts it at the end of the IPL with the units it opened;
//! jobs connect to [`stg::SOCKET`] and say who they are. Every request
//! runs in transactions the service begins and ends, so a job that
//! vanishes leaves no transaction open; what it held at a higher level
//! (open files, a commit cycle, record and object locks) is released
//! when its connection closes. `docs/STORAGE_SERVICE.md` has the design.

pub(crate) mod devices;
pub(crate) mod exec;
pub(crate) mod hist;
mod joblog;
pub(crate) mod jobqs;
mod jobs;
pub(crate) mod jobscd;
pub(crate) mod msgs;
pub(crate) mod prtf;
pub(crate) mod sbs;
pub(crate) mod security;
pub(crate) mod splcmd;
pub(crate) mod spool;
pub(crate) mod stats;
pub(crate) mod wtr;

pub use jobs::chgjob;
mod served;

use std::fs;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::time::Duration;

use blockdev::{BlockDev, LinuxBlockDev};
use ipc::chan::Chan;
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use stg::{JobInfo, Request, Start};
use sysval::SysVals;

use crate::ipl::cmdline::Boot;
use crate::ipl::storage::{self, Storage};
use crate::ipl::system::{Role, System};
use crate::platform::console::Outputs;

use served::Conn;

/// Run as the storage service. Never returns.
pub fn run() -> ! {
    let mut slic = match Chan::from_stdin() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("qsstg: no channel to slic: {e}");
            std::process::exit(1);
        }
    };
    let start: Start = match slic.recv() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("qsstg: start: {e}");
            std::process::exit(1);
        }
    };
    let mut system = System::new(Outputs::open(), Boot::read());
    system.role = Role::Storage;
    let opened = open(&mut system, &start).and_then(|text| {
        let l = listen(Path::new(stg::SOCKET))?;
        Ok((text, l))
    });
    let (answer, listener) = match opened {
        Ok((text, l)) => (Ok(text), Some(l)),
        Err(e) => (Err(e), None),
    };
    let _ = slic.send(&answer);
    let Some(listener) = listener else {
        std::process::exit(1);
    };
    serve(&mut system, slic, &listener)
}

/// Open the units `slic` closed, a fresh temporary ASP, the system
/// values as the IPL left them, and the LIC partition for the resume
/// journal of a mirrored half.
fn open(system: &mut System, start: &Start) -> Result<String, String> {
    let mut devs: Vec<Box<dyn BlockDev>> = Vec::new();
    let mut paths = Vec::new();
    for d in &start.devices {
        let dev = LinuxBlockDev::open(Path::new(d)).map_err(|e| format!("open {d}: {e}"))?;
        devs.push(Box::new(dev));
        paths.push(PathBuf::from(d));
    }
    let (mut store, recovery) =
        store::Store::open(devs, storage::config()).map_err(|e| format!("open ASP 1: {e}"))?;
    // The page cache is the *MACHINE pool.
    if let Some(kb) = system
        .sysvals
        .as_ref()
        .and_then(|v| v.get("QMCHPOOL").ok())
        .and_then(|v| v.text().trim().parse::<usize>().ok())
    {
        store.resize_cache(kb * 1024 / store::page::PAGE_SIZE);
    }
    let sb = store.asp().superblock();
    let qsys = (!sb.qsys.is_none()).then_some(sb.qsys);
    let text = format!(
        "{} unit(s), generation {}, {} transaction(s) replayed",
        paths.len(),
        sb.generation,
        recovery.replayed
    );
    system.storage = Some(Storage {
        store,
        qsys,
        fresh: false,
        recovery,
        devices: paths,
        resume: None,
    });
    system.temp = Some(storage::fresh_temp().map_err(|e| format!("temporary ASP: {e}"))?);
    let mut made = install(system).map_err(|e| format!("work management objects: {e}"))?;
    made += install_commands(system);
    jobqs::load(system);
    spool::load(system);
    wtr::clear_queues(system);
    // The startup program: CL source compiled here, against the shipped
    // definitions.
    let strup = include_str!("shipped/QSTRUP.clle");
    match crate::cpp::clpgm::compile_shipped("QSTRUP", strup).and_then(|qir| {
        crate::svc::ile::commands::ship_program(system, "QSYS", &qir, "Startup program")
    }) {
        Ok(true) => made += 1,
        Ok(false) => {}
        Err(e) => eprintln!("qsstg: QSYS/QSTRUP: {e}"),
    }
    let text = if made > 0 {
        format!("{text}; {made} work management object(s) created")
    } else {
        text
    };
    if let Some(blob) = &start.sysvals {
        let host = crate::ipl::sysvals::host_info();
        let mut vals =
            SysVals::from_blob(blob, &host).map_err(|e| format!("system values: {e}"))?;
        vals.set_system_blob(&start.system)
            .map_err(|e| format!("system values: {e}"))?;
        system.sysvals = Some(vals);
    }
    let text = match security::install(system) {
        Ok(d) if d.profiles > 0 => format!(
            "{text}; {} user profile(s) created, {} object(s) re-owned",
            d.profiles, d.reowned
        ),
        Ok(_) => text,
        Err(e) => format!("{text}; user profiles: {e}"),
    };
    security::guard(system);
    system.pools = sbs::pool_table_saved(system);
    let iplsts = jobs::take_next_iplsts(system);
    if iplsts != 0
        && let Some(v) = system.sysvals.as_mut()
    {
        let _ = v.set_system("QIPLSTS", sysval::Value::Char(iplsts.to_string()));
    }
    if let Ok(path) = licpart::locate()
        && let Ok(f) = fs::OpenOptions::new().read(true).write(true).open(&path)
    {
        let dev: Box<dyn licpart::Device> = Box::new(f);
        let mut part = licpart::LicPart::open(dev);
        if let Ok(record) = part.read() {
            // Only the resume journal is used; the record is slic's.
            system.lic = Some((part, record));
        }
    }
    Ok(text)
}

/// Bind the jobs' socket, replacing a stale one.
fn listen(path: &Path) -> Result<UnixListener, String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let _ = fs::remove_file(path);
    UnixListener::bind(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Serve `slic` and the jobs until `slic` asks for the close.
fn serve(system: &mut System, slic: Chan, listener: &UnixListener) -> ! {
    let scpf = JobInfo {
        name: "SCPF".into(),
        user: "QSYS".into(),
        number: 0,
        kind: 'X',
        subtype: ' ',
        pid: 1,
        ..JobInfo::default()
    };
    ipl_message(system);
    let ctx = match served::begin_job(system, scpf) {
        Ok(c) => Some(c),
        Err(e) => {
            eprintln!("qsstg: SCPF: {e}");
            None
        }
    };
    let mut conns = vec![Conn::new(slic, ctx)];
    loop {
        let wait = if storage::busy(system) {
            Duration::from_millis(20)
        } else {
            Duration::from_secs(5)
        };
        let (accept, ready) = wait_ready(listener, &conns, wait);
        storage::tick(system);
        if accept {
            match listener.accept() {
                Ok((s, _)) => conns.push(Conn::new(Chan::new(s), None)),
                Err(e) => eprintln!("qsstg: accept: {e}"),
            }
        }
        for i in ready.into_iter().rev() {
            let frame = conns[i].chan.recv_raw();
            let req = frame.and_then(|b| ipc::decode::<Request>(&b));
            let Ok(req) = req else {
                let c = conns.remove(i);
                served::vanished(system, c);
                continue;
            };
            let close = matches!(req, Request::Close {});
            let reply = served::handle(system, &mut conns, i, req);
            let _ = conns[i].chan.send_raw(&reply);
            if close {
                std::process::exit(0);
            }
        }
    }
}

/// Wait for the listener or a connection to be readable; whether to
/// accept, and the connections to read.
fn wait_ready(listener: &UnixListener, conns: &[Conn], wait: Duration) -> (bool, Vec<usize>) {
    let mut fds = Vec::with_capacity(conns.len() + 1);
    fds.push(PollFd::new(listener, PollFlags::IN));
    for c in conns {
        fds.push(PollFd::new(c.chan.stream(), PollFlags::IN));
    }
    let t = Timespec {
        tv_sec: i64::try_from(wait.as_secs()).unwrap_or(i64::MAX),
        tv_nsec: i64::from(wait.subsec_nanos()),
    };
    if poll(&mut fds, Some(&t)).is_err() {
        return (false, Vec::new());
    }
    let hit = |f: &PollFd<'_>| {
        f.revents()
            .intersects(PollFlags::IN | PollFlags::HUP | PollFlags::ERR)
    };
    let accept = hit(&fds[0]);
    let ready = fds[1..]
        .iter()
        .enumerate()
        .filter(|(_, f)| hit(f))
        .map(|(i, _)| i)
        .collect();
    (accept, ready)
}

/// The system command set: every `*CMD` of `QSYS` created or, when its
/// shipped definition changed, replaced. How many were made.
fn install_commands(system: &mut System) -> usize {
    let Some(s) = system.storage.as_mut() else {
        return 0;
    };
    let Some(qsys) = s.qsys else {
        return 0;
    };
    let ctx = cmd::store::Ctx {
        now: crate::ipl::system::now_unix(),
        owner: "QSYS".into(),
        system: "QS400".into(),
    };
    let Ok(mut txn) = s.store.begin() else {
        return 0;
    };
    match sys_cpps::install::install(&mut txn, &ctx, qsys) {
        Ok(done) => {
            for (name, why) in &done.failed {
                eprintln!("qsstg: command {name}: {why}");
            }
            if txn.commit().is_err() {
                return 0;
            }
            done.created.len() + done.replaced.len()
        }
        Err(e) => {
            eprintln!("qsstg: installing the commands: {e}");
            txn.abort();
            0
        }
    }
}

/// The shipped work management objects that are missing.
fn install(system: &mut System) -> Result<usize, String> {
    let Some(s) = system.storage.as_mut() else {
        return Ok(0);
    };
    let Some(qsys) = s.qsys else {
        return Ok(0);
    };
    let stamp = wm::obj::Stamp {
        owner: "QSYS".into(),
        system: "QS400".into(),
        now: crate::ipl::system::now_unix(),
    };
    let mut txn = s.store.begin().map_err(|e| e.to_string())?;
    let mut made = wm::obj::shipped::install(&mut txn, qsys, &stamp).map_err(|e| e.to_string())?;
    made += usize::from(crate::svc::neta::ensure(&mut txn, qsys, stamp.now)?);
    txn.commit().map_err(|e| e.to_string())?;
    Ok(made)
}

/// `CPF0900` in the history log: this IPL, and how the last one ended.
fn ipl_message(system: &mut System) {
    let abnormal = system
        .sysvals
        .as_ref()
        .and_then(|v| v.get("QABNORMSW").ok())
        .is_some_and(|v| v.text() == "1");
    let name = system.system_name();
    let (d, t) = wm::joblog::stamp_of(crate::ipl::system::now_unix().saturating_mul(1_000_000));
    let data = wm::joblog::data(&[
        (&name, 8),
        (&d, 8),
        (&t[..8], 8),
        (if abnormal { "ABNORMAL" } else { "NORMAL" }, 8),
    ]);
    hist::system_message(
        system,
        "CPF0900",
        &data,
        "04",
        ("SCPF".into(), "QSYS".into(), 0),
    );
}
