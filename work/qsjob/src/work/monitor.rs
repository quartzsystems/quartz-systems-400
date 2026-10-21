// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The subsystem monitor: one per active subsystem, job type `M`, job
//! name the subsystem's. It registers the subsystem with the storage
//! service, makes its pools, starts its autostart jobs, then watches:
//! its jobs end, and it ends them all when the subsystem is to end.

use std::time::{Duration, Instant};

use ipc::chan::Chan;
use stg::{Client, Shared};
use wm::ctl::{FromJob, Start, ToJob};
use wm::obj::jobd::JobD;
use wm::obj::sbsd::{PoolKind, Sbsd};

use super::cgroup;
use super::spawn::{self, Spawned};

/// How often the monitor looks around.
const TICK: Duration = Duration::from_millis(100);

/// The monitor's state.
pub struct Monitor {
    /// The subsystem.
    pub name: String,
    /// Its description.
    pub sbsd: Sbsd,
    link: Shared,
    /// Its jobs.
    pub jobs: Vec<Spawned>,
}

/// The storage pool a subsystem pool is.
fn pool_name(sbs: &str, id: u8, kind: PoolKind) -> String {
    match kind {
        PoolKind::Private => format!("{sbs}/{id}"),
        k => k.name(),
    }
}

/// Run the monitor. Never returns.
pub fn run(mut ctl: Chan, link: &Shared, start: &Start) -> ! {
    let (lib, name) = start
        .rqsdta
        .split_once('/')
        .map_or(("QSYS".to_string(), start.rqsdta.clone()), |(l, n)| {
            (l.to_string(), n.to_string())
        });
    let registered = link
        .borrow_mut()
        .with(|l| l.sbs_register(name.clone(), lib.clone()));
    let sbsd = match registered.map(|b| Sbsd::decode(&b)) {
        Ok(Ok(d)) => d,
        Ok(Err(e)) => crate::start::end(&mut ctl, &e.to_string()),
        Err(e) => crate::start::end(&mut ctl, &e),
    };
    let mut m = Monitor {
        name,
        sbsd,
        link: link.clone(),
        jobs: Vec::new(),
    };
    m.pools();
    for a in m.sbsd.ajes.clone() {
        let jobd = format!("{}/{}", a.jobd.1, a.jobd.0);
        if let Err(e) = m.start_routed(&a.job, &jobd, 'A', &[]) {
            eprintln!("qsjob: {}: autostart job {}: {e}", m.name, a.job);
        }
    }
    loop {
        stg::remote::status(&m.link, wm::job::Status::Deqw, "");
        std::thread::sleep(TICK);
        m.reap();
        let state = m.link.borrow_mut().with(Client::sbs_state);
        m.end_requested(&state.end_jobs);
        if let Some((immediate, delay)) = state.ending {
            stg::remote::status(&m.link, wm::job::Status::Deqw, "END-SBS");
            m.end_all(immediate, delay);
            let _ = ctl.send(&FromJob::Ending {
                reason: format!("subsystem {} ended", m.name),
            });
            std::process::exit(0);
        }
        m.start_from_queues();
    }
}

impl Monitor {
    /// Define the private pools, make every pool's cgroup.
    fn pools(&mut self) {
        let shared = self.link.borrow_mut().with(Client::pools);
        for p in self.sbsd.pools.clone() {
            let pool = pool_name(&self.name, p.id, p.kind);
            let size = if p.kind == PoolKind::Private {
                self.link
                    .borrow_mut()
                    .with(|l| l.define_pool(pool.clone(), p.size_kb, p.actlvl));
                p.size_kb
            } else {
                shared
                    .iter()
                    .find(|i| i.name == pool)
                    .map_or(0, |i| i.size_kb)
            };
            // The pools the system manages hold what the others leave
            // (QBASPOOL is *BASE's minimum, not its cap): not limited.
            let managed = matches!(pool.as_str(), "*MACHINE" | "*BASE" | "*INTERACT" | "*SPOOL");
            cgroup::ensure(&pool, if managed { 0 } else { size });
        }
    }

    /// Start a job routed by its job description's routing data.
    pub fn start_routed(
        &mut self,
        job: &str,
        jobd: &str,
        kind: char,
        lda: &[u8],
    ) -> Result<u32, String> {
        let b = self
            .link
            .borrow_mut()
            .with(|l| l.wm_object(jobd.to_string(), "*JOBD".into()))?;
        let d = JobD::decode(&b).map_err(|e| e.to_string())?;
        let number = self.link.borrow_mut().with(Client::job_number)?;
        let user = if d.user.starts_with('*') {
            "QSYS".to_string()
        } else {
            d.user.clone()
        };
        let rqsdta = if d.rqsdta == "*NONE" {
            String::new()
        } else {
            d.rqsdta.clone()
        };
        let routed = Routed {
            name: job.to_string(),
            user,
            number,
            kind,
            jobd: jobd.to_string(),
            rtgdta: d.rtgdta,
            rqsdta,
            lda: lda.to_vec(),
        };
        self.start(routed).map(|_| number)
    }

    /// Start a job taken from a job queue: numbered when it was submitted.
    fn start_queued(&mut self, q: &str, j: wm::obj::jobq::QJob) -> Result<(), String> {
        let routed = Routed {
            name: j.name,
            user: j.user,
            number: j.number,
            // A print writer submitted by STRPRTWTR is a writer job.
            kind: if j.rtgdta == "PRTWTR" { 'W' } else { 'B' },
            jobd: j.jobd,
            rtgdta: j.rtgdta,
            rqsdta: j.rqsdta,
            lda: j.lda,
        };
        let pty = j.jobpty;
        let s = self.start(routed)?;
        s.jobq = q.to_string();
        s.jobpty = pty;
        Ok(())
    }

