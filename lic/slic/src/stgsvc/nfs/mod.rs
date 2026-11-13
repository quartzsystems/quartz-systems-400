// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NFS in the storage service (docs/NFS.md sections 4, 6 and 8): the
//! current export table ([`table`]), the daemon registry
//! ([`registry`]), the NFS options of `QZNFNFSO` ([`optfile`]),
//! `EXPORTFS` ([`export`]), `RLSIFSLCK`
//! ([`locks`]), the APIs ([`api`]) and the `QSYS2` views ([`sql`]). The
//! daemon jobs and the job's
//! commands reach it through `stg::nfs`.

pub mod api;
pub mod client;
pub mod export;
pub mod ids;
pub mod locks;
pub mod optfile;
pub mod registry;
pub mod sql;
pub mod table;

use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};

use sec::auth::principal::Principal;
use sec::profile::SpecialAuth;
use stg::nfs::{Daemon, NfsExport, NfsOp, NfsOptions, NfsReply};
use wm::table::JobInfo;

use crate::ipl::system::{System, now_unix};

/// What the service holds for NFS through an IPL.
#[derive(Default)]
pub struct State {
    /// The export table.
    pub table: table::ExportTable,
    /// The daemon jobs.
    pub registry: registry::Registry,
    /// The options, once read.
    pub options: Option<NfsOptions>,
}

static STATE: LazyLock<Mutex<State>> = LazyLock::new(|| Mutex::new(State::default()));

