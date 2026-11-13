// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `STRNFSSVR`, `ENDNFSSVR`, `RPCBIND` and `ENDRPCBIND` (docs/NFS.md
//! section 4): the storage service submits the daemon jobs and keeps
//! their registry (`stg::nfs`); the command asks for each type in
//! IBM's order and waits for it to report ready, or to leave the
//! registry when it ends, within the command's time-out. Its messages
//! go to the job log: `CPIA1BB`/`CPIA1BC` a type started or ended,
//! `CPDA1BA`/`CPDA1BD` one running already, `CPCA1BB`/`CPCA1BC` the
//! command done, `CPFA1B9`/`CPFA1BD` the time-out passed,
//! `CPFA1BC`/`CPFA1BE` a daemon that did not start or end; the binder's
//! own `CPCA1B4`-`CPCA1B6`, `CPFA1C9` and `CPFA1CA`.

use std::time::Duration;

use cmd::Resolved;
use ifscmd::Msg;
use mi::Job;
use qir::MsgType;
use stg::Client;
use stg::nfs::{Daemon, NfsOp, NfsReply};

use super::{Fail, log, parms_of};
use crate::console::host::Host;

/// How long a binder start or end waits (`RPCBIND` has no time-out).
const BINDER_WAIT: u32 = 30;
/// One wait between looks at the registry, in milliseconds.
pub const PAUSE_MS: u64 = 250;
/// How long a submitted job may take to say hello before a look that
/// finds none of its type means it ended.
const HELLO_MS: u64 = 1500;

/// What the commands need of the job, so they run over a double.
pub trait Env {
    /// One request to the service.
    fn op(&mut self, op: NfsOp) -> NfsReply;
    /// A message to the job log.
    fn log(&mut self, m: &Msg, kind: MsgType);
    /// Wait [`PAUSE_MS`]; `false` when the job is to end.
    fn pause(&mut self) -> bool;
}

/// A message as the command's escape.
fn fail(m: Msg) -> Fail {
    Fail {
        msgid: m.id,
        data: m.data,
        text: String::new(),
    }
}

/// Jobs of a type the registry has: (registered, ready, ending).
fn count(env: &mut dyn Env, d: Daemon) -> (usize, usize) {
    match env.op(NfsOp::Daemons) {
        NfsReply::Daemons { jobs } => {
            let of: Vec<_> = jobs.iter().filter(|j| j.daemon == d).collect();
            (of.len(), of.iter().filter(|j| j.ready && !j.ending).count())
        }
        _ => (0, 0),
    }
}

/// A refusal from the service as the escape.
fn refused(r: NfsReply) -> Option<Fail> {
    match r {
        NfsReply::Refused { msgid, data, text } => Some(Fail { msgid, data, text }),
        _ => None,
    }
}

/// Start `want` jobs of `d` and wait for them; `Ok(true)` when they
/// were running already.
fn start_one(
    env: &mut dyn Env,
    d: Daemon,
    want: u16,
    rtvrpcreg: bool,
    timeout: Option<u32>,
) -> Result<bool, Fail> {
    let (_, ready) = count(env, d);
    let want = usize::from(want.max(1));
    if ready >= want {
        return Ok(true);
    }
    let r = env.op(NfsOp::Start {
        daemon: d,
        count: u16::try_from(want).unwrap_or(1),
        rtvrpcreg,
    });
    let running = match r {
        NfsReply::Started { running, .. } => usize::from(running),
        other => {
            return Err(
                refused(other).unwrap_or_else(|| fail(Msg::new("CPFA1BC").chars(d.keyword(), 8)))
            );
        }
    };
    // Those running already: one diagnostic each, as IBM's.
    for n in 1..=running.min(want) {
        if want > 1 {
            env.log(
                &Msg::new("CPDA1BD")
                    .chars(d.keyword(), 4)
                    .bin4(i64::try_from(n).unwrap_or(0))
                    .bin4(i64::try_from(want).unwrap_or(0)),
                MsgType::Diag,
            );
        }
    }
    let mut waited = 0u64;
    loop {
        let (registered, ready) = count(env, d);
        if ready >= want {
            return Ok(false);
        }
        // Started and gone again without reporting ready.
        if registered == 0 && waited >= HELLO_MS && !queued(env, d) {
            return Err(fail(Msg::new("CPFA1BC").chars(d.keyword(), 8)));
        }
        if let Some(t) = timeout
            && waited >= u64::from(t) * 1000
        {
            return Err(fail(
                Msg::new("CPFA1B9").chars(d.keyword(), 4).bin4(i64::from(t)),
            ));
        }
        if !env.pause() {
            return Err(fail(Msg::new("CPFA1BC").chars(d.keyword(), 8)));
        }
        waited += PAUSE_MS;
    }
}