    /// Route a job by its routing data and start it.
    fn start(&mut self, j: Routed) -> Result<&mut Spawned, String> {
        let r = self.sbsd.route(&j.rtgdta).cloned().ok_or_else(|| {
            format!(
                "CPF1226 no routing entry in {} for '{}'",
                self.name, j.rtgdta
            )
        })?;
        let kind_of = self.sbsd.pool(r.pool).map_or(PoolKind::Base, |p| p.kind);
        let class = if r.cls.0.starts_with('*') {
            "QGPL/QBATCH".to_string()
        } else {
            format!("{}/{}", r.cls.1, r.cls.0)
        };
        let start = Start {
            name: j.name,
            user: j.user,
            number: j.number,
            handle: u64::from(j.number),
            kind: j.kind,
            subsystem: self.name.clone(),
            role: "BATCH".into(),
            jobd: j.jobd,
            class,
            program: format!("{}/{}", r.pgm.1, r.pgm.0),
            rqsdta: j.rqsdta,
            pool: pool_name(&self.name, r.pool, kind_of),
            lda: j.lda,
        };
        let mut s = spawn::job(start)?;
        spawn::ready(&mut s)?;
        self.jobs.push(s);
        Ok(self.jobs.last_mut().expect("pushed"))
    }

    /// Start what the job queues have, within each entry's `MAXACT` and
    /// `MAXPTYn`.
    fn start_from_queues(&mut self) {
        let mut entries = self.sbsd.jobqes.clone();
        entries.sort_by_key(|e| e.seq);
        for e in entries {
            let q = format!("{}/{}", e.jobq.1, e.jobq.0);
            loop {
                let mine: Vec<u8> = self
                    .jobs
                    .iter()
                    .filter(|s| s.jobq == q)
                    .map(|s| s.jobpty)
                    .collect();
                if e.maxact != 0 && mine.len() >= usize::from(e.maxact) {
                    break;
                }
                let mut per = [0usize; 10];
                for p in &mine {
                    per[usize::from(*p).min(9)] += 1;
                }
                let allowed: Vec<bool> = (0..10usize)
                    .map(|p| {
                        if p == 0 {
                            return true;
                        }
                        let max = e.maxpty[p - 1];
                        max == 0 || per[p] < usize::from(max)
                    })
                    .collect();
                let taken = self
                    .link
                    .borrow_mut()
                    .with(|l| l.jobq_take(q.clone(), allowed));
                let Some(j) = taken else {
                    break;
                };
                let name = j.qualified();
                stg::remote::status(
                    &self.link,
                    wm::job::Status::Deqw,
                    &format!("START-{}", j.name),
                );
                if let Err(err) = self.start_queued(&q, j) {
                    eprintln!("qsjob: {}: job {name} from {q}: {err}", self.name);
                    break;
                }
            }
        }
    }

    /// Pass `ENDJOB` on to the jobs it names.
    fn end_requested(&mut self, which: &[(u32, bool)]) {
        for (n, immediate) in which {
            if let Some(s) = self.jobs.iter_mut().find(|s| s.start.number == *n) {
                let _ = s.chan.send(&ToJob::End {
                    immediate: *immediate,
                });
            }
        }
    }

    /// Forget the jobs that ended.
    pub fn reap(&mut self) {
        self.jobs
            .retain_mut(|s| matches!(s.child.try_wait(), Ok(None)));
    }

    /// End every job: a controlled end asks them and waits `delay`
    /// seconds (0 waits as long as they take); then, or at once for
    /// `*IMMED`, the immediate end, and after `QENDJOBLMT` the kill.
    fn end_all(&mut self, immediate: bool, delay: u32) {
        if !immediate {
            for s in &mut self.jobs {
                let _ = s.chan.send(&ToJob::End { immediate: false });
            }
            let until = (delay > 0).then(|| Instant::now() + Duration::from_secs(u64::from(delay)));
            while !self.jobs.is_empty() && until.is_none_or(|t| Instant::now() < t) {
                std::thread::sleep(TICK);
                self.reap();
            }
        }
        for s in &mut self.jobs {
            let _ = s.chan.send(&ToJob::End { immediate: true });
        }
        let limit = self
            .link
            .borrow_mut()
            .with(|l| l.sysval("QENDJOBLMT".into()))
            .and_then(|v| ccsid::from_ebcdic(&v.data).trim().parse::<u64>().ok())
            .unwrap_or(120)
            .min(120);
        let until = Instant::now() + Duration::from_secs(limit);
        while !self.jobs.is_empty() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(100));
            self.reap();
        }
        for s in &mut self.jobs {
            let _ = s.child.kill();
            let _ = s.child.wait();
        }
        self.jobs.clear();
        for p in self.sbsd.pools.clone() {
            cgroup::remove(&pool_name(&self.name, p.id, p.kind));
        }
    }
}

/// What starting a job needs once it is chosen.
struct Routed {
    name: String,
    user: String,
    number: u32,
    kind: char,
    jobd: String,
    rtgdta: String,
    rqsdta: String,
    lda: Vec<u8>,
}
