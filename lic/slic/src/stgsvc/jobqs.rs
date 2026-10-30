// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Job queues in the service: `SBMJOB`, the jobs waiting on each queue,
//! which subsystem holds each queue, and handing the next job to a
//! monitor. A job is numbered when it is submitted, as on the machine,
//! so its number is taken while it waits.

use std::fmt::Write as _;

use store::obj::{JobHandle, ObjectId, ops, types};
use wm::job::JobAttrs;
use wm::obj::jobd::JobD;
use wm::obj::jobq::{JobQ, QJob, SbmJob};
use wm::obj::params::qualified;

use crate::ipl::system::{System, now_unix};

/// `LIB/NAME`, upper case, a missing library as `*LIBL`.
pub fn qual(s: &str) -> String {
    let (l, n) = qualified(s);
    format!("{l}/{n}")
}

/// Run `f` on a queue (`LIB/NAME` or `*LIBL/NAME`); written back when
/// `f` says it changed it. The answer has the queue's real library.
pub fn with_jobq<T>(
    system: &mut System,
    q: &str,
    f: impl FnOnce(&mut JobQ, &str) -> Result<(T, bool), String>,
) -> Result<T, String> {
    let (lib, name) = qualified(q);
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    crate::svc::db::with_txn(&mut s.store, |txn| {
        let names = crate::svc::ile::name_context(txn, qsys);
        let id = wm::obj::find(txn, qsys, &names, &lib, &name, types::JOBQ)
            .map_err(|_| format!("CPF3307 job queue {name} in {lib} not found"))?;
        let real = format!(
            "{}/{name}",
            ile::store::library_name(txn, id).map_err(|e| e.to_string())?
        );
        let mut jq = JobQ::decode(&wm::obj::read(txn, id).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let (r, changed) = f(&mut jq, &real)?;
        if changed {
            wm::obj::write(txn, id, &jq.encode()).map_err(|e| e.to_string())?;
        }
        Ok(r)
    })
}

/// Every job queue: `LIB/NAME` and its object.
fn all_queues(system: &mut System) -> Result<Vec<(String, ObjectId)>, String> {
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    let mut txn = s.store.begin().map_err(|e| e.to_string())?;
    let mut libs = vec![("QSYS".to_string(), qsys)];
    for (k, id) in ops::list(&mut txn, qsys, Some(types::LIB)).map_err(|e| e.to_string())? {
        libs.push((k.name.to_string(), id));
    }
    let mut out = Vec::new();
    for (lname, lid) in libs {
        for (k, id) in ops::list(&mut txn, lid, Some(types::JOBQ)).map_err(|e| e.to_string())? {
            out.push((format!("{lname}/{}", k.name), id));
        }
    }
    txn.abort();
    Ok(out)
}

/// At start: the numbers of the jobs that wait on queues.
pub fn load(system: &mut System) {
    let Ok(queues) = all_queues(system) else {
        return;
    };
    for (q, _) in queues {
        if let Ok(nums) = with_jobq(system, &q, |jq, _| {
            Ok((jq.jobs.iter().map(|j| j.number).collect::<Vec<_>>(), false))
        }) {
            for n in nums {
                system.queued.insert(n, q.clone());
            }
        }
    }
}

/// The queues a subsystem's monitor holds, when no other active
/// subsystem holds them already.
pub fn attach(system: &mut System, sbs: &str, queues: &[String]) {
    for q in queues {
        let q = qual(q);
        system
            .jobq_attach
            .entry(q)
            .or_insert_with(|| sbs.to_string());
    }
}

/// A subsystem ended: its queues are free.
pub fn detach(system: &mut System, sbs: &str) {
    system.jobq_attach.retain(|_, s| s != sbs);
}

fn sv(system: &System) -> impl Fn(&str) -> String + '_ {
    move |name: &str| {
        system
            .sysvals
            .as_ref()
            .and_then(|v| v.get(name).ok())
            .map(|v| v.text())
            .unwrap_or_default()
    }
}

/// The user `SBMJOB` runs the job as: `USER`, or the submitter.
fn sbm_user(r: &SbmJob, me: &str) -> String {
    if r.user.is_empty() || r.user == "*CURRENT" {
        me.to_string()
    } else {
        r.user.clone()
    }
}