/// Whether jobs of `d` were submitted and have not said hello: a start
/// with none submitted now and none registered means they are gone.
fn queued(env: &mut dyn Env, d: Daemon) -> bool {
    // A Start of no new jobs answers how many the service counts,
    // submitted ones included.
    matches!(
        env.op(NfsOp::Start { daemon: d, count: 0, rtvrpcreg: false }),
        NfsReply::Started { running, .. } if running > 0
    )
}

/// End the jobs of `d` and wait for them to leave the registry.
fn end_one(env: &mut dyn Env, d: Daemon, timeout: Option<u32>) -> Result<bool, Fail> {
    if count(env, d).0 == 0 && !queued(env, d) {
        return Ok(false);
    }
    if let Some(f) = refused(env.op(NfsOp::End { daemon: d })) {
        return Err(f);
    }
    let mut waited = 0u64;
    loop {
        if count(env, d).0 == 0 {
            return Ok(true);
        }
        if let Some(t) = timeout
            && waited >= u64::from(t) * 1000
        {
            return Err(fail(
                Msg::new("CPFA1BD").chars(d.keyword(), 4).bin4(i64::from(t)),
            ));
        }
        if !env.pause() {
            return Err(fail(Msg::new("CPFA1BE")));
        }
        waited += PAUSE_MS;
    }
}

/// `STRNFSSVR SERVER(..) NBRSVR NBRBIO RTVRPCREG STRJOBTIMO`.
pub fn strnfssvr(
    env: &mut dyn Env,
    server: i64,
    nbrsvr: u16,
    nbrbio: u16,
    rtvrpcreg: bool,
    timeout: Option<u32>,
) -> Result<(), Fail> {
    let list: Vec<Daemon> = match Daemon::from_mapped(server) {
        Some(Daemon::Gss) => {
            env.log(&Msg::new("CPI9C10"), MsgType::Info);
            return Err(fail(Msg::new("CPFA1BC").chars("*GSS", 8)));
        }
        Some(d) if d.needs_binder() => vec![Daemon::Rpc, d],
        Some(d) => vec![d],
        None => Daemon::ALL.to_vec(),
    };
    for d in list {
        let want = match d {
            Daemon::Svr => nbrsvr,
            Daemon::Bio => nbrbio,
            _ => 1,
        };
        // The binder a daemon needs is started quietly when it is up.
        let implicit =
            d == Daemon::Rpc && Daemon::from_mapped(server) != Some(Daemon::Rpc) && server != -1;
        let already = start_one(env, d, want, rtvrpcreg, timeout)?;
        if implicit {
            continue;
        }
        if already {
            if want > 1 {
                for n in 1..=want {
                    env.log(
                        &Msg::new("CPDA1BD")
                            .chars(d.keyword(), 4)
                            .bin4(i64::from(n))
                            .bin4(i64::from(want)),
                        MsgType::Diag,
                    );
                }
            } else {
                env.log(&Msg::new("CPDA1BA").chars(d.keyword(), 8), MsgType::Diag);
            }
        } else {
            env.log(&Msg::new("CPIA1BB").chars(d.keyword(), 4), MsgType::Info);
        }
    }
    env.log(&Msg::new("CPCA1BB"), MsgType::Comp);
    Ok(())
}

/// `ENDNFSSVR SERVER(..) ENDJOBTIMO`.
pub fn endnfssvr(env: &mut dyn Env, server: i64, timeout: Option<u32>) -> Result<(), Fail> {
    let list: Vec<Daemon> = match Daemon::from_mapped(server) {
        Some(Daemon::Gss) => {
            env.log(&Msg::new("CPI9C10"), MsgType::Info);
            return Err(fail(Msg::new("CPFA1BE")));
        }
        Some(d) => vec![d],
        None => Daemon::ALL.iter().rev().copied().collect(),
    };
    for d in list {
        if end_one(env, d, timeout)? || server != -1 {
            env.log(&Msg::new("CPIA1BC").chars(d.keyword(), 4), MsgType::Info);
        }
    }
    env.log(&Msg::new("CPCA1BC"), MsgType::Comp);
    Ok(())
}

