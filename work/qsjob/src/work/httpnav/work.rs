// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `navigator::env::work::Work` as the user, over the requests the
//! `WRK*` panels use: `actjob_data` (`WRKACTJOB`), `job_detail`
//! (`DSPJOB`), `panel2_data JOBLCK` (`DSPJOB OPTION(*JOBLCK)`),
//! `list_objects` and `wm_object` (`WRKJOBQ`, `WRKSBS`/`DSPSBSD`,
//! `WRKOUTQ`, `WRKJOBSCDE`), `jobq_attach`, `sbs_list`, `pools`,
//! `splf_list` and `splf_fetch` (`WRKSPLF`, `DSPSPLF`), `wtr_list` and
//! `cfg_status` (`WRKWTR`, `WRKCFGSTS`); changes are commands (`run`).
//! Another job's log and call stack are not readable (the 5250
//! displays refuse them too): those tabs are empty. A job a user ends
//! here is reported to that user once it has left the active job table
//! (`CPF1321`).

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

use navigator::env::work::{
    ActiveJobs, Autostart, Choices, Job, JobPerf, JobProps, JobQueue, LibEntry, Lock, LogEntry, OutQueue, Prestart,
    Routing, SbsPool, ScheduleEntry, Splf, Subsystem, SubsystemDesc, Thread, Work, Workstation, Writer,
};
use qsweb::msg::{Kind, Message};
use stg::{Client, JobInfo};
use wm::obj::jobq::JobQ;
use wm::obj::jobscd::{DateKind, Frequency, JobScd, SCHEDULE};
use wm::obj::outq::{OutQ, Seq};
use wm::obj::sbsd::{NOMAX16, PoolKind, Sbsd};
use wm::obj::splf::SplfAttrs;

use super::Link;
use crate::work::web::bridge::Main;

/// Jobs each user ended here, by user, as `number/user/name`.
type Ended = Arc<Mutex<HashMap<String, Vec<String>>>>;

/// Work management.
pub struct LiveWork {
    link: Link,
    ended: Ended,
}

impl LiveWork {
    /// Over the job's thread; `_me` is this job's qualified name.
    pub fn new(link: Link, _me: String) -> Self {
        Self {
            link,
            ended: Arc::default(),
        }
    }
}

/// The job `ENDJOB JOB(...)` names, when `command` is one.
fn ended_job(command: &str) -> Option<String> {
    let c = command.trim().to_ascii_uppercase();
    let rest = c.strip_prefix("ENDJOB ")?;
    let at = rest.find("JOB(")? + 4;
    let end = rest[at..].find(')')? + at;
    Some(rest[at..end].trim().to_string())
}

/// `CPF1321` for each job `user` ended that is gone from `jobs`; those
/// are forgotten.
fn notes(m: &Main, ended: &Ended, user: &str, jobs: &[Job]) -> Vec<Message> {
    let gone: Vec<String> = {
        let Ok(mut all) = ended.lock() else {
            return Vec::new();
        };
        let Some(mine) = all.get_mut(user) else {
            return Vec::new();
        };
        let number = |q: &str| q.split('/').next().and_then(|n| n.trim().parse::<u32>().ok());
        let (gone, left): (Vec<String>, Vec<String>) = mine
            .drain(..)
            .partition(|q| !jobs.iter().any(|j| j.number.parse::<u32>().ok() == number(q)));
        *mine = left;
        gone
    };
    if gone.is_empty() {
        return Vec::new();
    }
    let now = m.link.borrow_mut().with(Client::now);
    let (date, time) = crate::httpenv::stamp::stamp(now.unix, now.offset_min * 60, "*MDY", "/", ":");
    gone.iter()
        .map(|q| {
            let vars = [q.as_str(), date.as_str(), time.as_str()];
            let text = super::db::text_of(m, "CPF1321", &vars);
            Message::new(Kind::Info, "CPF1321", &text).with_data(&vars)
        })
        .collect()
}