/// `SBMJOB`: the job's number and the message to the submitter.
pub fn sbmjob(system: &mut System, me: u32, r: &SbmJob) -> Result<(u32, String), String> {
    let mine = system.attrs.get(&me).cloned().unwrap_or_default();
    let me_info = system.jobs.get(me).cloned().unwrap_or_default();
    let jobd = if r.jobd.is_empty() {
        "QGPL/QDFTJOBD".to_string()
    } else {
        qual(&r.jobd)
    };
    let b = super::sbs::wm_object(system, &jobd, "*JOBD")?;
    let d = JobD::decode(&b).map_err(|e| e.to_string())?;
    let user = sbm_user(r, &me_info.user);
    let prf = super::jobs::user_dft(system, &user);
    let mut attrs = JobAttrs::resolve_for(
        &jobd,
        &d,
        &wm::obj::cls::Cls::default(),
        &sv(system),
        &prf,
    );
    match &r.inllibl {
        None => attrs.libl.usr.clone_from(&mine.libl.usr),
        Some(l) if l.is_empty() => {}
        Some(l) => attrs.libl.usr.clone_from(l),
    }
    attrs.libl.cur = if r.curlib.is_empty() {
        mine.libl.cur.clone()
    } else if r.curlib == "*CRTDFT" {
        None
    } else {
        Some(r.curlib.clone())
    };
    for (kw, v) in &r.changes {
        attrs
            .change(kw, v)
            .map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPD0084")))?;
    }
    if user != me_info.user {
        // Running as another user needs *USE to its profile.
        super::spool::with_store(system, |txn, qsys| {
            let id = sec::profile::objects::find(txn, qsys, &user)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("CPF2204 user profile {user} not found"))?;
            store::obj::guard::check_id(txn, "usrprf.use", id).map_err(|e| e.to_string())
        })?;
    }
    let name = if r.job.is_empty() || r.job == "*JOBD" {
        qualified(&jobd).1
    } else {
        r.job.clone()
    };
    qsname::Name::parse(&name).map_err(|e| format!("CPD0078 JOB({name}): {e}"))?;
    let queue = if r.jobq.is_empty() || r.jobq == "*JOBD" {
        format!("{}/{}", d.jobq.0, d.jobq.1)
    } else {
        qual(&r.jobq)
    };
    let msgq = match r.msgq.as_str() {
        "" | "*USRPRF" => format!("QUSRSYS/{}", me_info.user),
        "*NONE" => String::new(),
        q => qual(q),
    };
    let number = super::jobs::next(system)?;
    let job = QJob {
        number,
        name: name.clone(),
        user: user.clone(),
        jobpty: r.jobpty.unwrap_or(d.jobpty),
        held: r.hold.unwrap_or(d.hold),
        submitted: super::msgs::now_micros(),
        scheduled: r.scheduled,
        jobd,
        msgq,
        submitter: (me_info.name.clone(), me_info.user.clone(), me),
        rtgdta: if r.rtgdta.is_empty() {
            "QCMDB".into()
        } else {
            r.rtgdta.clone()
        },
        rqsdta: r.cmd.clone(),
        attrs,
        lda: r.lda.clone(),
    };
    let real = with_jobq(system, &queue, |jq, real| {
        jq.add(job);
        Ok((real.to_string(), true))
    })?;
    system.queued.insert(number, real.clone());
    let (lib, qname) = qualified(&real);
    let data = wm::joblog::data(&[
        (&name, 10),
        (&user, 10),
        (&format!("{number:06}"), 6),
        (&qname, 10),
        (&lib, 10),
    ]);
    let who = super::msgs::sender(system, me);
    super::hist::system_message(system, "CPC1221", &data, "01", who);
    super::security::js("B", (&name, &user, number), 'B', &r.jobd);
    Ok((
        number,
        format!("Job {number:06}/{user}/{name} submitted to job queue {qname} in {lib}."),
    ))
}

/// The next job a monitor may start from a queue it holds.
pub fn take(system: &mut System, monitor: u32, q: &str, allowed: &[bool]) -> Option<QJob> {
    let q = qual(q);
    let sbs = system
        .sbs
        .iter()
        .find(|(_, a)| a.monitor == monitor)
        .map(|(n, _)| n.clone())?;
    if system.jobq_attach.get(&q) != Some(&sbs) || !super::sbs::has_room(system, &sbs) {
        return None;
    }
    let now = now_unix();
    let j = with_jobq(system, &q, |jq, _| {
        let j = jq.take(now, |p| {
            allowed.get(usize::from(p)).copied().unwrap_or(true)
        });
        let changed = j.is_some();
        Ok((j, changed))
    })
    .ok()??;
    system.queued.remove(&j.number);
    system.pending.insert(j.number, j.clone());
    system.stats.taken.insert(j.number, std::time::Instant::now());
    Some(j)
}