/// `RPCBIND RTVRPCREG`.
pub fn rpcbind(env: &mut dyn Env, rtvrpcreg: bool) -> Result<(), Fail> {
    match start_one(env, Daemon::Rpc, 1, rtvrpcreg, Some(BINDER_WAIT)) {
        Ok(true) => env.log(&Msg::new("CPCA1B6"), MsgType::Comp),
        Ok(false) => env.log(&Msg::new("CPCA1B4"), MsgType::Comp),
        Err(f) if f.msgid == "CPFA1B8" => return Err(f),
        Err(_) => return Err(fail(Msg::new("CPFA1C9"))),
    }
    Ok(())
}

/// `ENDRPCBIND`.
pub fn endrpcbind(env: &mut dyn Env) -> Result<(), Fail> {
    match end_one(env, Daemon::Rpc, Some(BINDER_WAIT)) {
        Ok(_) => env.log(&Msg::new("CPCA1B5"), MsgType::Comp),
        Err(f) if f.msgid == "CPFA1B8" => return Err(f),
        Err(_) => return Err(fail(Msg::new("CPFA1CA"))),
    }
    Ok(())
}

/// The job as the commands' environment.
struct JobEnv<'a> {
    job: &'a mut Job,
    host: &'a Host,
}

impl Env for JobEnv<'_> {
    fn op(&mut self, op: NfsOp) -> NfsReply {
        self.host.link.borrow_mut().with(|l| l.nfs(op))
    }
    fn log(&mut self, m: &Msg, kind: MsgType) {
        log(self.job, &m.id, &m.data, kind, None);
    }
    fn pause(&mut self) -> bool {
        self.host.idle(self.job, Duration::from_millis(PAUSE_MS))
    }
}

