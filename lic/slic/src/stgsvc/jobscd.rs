// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The job schedule in the service: the entry commands and the
//! scheduler's tick, which submits each entry that is due. An entry due
//! before this IPL started was missed while the system was down; its
//! `RCYACN` says what happens to it.

use store::obj::types;
use wm::cmd::Params;
use wm::obj::jobq::SbmJob;
use wm::obj::jobscd::{self, Frequency, JobScd, RcyAcn, SCHEDULE};

use crate::ipl::system::{System, now_unix};

/// Run `f` on the schedule; written back when `f` says it changed it.
fn with_scd<T>(
    system: &mut System,
    f: impl FnOnce(&mut JobScd, &mut System) -> Result<(T, bool), String>,
) -> Result<T, String> {
    let (lib, name) = SCHEDULE;
    let (id, mut scd) = {
        let s = system.storage.as_mut().ok_or("object store not open")?;
        let qsys = s.qsys.ok_or("object store not initialized")?;
        let mut txn = s.store.begin().map_err(|e| e.to_string())?;
        let names = crate::svc::ile::name_context(&mut txn, qsys);
        let r = wm::obj::find(&mut txn, qsys, &names, lib, name, types::JOBSCD)
            .and_then(|id| Ok((id, JobScd::decode(&wm::obj::read(&mut txn, id)?)?)))
            .map_err(|e| format!("CPF1629 job schedule {lib}/{name}: {e}"));
        txn.abort();
        r?
    };
    // The submissions `f` makes take their own transactions.
    let (r, changed) = f(&mut scd, system)?;
    if changed {
        let s = system.storage.as_mut().ok_or("object store not open")?;
        crate::svc::db::with_txn(&mut s.store, |txn| {
            wm::obj::write(txn, id, &scd.encode()).map_err(|e| e.to_string())
        })?;
    }
    Ok(r)
}

/// `ADDJOBSCDE`, `CHGJOBSCDE`, `RMVJOBSCDE`, `HLDJOBSCDE`,
/// `RLSJOBSCDE`, `WRKJOBSCDE`.
pub fn command(system: &mut System, me: u32, cmd: &str, line: &str) -> Result<String, String> {
    let p = Params::parse(line);
    let now = now_unix();
    let job = p
        .pos_or(0, "JOB")
        .map(|j| j.to_ascii_uppercase())
        .unwrap_or_default();
    let number: u32 = match p.upper("ENTRYNBR").as_deref() {
        None | Some("*ONLY") => 0,
        Some(n) => n
            .parse()
            .map_err(|_| format!("CPD0084 ENTRYNBR({n}) is not valid"))?,
    };
    let user = system
        .jobs
        .get(me)
        .map(|j| j.user.clone())
        .unwrap_or_default();
    let err = |e: wm::Error| format!("{} {e}", e.msgid().unwrap_or("CPD0084"));
    with_scd(system, |scd, _| match cmd {
        "addjobscde" => {
            qsname::Name::parse(&job).map_err(|e| format!("CPD0078 JOB({job}): {e}"))?;
            let mut e = jobscd::defaults(&job, scd.next_entry.max(1));
            e.user = user;
            let e = jobscd::apply(e, &p, now).map_err(err)?;
            let n = e.number;
            scd.next_entry = n + 1;
            scd.entries.push(e);
            Ok((
                format!("Job schedule entry {job} number {n:06} added.\n"),
                true,
            ))
        }
        "chgjobscde" => {
            let e = scd.find_mut(&job, number).map_err(err)?;
            *e = jobscd::apply(e.clone(), &p, now).map_err(err)?;
            Ok((
                format!("Job schedule entry {job} number {:06} changed.\n", e.number),
                true,
            ))
        }
        "rmvjobscde" => {
            let n = scd.find_mut(&job, number).map_err(err)?.number;
            scd.entries.retain(|e| e.number != n);
            Ok((
                format!("Job schedule entry {job} number {n:06} removed.\n"),
                true,
            ))
        }
        "hldjobscde" | "rlsjobscde" => {
            let hold = cmd == "hldjobscde";
            let e = scd.find_mut(&job, number).map_err(err)?;
            e.held = hold;
            if !hold && e.next != 0 && e.next <= now {
                e.next = e.next_after(now).unwrap_or(0);
            }
            let what = if hold { "held" } else { "released" };
            Ok((
                format!("Job schedule entry {job} number {:06} {what}.\n", e.number),
                true,
            ))
        }
        _ => Ok((scd.display(), false)),
    })
}

/// Submit every entry that is due; what was submitted, for the
/// scheduler's job log.
pub fn tick(system: &mut System, me: u32) -> Vec<String> {
    let now = now_unix();
    let ipl = system.start_time;
    let r = with_scd(system, |scd, system| {
        let mut out = Vec::new();
        let mut changed = false;
        let mut done = Vec::new();
        for e in &mut scd.entries {
            if e.held || e.next == 0 || e.next > now {
                continue;
            }
            changed = true;
            let missed = e.next < ipl;
            if missed && e.rcyacn == RcyAcn::NoSbm {
                e.next = e.next_after(now).unwrap_or(0);
                out.push(format!(
                    "Job schedule entry {} missed, not submitted.",
                    e.job
                ));
                continue;
            }
            let r = SbmJob {
                cmd: e.cmd.clone(),
                job: e.job.clone(),
                jobd: e.jobd.clone(),
                jobq: e.jobq.clone(),
                user: e.user.clone(),
                hold: (missed && e.rcyacn == RcyAcn::SbmHld).then_some(true),
                msgq: if e.msgq.is_empty() {
                    format!("QUSRSYS/{}", e.user)
                } else {
                    e.msgq.clone()
                },
                inllibl: Some(Vec::new()),
                curlib: "*CRTDFT".into(),
                ..SbmJob::default()
            };
            match super::jobqs::sbmjob(system, me, &r) {
                Ok((n, _)) => {
                    let user = if e.user.is_empty() { "QSYS" } else { &e.user };
                    e.last_job = format!("{n:06}/{user}/{}", e.job);
                    e.last_submit = now;
                    let data = wm::joblog::data(&[
                        (&e.job, 10),
                        (&format!("{:06}", e.number), 6),
                        (&format!("{n:06}"), 6),
                        (user, 10),
                    ]);
                    let who = super::msgs::sender(system, me);
                    super::hist::system_message(system, "CPI1E63", &data, "01", who);
                    out.push(format!(
                        "Job {n:06}/{user}/{} submitted for schedule entry {:06}.",
                        e.job, e.number
                    ));
                }
                Err(m) => out.push(format!("Job schedule entry {} not submitted: {m}", e.job)),
            }
            e.next = if e.frequency == Frequency::Once {
                0
            } else {
                e.next_after(now).unwrap_or(0)
            };
            if e.next == 0 && !e.save {
                done.push(e.number);
            }
        }
        scd.entries.retain(|e| !done.contains(&e.number));
        Ok((out, changed))
    });
    r.unwrap_or_else(|e| vec![e])
}