/// Find a queued job by `number/user/name` or number.
fn find_queued(system: &System, job: &str) -> Option<(u32, String)> {
    let number = wm::job::JobId::parse(job)
        .map(|id| id.number)
        .or_else(|| job.trim().parse().ok())?;
    system.queued.get(&number).map(|q| (number, q.clone()))
}

/// The number of an active job by `number/user/name` or number.
fn find_active(system: &System, job: &str) -> Option<u32> {
    let number = wm::job::JobId::parse(job)
        .map(|id| id.number)
        .or_else(|| job.trim().parse().ok());
    if let Some(n) = number {
        return system.jobs.in_use(n).then_some(n);
    }
    // A bare name: the newest active job of that name.
    let name = job.trim().to_ascii_uppercase();
    system.jobs.all().into_iter().filter(|j| j.name == name).map(|j| j.number).max()
}

fn signal(system: &System, number: u32, sig: rustix::process::Signal) -> Result<(), String> {
    let pid = system
        .jobs
        .get(number)
        .map(|j| j.pid)
        .ok_or("CPF1321 job not found")?;
    let p = rustix::process::Pid::from_raw(i32::try_from(pid).unwrap_or(0)).ok_or("no process")?;
    rustix::process::kill_process(p, sig).map_err(|e| e.to_string())
}

/// Jobs `HLDJOB` cannot hold: the system's own and the caller.
fn system_job(system: &System, number: u32, me: u32) -> bool {
    number == me
        || system
            .jobs
            .get(number)
            .is_some_and(|j| matches!(j.kind, 'S' | 'X' | 'M'))
}

/// Whether the actor may control active job `n`.
fn control_active(system: &System, n: u32) -> Result<(), String> {
    match system.jobs.get(n) {
        Some(j) => super::security::may_control(&j.user, &j.qualified()),
        None => Ok(()),
    }
}

/// `HLDJOB`, `RLSJOB`.
pub fn hold(system: &mut System, me: u32, job: &str, hold: bool) -> Result<String, String> {
    let verb = if hold { "held" } else { "released" };
    if let Some((n, q)) = find_queued(system, job) {
        with_jobq(system, &q, |jq, _| {
            let j = jq.job_mut(n).ok_or("CPF1321 job not found")?;
            super::security::may_control(&j.user, &j.qualified())?;
            j.held = hold;
            Ok(((), true))
        })?;
        return Ok(format!("Job {n:06} on job queue {q} {verb}.\n"));
    }
    let n = find_active(system, job).ok_or_else(|| format!("CPF1321 job {job} not found"))?;
    control_active(system, n)?;
    if system_job(system, n, me) {
        return Err(format!("CPF1342 job {n:06} cannot be held"));
    }
    if hold {
        signal(system, n, rustix::process::Signal::STOP)?;
        if let Some(j) = system.jobs.get_mut(n) {
            j.status = wm::job::Status::Hld.byte();
        }
    } else {
        signal(system, n, rustix::process::Signal::CONT)?;
        if let Some(j) = system.jobs.get_mut(n) {
            j.status = wm::job::Status::Run.byte();
        }
    }
    if let Some(j) = system.jobs.get(n) {
        let (name, user, kind, jobd) = (j.name.clone(), j.user.clone(), j.kind, j.jobd.clone());
        super::security::js(if hold { "H" } else { "R" }, (&name, &user, n), kind, &jobd);
    }
    Ok(format!("Job {n:06} {verb}.\n"))
}

