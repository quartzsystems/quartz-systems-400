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

pub(crate) mod acg;
mod census;
pub(crate) mod certs;
pub(crate) mod cmn;
pub(crate) mod cmntrc;
pub(crate) mod ddm;
mod dbg;
pub(crate) mod devices;
pub(crate) mod exec;
pub(crate) mod hist;
mod hostsvr;
pub(crate) mod ifs;
pub(crate) mod ifsvol;
mod isql;
mod joblog;
pub(crate) mod media;
mod pdm;
mod dfu;
pub(crate) mod sst;
pub(crate) mod dumps;
pub(crate) mod liclog;
mod svcop;
pub(crate) mod trace;
pub(crate) mod jobqs;
mod jobs;
pub(crate) mod jobscd;
pub(crate) mod licpgm;
pub(crate) mod fmt;
pub(crate) mod msgs;
pub(crate) mod prtf;
pub(crate) mod prtdev;
pub(crate) mod prblog;
pub(crate) mod qap;
pub(crate) mod qsh;
pub(crate) mod regfac;
pub(crate) mod sbs;
pub(crate) mod security;
pub(crate) mod splcmd;
pub(crate) mod spool;
pub(crate) mod perfcol;
pub(crate) mod stats;
pub(crate) mod tcp;
mod units;
mod work2;
mod uiapi;
pub(crate) mod webadm;
pub(crate) mod wtr;

pub use jobs::chgjob;
mod served;
mod sqlhost;
mod sqlsrv;
mod fiber;
mod turns;
#[cfg(test)]
mod longtests;
mod seu;
pub(crate) mod sysdata;
mod authdata;
mod cppdata;
mod cppmedia;
mod system2;
mod sysdsp;

use std::fs;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};

use blockdev::{BlockDev, LinuxBlockDev};
use ipc::chan::Chan;
use stg::{JobInfo, Start};
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
    serve(system, slic, listener)
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
    let (store, recovery) =
        store::Store::open(devs, storage::config()).map_err(|e| format!("open ASP 1: {e}"))?;
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
    storage::pick_up_resumes();
    system.temp = Some(storage::fresh_temp().map_err(|e| format!("temporary ASP: {e}"))?);
    purge_sql_qtemp(system);
    let mut made = install(system).map_err(|e| format!("work management objects: {e}"))?;
    if let Ok(path) = licpart::locate()
        && let Ok(f) = fs::OpenOptions::new().read(true).write(true).open(&path)
    {
        let dev: Box<dyn licpart::Device> = Box::new(f);
        let mut part = licpart::LicPart::open(dev);
        if let Ok(record) = part.read() {
            // The record is slic's: the service writes only the TLVs it
            // owns (the LIC PTF slots, the IPL attributes), refreshed
            // first, and the resume journal.
            system.lic = Some((part, record));
        }
    }
    let ptfs = crate::cpp::ptfcmd::ipl_pending(system);
    made += usize::from(licpgm::seed(system));
    jobqs::load(system);
    spool::load(system);
    wtr::clear_queues(system);
    let text = if made > 0 {
        format!("{text}; {made} work management object(s) created")
    } else {
        text
    };
    let text = if ptfs.is_empty() {
        text
    } else {
        format!("{text}; {ptfs}")
    };
    if let Some(blob) = &start.sysvals {
        let host = crate::ipl::sysvals::host_info();
        let mut vals =
            SysVals::from_blob(blob, &host).map_err(|e| format!("system values: {e}"))?;
        vals.set_system_blob(&start.system)
            .map_err(|e| format!("system values: {e}"))?;
        system.sysvals = Some(vals);
    }
    // The page cache is the *MACHINE pool, sized once the system values
    // are here (until then the store's default bound).
    if let Some(kb) = system
        .sysvals
        .as_ref()
        .and_then(|v| v.get("QMCHPOOL").ok())
        .and_then(|v| v.text().trim().parse::<usize>().ok())
        && let Some(s) = system.storage.as_mut()
    {
        s.store.resize_cache(kb * 1024 / store::page::PAGE_SIZE);
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
    let text = format!("{text}; {}", ifs_line(system));
    let text = certs::ensure_system(system).map_or_else(|| text.clone(), |made| format!("{text}; {made}"));
    system.pools = sbs::pool_table_saved(system);
    let iplsts = jobs::take_next_iplsts(system);
    if iplsts != 0
        && let Some(v) = system.sysvals.as_mut()
    {
        let _ = v.set_system("QIPLSTS", sysval::Value::Char(iplsts.to_string()));
    }
    // The IPL attributes (CHGIPLA): queues, writers, TCP/IP, restricted state.
    let ipla = crate::cpp::iplcmd::at_ipl(system);
    let mut text = if ipla.is_empty() {
        text
    } else {
        format!("{text}; {ipla}")
    };
    // SRCs raised while opening (delayed PTFs) ride after a separator
    // for slic to show; there is no reply to carry them yet.
    let raised: Vec<&str> = std::mem::take(&mut system.raised).into_iter().map(|s| s.code).collect();
    if !raised.is_empty() {
        text.push(SRC_SEP);
        text.push_str(&raised.join(","));
    }
    Ok(text)
}

