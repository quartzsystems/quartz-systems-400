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
const TICK: Duration = Duration::from_millis(500);

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
        std::thread::sleep(TICK);
        m.reap();
        let state = m.link.borrow_mut().with(Client::sbs_state);
        if let Some((immediate, delay)) = state.ending {
            m.end_all(immediate, delay);
            let _ = ctl.send(&FromJob::Ending {
                reason: format!("subsystem {} ended", m.name),
            });
            std::process::exit(0);
        }
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
            cgroup::ensure(&pool, size);
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
        let r = self.sbsd.route(&d.rtgdta).cloned().ok_or_else(|| {
            format!(
                "CPF1226 no routing entry in {} for '{}'",
                self.name, d.rtgdta
            )
        })?;
        let kind_of = self.sbsd.pool(r.pool).map_or(PoolKind::Base, |p| p.kind);
        let number = self.link.borrow_mut().with(Client::job_number)?;
        let class = if r.cls.0.starts_with('*') {
            "QGPL/QBATCH".to_string()
        } else {
            format!("{}/{}", r.cls.1, r.cls.0)
        };
        let user = if d.user.starts_with('*') {
            "QSYS".to_string()
        } else {
            d.user.clone()
        };
        let start = Start {
            name: job.to_string(),
            user,
            number,
            handle: u64::from(number),
            kind,
            subsystem: self.name.clone(),
            role: "BATCH".into(),
            jobd: jobd.to_string(),
            class,
            program: format!("{}/{}", r.pgm.1, r.pgm.0),
            rqsdta: if d.rqsdta == "*NONE" {
                String::new()
            } else {
                d.rqsdta.clone()
            },
            pool: pool_name(&self.name, r.pool, kind_of),
            lda: lda.to_vec(),
        };
        let mut s = spawn::job(start)?;
        spawn::ready(&mut s)?;
        self.jobs.push(s);
        Ok(number)
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