fn job_of(j: &JobInfo, cpu: u32) -> Job {
    Job {
        number: format!("{:06}", j.number),
        user: j.user.clone(),
        name: j.name.clone(),
        kind: j
            .job_type()
            .map_or("", |t| wm::job::wrkactjob_type(t, j.sub_type()))
            .trim()
            .to_string(),
        subsystem: j.subsystem.clone(),
        status: j.job_status().code().to_string(),
        cpu,
        function: j.function.clone(),
        current_user: j.user.clone(),
    }
}

/// Every object of a type, as `(lib, name)`.
fn all_of(m: &Main, ty: &str) -> Vec<(String, String)> {
    let t = ty.to_string();
    m.link
        .borrow_mut()
        .with(|l| l.list_objects("*ALL".into(), "*ALL".into(), t))
        .into_iter()
        .map(|o| (o.lib, o.name))
        .collect()
}

fn payload(m: &Main, lib: &str, name: &str, ty: &str) -> Option<Vec<u8>> {
    let (q, t) = (format!("{lib}/{name}"), ty.to_string());
    m.link.borrow_mut().with(|l| l.wm_object(q, t)).ok()
}

fn nomax16(v: u16) -> Option<u32> {
    (v != NOMAX16 && v != u16::MAX).then_some(u32::from(v))
}

fn nomax32(v: u32) -> Option<u32> {
    (v != 0 && v != u32::MAX).then_some(v)
}

fn qual((name, lib): &(String, String)) -> String {
    if lib.is_empty() { name.clone() } else { format!("{lib}/{name}") }
}

fn splf(f: SplfAttrs) -> Splf {
    Splf {
        user: f.job.1.clone(),
        user_data: f.usrdta,
        status: f.status.code().to_string(),
        pages: f.pages,
        copies: u32::from(f.copies_left.max(1)),
        form_type: f.formtype,
        created: f.created / 1_000_000,
        job: format!("{:06}/{}/{}", f.job.2, f.job.1, f.job.0),
        number: f.number,
        outq: f.outq,
        file: f.name,
    }
}

/// The subsystem's list entry from its description and, while active,
/// the service's figures.
fn subsystem_of(m: &Main, lib: &str, name: &str, active: &[wm::sbs::SbsInfo]) -> Option<(Subsystem, Sbsd)> {
    let d = Sbsd::decode(&payload(m, lib, name, "*SBSD")?).ok()?;
    let a = active.iter().find(|s| s.name == name);
    let s = Subsystem {
        name: name.to_string(),
        lib: lib.to_string(),
        active: a.is_some(),
        jobs: a.map_or(0, |a| a.jobs),
        pools: d
            .pools
            .iter()
            .map(|p| match p.kind {
                PoolKind::Private => format!("{} K", p.size_kb),
                k => k.name(),
            })
            .collect(),
        max_jobs: nomax32(d.maxjobs),
        text: d.text.trim().to_string(),
    };
    Some((s, d))
}