/// The state, locked.
pub fn state() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The profile daemons are submitted under (NEEDS-CODY: IBM's,
/// docs/NFS.md section 12).
const DAEMON_USER: &str = "QSECOFR";
/// Their job queue (`QP0LNFSD`'s and `QP0LRPCD`'s).
const DAEMON_JOBQ: &str = "QSYS/QSYSNOMAX";

/// The table's entries as the link carries them.
fn wire(e: &table::Entry) -> NfsExport {
    NfsExport {
        id: e.id,
        entry: nfs::exports::file::print_entry(&e.export),
        anon_uid: e.anon_uid,
        anon_profile: e.anon_profile.clone(),
        by: e.by.clone(),
        since: e.since,
    }
}

/// A refusal with a message of `QCPFMSG`.
fn refused(system: &mut System, msgid: &str, data: Vec<u8>, values: &[&str]) -> NfsReply {
    NfsReply::Refused {
        msgid: msgid.to_string(),
        text: super::msgs::text(system, msgid, values),
        data,
    }
}

/// Whether job `number` is alive: in the job table or on a queue.
fn live(system: &System, number: u32) -> bool {
    system.jobs.get(number).is_some() || system.queued.contains_key(&number)
}

/// One request from a job.
#[allow(clippy::too_many_lines)] // one arm a request
pub fn request(system: &mut System, job: &JobInfo, me: &Principal, op: NfsOp) -> NfsReply {
    let q = format!("{:06}/{}/{}", job.number, job.user, job.name);
    {
        let mut s = state();
        s.registry.prune(&|n| live(system, n));
    }
    match op {
        NfsOp::Exports => {
            let s = state();
            NfsReply::Table {
                generation: s.table.generation(),
                exports: s.table.entries().iter().map(wire).collect(),
            }
        }
        NfsOp::Generation => NfsReply::Generation {
            generation: state().table.generation(),
        },
        NfsOp::ExportOf { path } => {
            let s = state();
            NfsReply::Table {
                generation: s.table.generation(),
                exports: s.table.holding(&path).map(wire).into_iter().collect(),
            }
        }
        NfsOp::Hello { daemon } => {
            if Daemon::of_job(&job.name) != Some(daemon) {
                return refused(system, "CPFA1B4", 0i32.to_be_bytes().to_vec(), &["0"]);
            }
            let (index, restore) = state().registry.hello(daemon, job.number, &q, now_unix());
            NfsReply::Hello { index, restore }
        }
        NfsOp::Ready { ports } => {
            state().registry.set_ready(job.number, ports);
            NfsReply::Done
        }
        NfsOp::Gone => {
            state().registry.gone(job.number);
            NfsReply::Done
        }
        NfsOp::ShouldEnd => NfsReply::Flag {
            yes: state().registry.should_end(job.number),
        },
        NfsOp::Daemons => NfsReply::Daemons {
            jobs: state().registry.list(),
        },
        NfsOp::Start {
            daemon,
            count,
            rtvrpcreg,
        } => {
            if !me.has(SpecialAuth::IOSYSCFG) {
                return iosyscfg(system, "STRNFSSVR");
            }
            start(system, daemon, count, rtvrpcreg)
        }
        NfsOp::End { daemon } => {
            if !me.has(SpecialAuth::IOSYSCFG) {
                return iosyscfg(system, "ENDNFSSVR");
            }
            state().registry.end(daemon);
            NfsReply::Done
        }
        NfsOp::Options => NfsReply::Options {
            options: options(system),
        },
        NfsOp::SetOptions { options } => {
            if !me.has(SpecialAuth::IOSYSCFG) {
                return iosyscfg(system, "QZNFNFSO");
            }
            match save_options(system, &options) {
                Ok(()) => NfsReply::Done,
                Err(e) => {
                    eprintln!("slic: nfs: options: {e}");
                    refused(system, "CPFA1B4", 0i32.to_be_bytes().to_vec(), &["0"])
                }
            }
        }
        NfsOp::ServerRestarted { name, state } => {
            // Only the lock manager hears it from the status monitor.
            if Daemon::of_job(&job.name) != Some(Daemon::Nlm) {
                return refused(system, "CPFA1B4", 0i32.to_be_bytes().to_vec(), &["0"]);
            }
            client::restarted(&name, state);
            NfsReply::Done
        }
        NfsOp::Orders => NfsReply::Orders {
            orders: state().registry.take_orders(job.number),
        },
        NfsOp::OrderDone { id, count, errno } => {
            state().registry.order_done(id, count, errno);
            NfsReply::Done
        }
        NfsOp::Failed { errno } => {
            let gone = {
                let mut s = state();
                let d = s.registry.job(job.number);
                s.registry.gone(job.number);
                d
            };
            if let Some((daemon, false)) = gone {
                ended_unasked(system, daemon, &q, errno);
            }
            NfsReply::Done
        }
        NfsOp::NameOfUid { uid } => NfsReply::Name {
            name: ids(system, job, |m| m.profile_of_uid(uid)),
        },
        NfsOp::NameOfGid { gid } => NfsReply::Name {
            name: ids(system, job, |m| m.profile_of_gid(gid)),
        },
        NfsOp::UidOfName { name } => NfsReply::Id {
            id: ids(system, job, |m| m.uid_of(&name.to_ascii_uppercase())),
        },
        NfsOp::GidOfName { name } => NfsReply::Id {
            id: ids(system, job, |m| m.gid_of(&name.to_ascii_uppercase())),
        },
        NfsOp::Mapping => {
            // The state's guard goes before `domain`, which locks it again.
            let enabled = state().registry.ready(Daemon::Rgy) > 0;
            NfsReply::Mapping {
                enabled,
                domain: domain(system),
            }
        }
    }
}

/// The profile index, answered for `job`.
fn ids<T>(system: &mut System, job: &JobInfo, f: impl FnOnce(&dyn ifs::meta::IdMap) -> T) -> T {
    super::ifs::with_ids(system, job.handle, f)
}

/// The `NFSv4` domain: the options', else the TCP/IP domain name
/// (`CHGTCPDMN`), lower case.
pub fn domain(system: &mut System) -> String {
    let o = options(system).domain;
    let d = if o.trim().is_empty() {
        super::tcp::with(system, |_, rt| {
            tcpcfg::resolver::HostConfig::from_config(&rt.cfg).dmn_name
        })
        .unwrap_or_default()
    } else {
        o
    };
    d.trim().to_ascii_lowercase()
}

/// A daemon job ended without being asked to: `QSYSOPR` is told,
/// `CPFA1CB` (with the error number) for the binder, `CPFA1B5` for the
/// others.
fn ended_unasked(system: &mut System, daemon: Daemon, job: &str, errno: i32) {
    let (msgid, data) = if daemon == Daemon::Rpc {
        ("CPFA1CB", errno.to_be_bytes().to_vec())
    } else {
        ("CPFA1B5", Vec::new())
    };
    eprintln!("qsstg: nfs: {job} ({}) ended unasked", daemon.keyword());
    let text = super::msgs::text(system, msgid, &[&errno.to_string()]);
    let m = wm::obj::msgq::QMsg {
        msgid: msgid.into(),
        msgf: ("QCPFMSG".into(), "QSYS".into()),
        severity: 40,
        data,
        text,
        job: ("SCPF".into(), "QSYS".into(), 0),
        from_pgm: daemon.program().into(),
        ..wm::obj::msgq::QMsg::default()
    };
    let _ = super::msgs::send(system, "QSYS", "QSYSOPR", m);
}

/// Job `number` ended (`served::end_job`): a daemon job still in the
/// registry ended without saying so (it crashed or was killed), and one
/// not asked to end is reported to `QSYSOPR`.
pub fn job_ended(system: &mut System, number: u32, job: &str) {
    let gone = {
        let mut s = state();
        let d = s.registry.job(number);
        s.registry.gone(number);
        d
    };
    let asked = system
        .jobs
        .get(number)
        .is_some_and(|j| subsystem_ending(&system.sbs, &system.ended_sbs, &j.subsystem));
    if let Some((daemon, false)) = gone
        && !asked
    {
        ended_unasked(system, daemon, job, 0);
    }
}

/// Whether `sbs` is ending or has ended (`ENDSBS`, `ENDSYS`,
/// `PWRDWNSYS`): its jobs were asked to end, even those it had to end at
/// once.
fn subsystem_ending(
    active: &std::collections::BTreeMap<String, super::sbs::Active>,
    ended: &std::collections::BTreeSet<String>,
    sbs: &str,
) -> bool {
    active.get(sbs).is_some_and(|a| a.ending.is_some()) || ended.contains(sbs)
}

/// `CPFA1B8` for `what`.
fn iosyscfg(system: &mut System, what: &str) -> NfsReply {
    let data = ifscmd::Msg::new("CPFA1B8").chars(what, 20).data;
    refused(system, "CPFA1B8", data, &[what])
}

/// Submit the jobs of `daemon` that are not running: one, or `count`
/// for the server and block I/O daemons.
fn start(system: &mut System, daemon: Daemon, count: u16, rtvrpcreg: bool) -> NfsReply {
    if daemon == Daemon::Gss {
        let data = ifscmd::Msg::new("CPFA1BC").chars("*GSS", 8).data;
        return refused(system, "CPFA1BC", data, &["*GSS"]);
    }
    let running = u16::try_from(state().registry.count(daemon)).unwrap_or(u16::MAX);
    // A count of none asks how many there are.
    if count == 0 {
        return NfsReply::Started {
            submitted: 0,
            running,
        };
    }
    let want = if matches!(daemon, Daemon::Svr | Daemon::Bio) {
        count.clamp(1, 20)
    } else {
        1
    };
    let mut submitted = 0;
    for _ in running..want {
        match submit(system, daemon) {
            Ok(n) => {
                state().registry.submitted(daemon, n, rtvrpcreg);
                submitted += 1;
            }
            Err(e) => {
                eprintln!("slic: nfs: submit {}: {e}", daemon.job_name());
                let data = ifscmd::Msg::new("CPFA1BC").chars(daemon.keyword(), 8).data;
                return refused(system, "CPFA1BC", data, &[daemon.keyword()]);
            }
        }
    }
    NfsReply::Started { submitted, running }
}

/// Submit one daemon job to `QSYSNOMAX` with its routing data, as the
/// service's own request (as `QZLSSERVER` is).
fn submit(system: &mut System, daemon: Daemon) -> Result<u32, String> {
    let me = system.current_job;
    let r = wm::obj::jobq::SbmJob {
        job: daemon.job_name().into(),
        jobd: format!("QSYS/{}", daemon.jobd()),
        jobq: DAEMON_JOBQ.into(),
        user: DAEMON_USER.into(),
        rtgdta: daemon.job_name().into(),
        msgq: "*NONE".into(),
        inllibl: Some(Vec::new()),
        curlib: "*CRTDFT".into(),
        ..wm::obj::jobq::SbmJob::default()
    };
    let (number, _) = sec::guard::as_system(|| super::jobqs::sbmjob(system, me, &r))?;
    Ok(number)
}

/// The NFS options: read once from their file, the defaults without one.
pub fn options(system: &mut System) -> NfsOptions {
    if let Some(o) = state().options.clone() {
        return o;
    }
    let read = super::certs::as_system(system, |c| super::certs::read_file(c, optfile::PATH));
    let o = match read {
        Ok(Some(b)) => optfile::decode(&b).unwrap_or_else(|| {
            eprintln!("slic: nfs: {} is not in the options format", optfile::PATH);
            NfsOptions::default()
        }),
        _ => NfsOptions::default(),
    };
    state().options = Some(o.clone());
    o
}

/// Keep the NFS options.
pub fn save_options(system: &mut System, o: &NfsOptions) -> Result<(), String> {
    let b = optfile::encode(o);
    super::certs::as_system(system, |c| super::certs::write_file(c, optfile::PATH, &b))?;
    state().options = Some(o.clone());
    Ok(())
}

/// The anonymous user of an export: `ANON=-1` refuses, `ANON=uid` is
/// that UID, none is `QNFSANON`'s.
pub fn anon(system: &mut System, anon: Option<i64>) -> export::Anon {
    let none = || export::Anon {
        uid: None,
        profile: "*NONE".into(),
    };
    if anon == Some(-1) {
        return none();
    }
    let profiles = sec::guard::as_system(|| {
        super::spool::with_store(system, |txn, qsys| {
            let mut v = Vec::new();
            for n in sec::profile::objects::list(txn, qsys).map_err(|e| e.to_string())? {
                if let Ok((_, p)) = sec::profile::objects::read(txn, qsys, &n) {
                    v.push((p.name.clone(), p.uid));
                }
            }
            Ok(v)
        })
    })
    .unwrap_or_default();
    match anon {
        Some(uid) => {
            let uid = u32::try_from(uid).unwrap_or(0);
            export::Anon {
                uid: Some(uid),
                profile: profiles
                    .iter()
                    .find(|(_, u)| *u == uid)
                    .map_or_else(|| "*NONE".to_string(), |(n, _)| n.clone()),
            }
        }
        None => profiles
            .iter()
            .find(|(n, _)| n == "QNFSANON")
            .map_or_else(none, |(n, u)| export::Anon {
                uid: Some(*u),
                profile: n.clone(),
            }),
    }
}

/// Make the `/etc` files NFS ships that are missing
/// (`osobjects::nfsetc`), owned by `QSYS`, readable by everyone and
/// tagged with their CCSIDs.
pub fn ship_etc(system: &mut System) {
    use ifs::Errno;
    use ifs::ops::OpenArgs;
    use ifs::ops::flags::{O_CCSID, O_CREAT, O_EXCL, O_WRONLY};
    super::certs::as_system(system, |c| {
        for (path, ccsid, text) in osobjects::nfsetc::FILES {
            let fd = match c.open(
                path,
                OpenArgs::new(O_WRONLY | O_CREAT | O_EXCL | O_CCSID)
                    .mode(0o644)
                    .conv(*ccsid),
            ) {
                Ok(fd) => fd,
                Err(Errno::EEXIST) => continue,
                Err(e) => {
                    eprintln!("qsstg: {path}: {}", e.text());
                    continue;
                }
            };
            if let Err(e) = ifs::client::write_all(c, fd, text.as_bytes()) {
                eprintln!("qsstg: {path}: {}", e.text());
            }
            let _ = c.close(fd);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::super::sbs::Active;
    use super::subsystem_ending;

    /// A version 4 client's first owner made `QNFSNFSD` ask `Mapping`,
    /// whose answer held the state's lock while `domain` took it again:
    /// the storage service stopped for good. A watchdog ends the test
    /// process if the answer never comes.
    #[test]
    fn the_mapping_answer_does_not_lock_the_state_twice() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};
        let done = Arc::new(AtomicBool::new(false));
        let d2 = Arc::clone(&done);
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(20));
            if !d2.load(Ordering::SeqCst) {
                eprintln!("NfsOp::Mapping deadlocked");
                std::process::abort();
            }
        });
        super::state().options = Some(stg::nfs::NfsOptions::default());
        let mut system = crate::ipl::system::System::new(
            crate::platform::console::Outputs::open(),
            crate::ipl::cmdline::Boot::parse(""),
        );
        let r = super::request(
            &mut system,
            &wm::table::JobInfo::default(),
            &sec::auth::principal::Principal::default(),
            stg::nfs::NfsOp::Mapping,
        );
        done.store(true, Ordering::SeqCst);
        assert!(matches!(
            r,
            stg::nfs::NfsReply::Mapping { enabled: false, .. }
        ));
    }

    /// A power down ends `QSYSWRK` at once: its daemons' ends were asked
    /// for, so `QSYSOPR` gets no `CPFA1B5` (seven came at every IPL).
    #[test]
    fn a_subsystem_ending_asks_its_daemons_to_end() {
        let mut m = std::collections::BTreeMap::new();
        let mut a = Active {
            lib: "QSYS".into(),
            monitor: 9,
            ending: None,
            maxjobs: 0,
            pools: Vec::new(),
        };
        let mut gone = std::collections::BTreeSet::new();
        m.insert("QSYSWRK".to_string(), a.clone());
        assert!(!subsystem_ending(&m, &gone, "QSYSWRK"));
        a.ending = Some((true, 0));
        m.insert("QSYSWRK".to_string(), a);
        assert!(subsystem_ending(&m, &gone, "QSYSWRK"));
        assert!(!subsystem_ending(&m, &gone, ""));
        // Its monitor ended before the daemon's connection went.
        m.clear();
        gone.insert("QSYSWRK".to_string());
        assert!(subsystem_ending(&m, &gone, "QSYSWRK"));
    }
}