/// Run one of the commands in the job.
pub fn run(job: &mut Job, host: &Host, r: &Resolved) -> Result<String, Fail> {
    let p = parms_of(r);
    let int = |k: &str, d: i64| p.int(k).unwrap_or(d);
    let yes = |k: &str| matches!(p.name(k).as_str(), "Y" | "*YES");
    let secs = |k: &str| match int(k, 30) {
        -1 => None,
        n => Some(u32::try_from(n).unwrap_or(30)),
    };
    let count = |k: &str| u16::try_from(int(k, 1)).unwrap_or(1);
    let mut env = JobEnv { job, host };
    match r.name.as_str() {
        "STRNFSSVR" => strnfssvr(
            &mut env,
            int("SERVER", -1),
            count("NBRSVR"),
            count("NBRBIO"),
            yes("RTVRPCREG"),
            secs("STRJOBTIMO"),
        ),
        "ENDNFSSVR" => endnfssvr(&mut env, int("SERVER", -1), secs("ENDJOBTIMO")),
        "RPCBIND" => rpcbind(&mut env, yes("RTVRPCREG")),
        _ => endrpcbind(&mut env),
    }
    .map(|()| String::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use stg::nfs::NfsDaemonJob;

    /// A registry in memory whose daemons become ready (or never do)
    /// after a few pauses.
    struct Fake {
        jobs: Vec<NfsDaemonJob>,
        /// Submitted, ready after this many pauses.
        coming: Vec<(Daemon, u32)>,
        /// Daemons that start and end without reporting ready.
        broken: Vec<Daemon>,
        log: Vec<String>,
        iosyscfg: bool,
    }

    impl Fake {
        fn new() -> Self {
            Self {
                jobs: Vec::new(),
                coming: Vec::new(),
                broken: Vec::new(),
                log: Vec::new(),
                iosyscfg: true,
            }
        }
        fn running(&mut self, d: Daemon) {
            self.jobs.push(NfsDaemonJob {
                daemon: d,
                index: 1,
                job: d.job_name().into(),
                ready: true,
                ending: false,
                ports: Vec::new(),
                since: 0,
            });
        }
    }

    impl Env for Fake {
        fn op(&mut self, op: NfsOp) -> NfsReply {
            match op {
                NfsOp::Daemons => NfsReply::Daemons {
                    jobs: self.jobs.clone(),
                },
                NfsOp::Start { .. } | NfsOp::End { .. } if !self.iosyscfg => NfsReply::Refused {
                    msgid: "CPFA1B8".into(),
                    data: Vec::new(),
                    text: String::new(),
                },
                NfsOp::Start { daemon, count, .. } => {
                    let have = self.jobs.iter().filter(|j| j.daemon == daemon).count()
                        + self.coming.iter().filter(|(d, _)| *d == daemon).count();
                    let have = u16::try_from(have).unwrap_or(0);
                    for _ in have..count {
                        if !self.broken.contains(&daemon) {
                            self.coming.push((daemon, 2));
                        }
                    }
                    NfsReply::Started {
                        submitted: count.saturating_sub(have),
                        running: have,
                    }
                }
                NfsOp::End { daemon } => {
                    for j in &mut self.jobs {
                        if j.daemon == daemon {
                            j.ending = true;
                        }
                    }
                    NfsReply::Done
                }
                _ => NfsReply::Done,
            }
        }
        fn log(&mut self, m: &Msg, _: MsgType) {
            self.log.push(m.id.clone());
        }
        fn pause(&mut self) -> bool {
            let mut now = Vec::new();
            for c in &mut self.coming {
                c.1 = c.1.saturating_sub(1);
                if c.1 == 0 {
                    now.push(c.0);
                }
            }
            self.coming.retain(|c| c.1 > 0);
            for d in now {
                self.running(d);
            }
            // Ending jobs leave.
            self.jobs.retain(|j| !j.ending);
            true
        }
    }

    #[test]
    fn every_daemon_in_order() {
        let mut f = Fake::new();
        strnfssvr(&mut f, -1, 2, 1, false, Some(30)).unwrap();
        assert_eq!(f.log.iter().filter(|m| *m == "CPIA1BB").count(), 7);
        assert_eq!(f.log.last().map(String::as_str), Some("CPCA1BB"));
        let order: Vec<Daemon> = f.jobs.iter().map(|j| j.daemon).collect();
        assert_eq!(order.first(), Some(&Daemon::Rpc));
        assert_eq!(f.jobs.iter().filter(|j| j.daemon == Daemon::Svr).count(), 2);
        // Again: every type is running already.
        f.log.clear();
        strnfssvr(&mut f, -1, 2, 1, false, Some(30)).unwrap();
        assert!(f.log.contains(&"CPDA1BA".to_string()));
        assert!(f.log.contains(&"CPDA1BD".to_string()));
        endnfssvr(&mut f, -1, Some(30)).unwrap();
        assert!(f.jobs.is_empty());
        assert_eq!(f.log.last().map(String::as_str), Some("CPCA1BC"));
    }

    #[test]
    fn the_binder_comes_first() {
        let mut f = Fake::new();
        strnfssvr(&mut f, -7, 1, 1, false, Some(30)).unwrap();
        assert_eq!(f.jobs.len(), 2);
        // The binder started quietly: one CPIA1BB, for *NLM.
        assert_eq!(f.log, ["CPIA1BB", "CPCA1BB"]);
    }

    #[test]
    fn refusals() {
        let mut f = Fake::new();
        assert_eq!(
            strnfssvr(&mut f, -8, 1, 1, false, Some(5))
                .unwrap_err()
                .msgid,
            "CPFA1BC"
        );
        assert_eq!(f.log, ["CPI9C10"]);
        assert_eq!(endnfssvr(&mut f, -8, Some(5)).unwrap_err().msgid, "CPFA1BE");
        let mut f = Fake::new();
        f.broken.push(Daemon::Svr);
        f.running(Daemon::Rpc);
        assert_eq!(
            strnfssvr(&mut f, -4, 1, 1, false, Some(5))
                .unwrap_err()
                .msgid,
            "CPFA1BC"
        );
        let mut f = Fake::new();
        f.iosyscfg = false;
        assert_eq!(rpcbind(&mut f, false).unwrap_err().msgid, "CPFA1B8");
    }

    #[test]
    fn the_binder_alone() {
        let mut f = Fake::new();
        rpcbind(&mut f, true).unwrap();
        assert_eq!(f.log, ["CPCA1B4"]);
        rpcbind(&mut f, true).unwrap();
        assert_eq!(f.log, ["CPCA1B4", "CPCA1B6"]);
        endrpcbind(&mut f).unwrap();
        assert!(f.jobs.is_empty());
        assert_eq!(f.log.last().map(String::as_str), Some("CPCA1B5"));
    }
}