fn desc(m: &Main, lib: &str, name: &str) -> Option<SubsystemDesc> {
    let active = m.link.borrow_mut().with(Client::sbs_list);
    let (sbs, d) = subsystem_of(m, lib, name, &active)?;
    let a = active.iter().find(|s| s.name == name).cloned();
    let pools = m.link.borrow_mut().with(Client::pools);
    let jobs = m.link.borrow_mut().with(Client::active_jobs);
    let monitor = a
        .as_ref()
        .and_then(|a| jobs.iter().find(|j| j.number == a.monitor))
        .map(JobInfo::qualified);
    let since = a.as_ref().and_then(|a| jobs.iter().find(|j| j.number == a.monitor)).map(|j| j.started);
    let sys_pool = |id: u8| {
        let pname = a.as_ref().and_then(|a| a.pools.iter().find(|(n, _)| *n == id)).map(|(_, p)| p.clone());
        pname.and_then(|p| pools.iter().find(|x| x.name == p).cloned())
    };
    Some(SubsystemDesc {
        since,
        monitor,
        signon: qual(&d.sgndspf),
        job_queues: d.jobqes.iter().map(|q| qual(&q.jobq)).collect(),
        pools: d
            .pools
            .iter()
            .map(|p| {
                let sp = sys_pool(p.id);
                SbsPool {
                    id: u32::from(p.id),
                    storage: match p.kind {
                        PoolKind::Private => format!("{} K", p.size_kb),
                        k => k.name(),
                    },
                    activity: (p.kind == PoolKind::Private).then_some(p.actlvl),
                    system_pool: sp.as_ref().map_or(0, |x| u32::from(x.number)),
                    size_mb: sp.as_ref().map_or(p.size_kb / 1024, |x| x.size_kb / 1024),
                    reserved_mb: sp.as_ref().map_or(0, |x| x.reserved_kb / 1024),
                    faults: 0,
                    pages: 0,
                }
            })
            .collect(),
        routing: d
            .rtges
            .iter()
            .map(|r| Routing {
                seq: u32::from(r.seq),
                compare: r.cmpval.clone(),
                start: u32::from(r.start),
                program: r.pgm.0.clone(),
                lib: r.pgm.1.clone(),
                class: qual(&r.cls),
                max_active: nomax16(r.maxact),
                pool: u32::from(r.pool),
            })
            .collect(),
        autostart: d
            .ajes
            .iter()
            .map(|e| Autostart {
                job: e.job.clone(),
                jobd: e.jobd.0.clone(),
                lib: e.jobd.1.clone(),
            })
            .collect(),
        workstations: d
            .wsns
            .iter()
            .map(|w| (w, false))
            .chain(d.wsts.iter().map(|w| (w, true)))
            .map(|(w, by_type)| Workstation {
                name: w.name.clone(),
                by_type,
                jobd: qual(&w.jobd),
                max_active: nomax16(w.maxact),
                at: if w.at_signon { "*SIGNON" } else { "*ENTER" }.into(),
            })
            .collect(),
        prestart: d
            .pjes
            .iter()
            .map(|p| Prestart {
                program: p.pgm.0.clone(),
                lib: p.pgm.1.clone(),
                user: p.user.clone(),
                initial: u32::from(p.inljobs),
                threshold: u32::from(p.threshold),
                max_jobs: nomax16(p.maxjobs),
            })
            .collect(),
        sbs,
    })
}

fn detail(m: &Main, job: &str) -> Option<stg::proto::JobDetail> {
    let j = job.to_string();
    m.link.borrow_mut().with(|l| l.job_detail(j)).ok()
}

fn props(m: &Main, job: &str) -> Option<JobProps> {
    let d = detail(m, job)?;
    let j = &d.job;
    let a = &d.attrs;
    let now = m.link.borrow_mut().with(Client::now).unix;
    let pool = m
        .link
        .borrow_mut()
        .with(Client::pools)
        .into_iter()
        .find(|p| p.name == j.pool);
    Some(JobProps {
        job: job_of(j, 0),
        entered: j.started,
        started: j.started,
        submitted_by: String::new(),
        submitted_how: String::new(),
        jobd: a.jobd.clone(),
        jobq: a.jobq.clone(),
        outq: a.outq.clone(),
        run_priority: u32::from(a.runpty),
        time_slice: a.timeslice,
        logging: format!("{} {:02} {}", a.log_level, a.log_sev, a.log_text),
        log_cl: a.logclpgm == "*YES",
        inquiry: a.inqmsgrpy.clone(),
        ccsid: a.ccsid.to_string(),
        language: a.langid.clone(),
        country: a.cntryid.clone(),
        perf: JobPerf {
            cpu_ms: d.cpu_ms,
            elapsed: now.saturating_sub(j.started),
            io_sync: d.aux_io,
            threads: d.threads,
            temp_mb: d.tmp_mb,
            temp_peak_mb: d.tmp_mb,
            pool: pool.as_ref().map_or(0, |p| u32::from(p.number)),
            pool_name: j.pool.clone(),
            activity: pool.as_ref().map_or(0, |p| p.actlvl),
            pool_mb: pool.as_ref().map_or(0, |p| p.size_kb / 1024),
            ..JobPerf::default()
        },
    })
}