/// `ENDJOB JOB() OPTION(*CNTRLD|*IMMED)`.
pub fn endjob(system: &mut System, me: u32, job: &str, immediate: bool) -> Result<String, String> {
    if let Some((n, q)) = find_queued(system, job) {
        let j = with_jobq(system, &q, |jq, _| {
            let j = jq.job_mut(n).ok_or("CPF1321 job not found")?;
            super::security::may_control(&j.user, &j.qualified())?;
            let j = jq.remove(n).ok_or("CPF1321 job not found")?;
            Ok((j, true))
        })?;
        system.queued.remove(&n);
        let data = wm::joblog::ended_data((&j.name, &j.user, n), now_unix(), 0, 50);
        super::hist::system_message(
            system,
            "CPF1164",
            &data,
            "04",
            (j.name.clone(), j.user.clone(), n),
        );
        return Ok(format!(
            "Job {} ended while on job queue {q}.\n",
            j.qualified()
        ));
    }
    let n = find_active(system, job).ok_or_else(|| format!("CPF1321 job {job} not found"))?;
    control_active(system, n)?;
    if system_job(system, n, me) {
        return Err(format!("CPF1362 job {n:06} cannot be ended this way"));
    }
    let sbs = system
        .jobs
        .get(n)
        .map(|j| j.subsystem.clone())
        .unwrap_or_default();
    if !system.sbs.contains_key(&sbs) {
        return Err(format!("CPF1362 job {n:06} has no subsystem to end it"));
    }
    if system
        .jobs
        .get(n)
        .is_some_and(|j| j.job_status() == wm::job::Status::Hld)
    {
        let _ = signal(system, n, rustix::process::Signal::CONT);
    }
    system.end_requests.insert(n, immediate);
    if let Some(j) = system.jobs.get(n) {
        let (name, user, kind, jobd) = (j.name.clone(), j.user.clone(), j.kind, j.jobd.clone());
        super::security::js("N", (&name, &user, n), kind, &jobd);
    }
    Ok(format!("Job {n:06} is ending.\n"))
}

/// The end requests for monitor `monitor`'s jobs, taken.
pub fn take_end_requests(system: &mut System, monitor: u32) -> Vec<(u32, bool)> {
    let Some(sbs) = system
        .sbs
        .iter()
        .find(|(_, a)| a.monitor == monitor)
        .map(|(n, _)| n.clone())
    else {
        return Vec::new();
    };
    let mine: Vec<(u32, bool)> = system
        .end_requests
        .iter()
        .filter(|(n, _)| system.jobs.get(**n).is_some_and(|j| j.subsystem == sbs))
        .map(|(n, i)| (*n, *i))
        .collect();
    for (n, _) in &mine {
        system.end_requests.remove(n);
    }
    mine
}

/// `CHGJOB` of a queued job: `JOBPTY` and `JOBQ` move it; the other
/// keywords change the attributes it will start with. `None` when the
/// job is not queued.
pub fn chgjob(
    system: &mut System,
    job: &str,
    changes: &[(String, String)],
) -> Option<Result<String, String>> {
    let (n, q) = find_queued(system, job)?;
    Some((|| {
        let mut target: Option<String> = None;
        let mut pty: Option<u8> = None;
        let mut rest = Vec::new();
        for (k, v) in changes {
            match k.as_str() {
                "JOBQ" => target = Some(qual(v)),
                "JOBPTY" => {
                    pty = Some(
                        v.trim()
                            .parse()
                            .ok()
                            .filter(|p| *p <= 9)
                            .ok_or_else(|| format!("CPD0084 JOBPTY({v})"))?,
                    );
                }
                _ => rest.push((k.clone(), v.clone())),
            }
        }
        let mut moved = with_jobq(system, &q, |jq, _| {
            let j = jq.job_mut(n).ok_or("CPF1321 job not found")?;
            super::security::may_control(&j.user, &j.qualified())?;
            for (k, v) in &rest {
                j.attrs
                    .change(k, v)
                    .map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPD0084")))?;
            }
            if let Some(p) = pty {
                jq.set_priority(n, p);
            }
            let out = if target.is_some() { jq.remove(n) } else { None };
            Ok((out, true))
        })?;
        if let (Some(t), Some(j)) = (target, moved.take()) {
            let real = with_jobq(system, &t, |jq, real| {
                jq.add(j);
                Ok((real.to_string(), true))
            })?;
            system.queued.insert(n, real);
        }
        Ok(format!("Job {n:06} changed on its job queue.\n"))
    })())
}

/// `WRKJOBQ [LIB/NAME]`: every queue with its jobs and status, or one.
pub fn wrkjobq(system: &mut System, which: Option<&str>) -> Result<String, String> {
    let queues: Vec<String> = match which {
        Some(q) => vec![qual(q)],
        None => all_queues(system)?.into_iter().map(|(q, _)| q).collect(),
    };
    let now = now_unix();
    let mut out = String::from("Queue       Library     Jobs  Subsystem   Status\n");
    let mut detail = String::new();
    for q in queues {
        let (jobs, held, real) = with_jobq(system, &q, |jq, real| {
            Ok(((jq.jobs.clone(), jq.held, real.to_string()), false))
        })?;
        let (lib, name) = qualified(&real);
        let sbs = system.jobq_attach.get(&real).cloned().unwrap_or_default();
        let _ = writeln!(
            out,
            "{name:<10}  {lib:<10}  {:>4}  {sbs:<10}  {}",
            jobs.len(),
            if held { "HLD" } else { "RLS" }
        );
        if which.is_some() {
            let _ = writeln!(detail, "\nJob         User        Number  Pty  Status");
            for j in &jobs {
                let _ = writeln!(
                    detail,
                    "{:<10}  {:<10}  {:06}  {:>3}  {}",
                    j.name,
                    j.user,
                    j.number,
                    j.jobpty,
                    j.status(now)
                );
            }
        }
    }
    out.push_str(&detail);
    Ok(out)
}