/// The IFS volume served and mounted (when it is in ASP 1), then the
/// service's file system over it: their line of the open reply.
fn ifs_line(system: &mut System) -> String {
    match ifsvol::start(system) {
        Some(served) => format!("{served}; {}", ifs::start(system)),
        None => ifs::start(system),
    }
}

/// Drop the SQL `QTEMP` libraries the jobs of a stopped system left in
/// ASP 1 (`sql::exec::qtemp`).
fn purge_sql_qtemp(system: &mut System) {
    let Some(s) = system.storage.as_mut() else { return };
    let Some(qsys) = s.qsys else { return };
    let Ok(mut txn) = s.store.begin() else { return };
    match sql::exec::qtemp::purge(&mut txn, qsys, store::obj::JobHandle(0)) {
        Ok(n) => {
            if let Err(e) = txn.commit() {
                eprintln!("qsstg: SQL QTEMP purge: {e}");
            } else if n > 0 {
                eprintln!("qsstg: dropped {n} SQL QTEMP librar{} left by ended jobs", if n == 1 { "y" } else { "ies" });
            }
        }
        Err(e) => {
            txn.abort();
            eprintln!("qsstg: SQL QTEMP purge: {e}");
        }
    }
}

/// Separates the SRC codes from the text of the open reply.
pub const SRC_SEP: char = '\u{1}';

/// Bind the jobs' socket, replacing a stale one.
fn listen(path: &Path) -> Result<UnixListener, String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let _ = fs::remove_file(path);
    UnixListener::bind(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Before the first request: the IPL's message, communications, media,
/// the descriptor relay, `SCPF`'s job, a recovery's user data.
fn prepare(system: &mut System) -> Option<served::JobCtx> {
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
    liclog::write(0x0100, 0x0001, 0, "");
    cmn::start(system);
    eprintln!("qsstg: {}", media::start(system));
    // The descriptor relay between job processes (givedescriptor,
    // takedescriptor, the host server daemons and their prestart jobs).
    std::thread::spawn(|| {
        if let Err(e) = sockets::relay::serve(sockets::relay::SOCKET) {
            eprintln!("qsstg: descriptor relay: {e}");
        }
    });
    let ctx = match served::begin_job(system, scpf) {
        Ok(c) => Some(c),
        Err(e) => {
            eprintln!("qsstg: SCPF: {e}");
            None
        }
    };
    // A recovery's user libraries and IFS, from the files after the
    // system save it installed from.
    for line in crate::cpp::savsys::restore_user_data(system) {
        eprintln!("qsstg: {line}");
    }
    ctx
}

/// Serve `slic` and the jobs until `slic` asks for the close: rounds of
/// the loop in `turns`.
fn serve(mut system: System, slic: Chan, listener: UnixListener) -> ! {
    let ctx = prepare(&mut system);
    let mut svc = turns::Svc::new(system, vec![Conn::new(slic, ctx)], listener);
    loop {
        let wait = turns::top_wait(&svc.system);
        svc = turns::round(svc, wait);
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
    // The operating system's objects (the QS00SS1 load's content), then
    // the system's own configuration.
    let r = osobjects::create(&mut txn, qsys, &stamp).map_err(|e| e.to_string())?;
    for (name, why) in &r.failed_commands {
        eprintln!("qsstg: command {name}: {why}");
    }
    for p in &r.problems {
        eprintln!("qsstg: {p}");
    }
    let mut made = r.made + r.commands_replaced;
    made += usize::from(crate::svc::neta::ensure(&mut txn, qsys, stamp.now)?);
    made += tcp::store::install(&mut txn, qsys, &stamp)?;
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