fn writers(m: &Main) -> Vec<Writer> {
    let files = m.link.borrow_mut().with(|l| l.splf_list(String::new(), String::new()));
    m.link
        .borrow_mut()
        .with(Client::wtr_list)
        .into_iter()
        .map(|(name, dev, status, form, outq, _)| {
            let printing = files.iter().find(|f| f.writer == name);
            let varied_on = if dev.is_empty() {
                None
            } else {
                let d = dev.clone();
                m.link
                    .borrow_mut()
                    .with(|l| l.cfg_status(d, "*DEV".into()))
                    .map(|(code, _)| code >= 30)
            };
            Writer {
                device: if dev.is_empty() { name.clone() } else { dev },
                started: status != "END",
                held: status == "HLD",
                varied_on,
                outq,
                file: printing.map(|f| f.name.clone()).unwrap_or_default(),
                user: printing.map(|f| f.job.1.clone()).unwrap_or_default(),
                pages_left: printing.map(|f| f.pages.saturating_sub(f.last_page)),
                form_type: form,
                model: String::new(),
            }
        })
        .collect()
}

/// Days since 1970 at the start of `YYYYMMDD`.
fn date_secs(ymd: u32) -> Option<u64> {
    let (y, mo, d) = (i64::from(ymd / 10_000), i64::from(ymd / 100 % 100), i64::from(ymd % 100));
    if ymd == 0 || mo == 0 || d == 0 {
        return None;
    }
    let y = if mo <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((mo + 9) % 12) + 2) / 5 + d - 1;
    let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
    u64::try_from(days * 86_400).ok()
}

const DAYS: [&str; 7] = ["*MON", "*TUE", "*WED", "*THU", "*FRI", "*SAT", "*SUN"];

fn schedule(m: &Main) -> Vec<ScheduleEntry> {
    let (lib, name) = SCHEDULE;
    let scd = payload(m, lib, name, "*JOBSCD")
        .and_then(|b| JobScd::decode(&b).ok())
        .unwrap_or_default();
    scd.entries
        .into_iter()
        .map(|e| ScheduleEntry {
            entry: e.number,
            command: e.cmd,
            frequency: match e.frequency {
                Frequency::Once => "*ONCE",
                Frequency::Weekly => "*WEEKLY",
                Frequency::Monthly => "*MONTHLY",
            }
            .into(),
            date: if e.date_kind == DateKind::Date { date_secs(e.date) } else { None },
            days: if e.days == 0x7F {
                vec!["*ALL".into()]
            } else {
                DAYS.iter()
                    .enumerate()
                    .filter(|(i, _)| e.days & (1 << i) != 0)
                    .map(|(_, d)| (*d).to_string())
                    .collect()
            },
            time: format!("{:02}:{:02}", e.time / 3600, e.time / 60 % 60),
            next: (e.next != 0).then_some(e.next),
            jobq: e.jobq,
            user: e.user,
            held: e.held,
            text: e.text,
            name: e.job,
        })
        .collect()
}

impl Work for LiveWork {
    fn active_jobs(&self, user: &str, reset: bool) -> ActiveJobs {
        let (ended, u) = (Arc::clone(&self.ended), user.to_string());
        self.link.read(user, move |m| {
            let d = m.link.borrow_mut().with(|l| l.actjob_data(reset));
            let jobs: Vec<Job> = d.jobs.iter().map(|a| job_of(&a.job, a.cpu)).collect();
            ActiveJobs {
                cpu: d.cpu,
                elapsed: u32::try_from(d.elapsed).unwrap_or(u32::MAX),
                notes: notes(m, &ended, &u, &jobs),
                jobs,
            }
        })
    }