/// `CHGJOBQ`: `TEXT`, `OPRCTL`, `AUTCHK`.
pub fn chgjobq(system: &mut System, q: &str, p: &wm::cmd::Params) -> Result<String, String> {
    let text = p.kw("TEXT").map(|_| p.text("TEXT"));
    let oprctl = p.upper("OPRCTL").map(|v| v == "*YES");
    let autchk = match p.upper("AUTCHK") {
        None => None,
        Some(v) if v == "*OWNER" || v == "*DTAAUT" => Some(v),
        Some(v) => return Err(format!("CPD0084 AUTCHK({v}) is not *OWNER or *DTAAUT")),
    };
    with_jobq(system, q, |jq, _| {
        if let Some(t) = text {
            jq.text = if t == "*BLANK" { String::new() } else { t };
        }
        if let Some(o) = oprctl {
            jq.oprctl = o;
        }
        if let Some(a) = autchk {
            jq.autchk = a;
        }
        Ok(((), true))
    })?;
    Ok(format!("Job queue {} changed.\n", qual(q)))
}

/// `HLDJOBQ`, `RLSJOBQ`, `CLRJOBQ`.
pub fn queue_op(system: &mut System, q: &str, op: &str) -> Result<String, String> {
    let gone = with_jobq(system, q, |jq, _| {
        let gone: Vec<QJob> = match op {
            "hold" => {
                jq.held = true;
                Vec::new()
            }
            "release" => {
                jq.held = false;
                Vec::new()
            }
            _ => std::mem::take(&mut jq.jobs),
        };
        Ok((gone, true))
    })?;
    for j in &gone {
        system.queued.remove(&j.number);
    }
    Ok(match op {
        "hold" => format!("Job queue {} held.\n", qual(q)),
        "release" => format!("Job queue {} released.\n", qual(q)),
        _ => format!(
            "{} job(s) removed from job queue {}.\n",
            gone.len(),
            qual(q)
        ),
    })
}

/// `CRTJOBQ`, `DLTJOBQ`.
pub fn create_delete(
    system: &mut System,
    q: &str,
    text: &str,
    create: bool,
    me: u32,
) -> Result<String, String> {
    let (lib, name) = qualified(q);
    let lib = if lib == "*LIBL" {
        "QGPL".to_string()
    } else {
        lib
    };
    if !create {
        let jobs = with_jobq(system, q, |jq, _| Ok((jq.jobs.len(), false)))?;
        if jobs > 0 || system.jobq_attach.contains_key(&format!("{lib}/{name}")) {
            return Err(format!(
                "CPF2177 job queue {name} has jobs or is held by a subsystem"
            ));
        }
    }
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    crate::svc::db::with_txn(&mut s.store, |txn| {
        let l = wm::obj::library(txn, qsys, &lib).map_err(|e| e.to_string())?;
        let n = qsname::Name::parse(&name).map_err(|e| format!("CPD0078 {name}: {e}"))?;
        if create {
            let stamp = wm::obj::Stamp {
                owner: "QSECOFR".into(),
                system: "QS400".into(),
                now: now_unix(),
            };
            wm::obj::create(
                txn,
                l,
                n,
                types::JOBQ,
                text,
                &stamp,
                &JobQ::new(text).encode(),
            )
            .map_err(|e| e.to_string())?;
        } else {
            let names = crate::svc::ile::name_context(txn, qsys);
            let id = wm::obj::find(txn, qsys, &names, &lib, &name, types::JOBQ)
                .map_err(|e| e.to_string())?;
            ops::delete(txn, JobHandle(u64::from(me)), id).map_err(|e| e.to_string())?;
        }
        Ok(())
    })?;
    Ok(format!(
        "Job queue {name} {} library {lib}.\n",
        if create { "created in" } else { "deleted from" }
    ))
}