    fn choices(&self, user: &str, what: Choices) -> Vec<String> {
        self.link.read(user, move |m| {
            let (ty, libs) = match what {
                Choices::Subsystems => ("*SBSD", false),
                Choices::JobqLibraries => ("*JOBQ", true),
                Choices::OutqLibraries => ("*OUTQ", true),
            };
            let set: BTreeSet<String> = all_of(m, ty)
                .into_iter()
                .map(|(l, n)| if libs { l } else { n })
                .collect();
            set.into_iter().collect()
        })
    }

    fn job(&self, user: &str, job: &str) -> Option<JobProps> {
        let j = job.to_string();
        self.link.read(user, move |m| props(m, &j))
    }

    fn job_log(&self, _user: &str, _job: &str) -> Vec<LogEntry> {
        Vec::new()
    }

    fn library_list(&self, user: &str, job: &str) -> Vec<LibEntry> {
        let j = job.to_string();
        self.link.read(user, move |m| {
            let Some(d) = detail(m, &j) else { return Vec::new() };
            let l = &d.attrs.libl;
            let mut out = Vec::new();
            let mut seq = 0;
            let mut add = |lib: &str, kind: &str, current: bool| {
                let name = lib.to_string();
                let text = m
                    .link
                    .borrow_mut()
                    .with(|c| c.object("QSYS".into(), name, "*LIB".into()))
                    .map(|o| o.text)
                    .unwrap_or_default();
                let s = if current {
                    None
                } else {
                    seq += 10;
                    Some(seq)
                };
                out.push(LibEntry {
                    seq: s,
                    lib: lib.to_string(),
                    kind: kind.into(),
                    text,
                });
            };
            l.sys.iter().for_each(|x| add(x, "SYS", false));
            l.prd.iter().filter(|x| !x.is_empty()).for_each(|x| add(x, "PRD", false));
            l.cur.iter().for_each(|x| add(x, "CUR", true));
            l.usr.iter().for_each(|x| add(x, "USR", false));
            out
        })
    }

    fn locks(&self, user: &str, job: &str) -> Vec<Lock> {
        let j = job.to_string();
        self.link.read(user, move |m| {
            let Ok((_, rows)) = m.link.borrow_mut().with(|l| l.panel2_data("JOBLCK".into(), vec![j])) else {
                return Vec::new();
            };
            rows.into_iter()
                .map(|r| {
                    let v = |k: &str| r.iter().find(|(n, _)| n == k).map(|(_, x)| x.clone()).unwrap_or_default();
                    Lock {
                        object: v("OBJ"),
                        lib: v("OBJLIB"),
                        typ: v("OBJTYPE"),
                        lock: v("LOCK"),
                        status: v("STATUS"),
                        scope: v("SCOPE"),
                        thread: String::new(),
                    }
                })
                .collect()
        })
    }

    fn call_stack(&self, _user: &str, _job: &str) -> Vec<Thread> {
        Vec::new()
    }

    fn job_queues(&self, user: &str) -> Vec<JobQueue> {
        self.link.read(user, |m| {
            let attach = m.link.borrow_mut().with(Client::jobq_attach);
            let now = m.link.borrow_mut().with(Client::now).unix;
            all_of(m, "*JOBQ")
                .into_iter()
                .filter_map(|(lib, name)| {
                    let q = JobQ::decode(&payload(m, &lib, &name, "*JOBQ")?).ok()?;
                    let qual = format!("{lib}/{name}");
                    let count = |f: &dyn Fn(&wm::obj::jobq::QJob) -> bool| {
                        u32::try_from(q.jobs.iter().filter(|j| f(j)).count()).unwrap_or(0)
                    };
                    Some(JobQueue {
                        held: q.held,
                        subsystem: attach.iter().find(|(k, _)| *k == qual).map(|(_, s)| s.clone()).unwrap_or_default(),
                        waiting: count(&|_| true),
                        held_jobs: count(&|j| j.held),
                        scheduled: count(&|j| j.scheduled > now),
                        max_active: None,
                        text: q.text.trim().to_string(),
                        name,
                        lib,
                    })
                })
                .collect()
        })
    }

    fn subsystems(&self, user: &str) -> Vec<Subsystem> {
        self.link.read(user, |m| {
            let active = m.link.borrow_mut().with(Client::sbs_list);
            all_of(m, "*SBSD")
                .into_iter()
                .filter_map(|(lib, name)| subsystem_of(m, &lib, &name, &active).map(|(s, _)| s))
                .collect()
        })
    }

    fn subsystem(&self, user: &str, name: &str) -> Option<SubsystemDesc> {
        let n = name.trim().to_ascii_uppercase();
        self.link.read(user, move |m| {
            let (lib, name) = all_of(m, "*SBSD").into_iter().find(|(_, x)| *x == n)?;
            desc(m, &lib, &name)
        })
    }

    fn output_queues(&self, user: &str) -> Vec<OutQueue> {
        self.link.read(user, |m| {
            all_of(m, "*OUTQ")
                .into_iter()
                .filter_map(|(lib, name)| {
                    let q = OutQ::decode(&payload(m, &lib, &name, "*OUTQ")?).ok()?;
                    Some(OutQueue {
                        files: u32::try_from(q.files.len()).unwrap_or(0),
                        writer: q.writer.clone(),
                        held: q.held,
                        order: match q.seq {
                            Seq::Fifo => "*FIFO",
                            Seq::JobNbr => "*JOBNBR",
                        }
                        .into(),
                        operator_control: q.oprctl,
                        text: q.text.trim().to_string(),
                        name,
                        lib,
                    })
                })
                .collect()
        })
    }

    fn spooled_files(&self, user: &str, outq: &str, job: &str) -> Vec<Splf> {
        let (q, j) = (outq.to_string(), job.to_string());
        self.link.read(user, move |m| {
            let all = j.is_empty();
            let jb = j.clone();
            m.link
                .borrow_mut()
                .with(|l| l.splf_list(String::new(), jb))
                .into_iter()
                .filter(|f| !all || f.outq == q)
                .map(splf)
                .collect()
        })
    }

    fn spooled_text(&self, user: &str, job: &str, file: &str, number: u32) -> Option<(Splf, Vec<String>)> {
        let (j, f) = (job.to_string(), file.to_string());
        self.link.read(user, move |m| {
            let (a, data) = m.link.borrow_mut().with(|l| l.splf_fetch(f, j, number.to_string())).ok()?;
            let (pages, _) = spool::scs::decode(&data);
            let text = pages.iter().map(|p| spool::view::text(std::slice::from_ref(p))).collect();
            Some((splf(a), text))
        })
    }

    fn spooled_pdf(&self, user: &str, job: &str, file: &str, number: u32) -> Option<Vec<u8>> {
        let (j, f) = (job.to_string(), file.to_string());
        self.link.read(user, move |m| {
            let (_, data) = m.link.borrow_mut().with(|l| l.splf_fetch(f, j, number.to_string())).ok()?;
            let (pages, fmt) = spool::scs::decode(&data);
            Some(spool::pdf::render(&pages, fmt))
        })
    }

    fn writers(&self, user: &str) -> Vec<Writer> {
        self.link.read(user, |m| writers(m))
    }

    fn schedule(&self, user: &str) -> Vec<ScheduleEntry> {
        self.link.read(user, |m| schedule(m))
    }

    fn run(&self, user: &str, command: &str) -> Result<Message, Message> {
        let r = self.link.run(user, command);
        if r.is_ok()
            && let Some(job) = ended_job(command)
            && let Ok(mut all) = self.ended.lock()
        {
            all.entry(user.to_string()).or_default().push(job);
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_limits() {
        assert_eq!(date_secs(20_261_101), Some(1_793_491_200));
        assert_eq!(date_secs(0), None);
        assert_eq!(nomax16(0), None);
        assert_eq!(nomax16(5), Some(5));
        assert_eq!(nomax32(u32::MAX), None);
        assert_eq!(ended_job("ENDJOB JOB(123456/QPGMR/X) OPTION(*IMMED)").as_deref(), Some("123456/QPGMR/X"));
        assert_eq!(ended_job("HLDJOB JOB(1/A/B)"), None);
    }
}
