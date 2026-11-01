// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sample work management: the prototype's jobs, queues, subsystems,
//! spooled files, writers and schedule, and the commands the pages run
//! acting on them as the system would.

mod data;
mod job;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::Mutex;

use qsweb::msg::{Kind, Message};

use super::text;
use crate::env::work::{
    ActiveJobs, Choices, Job, JobProps, JobQueue, LibEntry, Lock, LogEntry, OutQueue, Routing,
    ScheduleEntry, Splf, Subsystem, SubsystemDesc, Thread, Work, Writer,
};
use crate::fixture::PROFILES;
use crate::util::secs;

/// The sample system's work management.
pub struct FixtureWork {
    /// Everything the pages show and the commands change.
    pub state: Mutex<State>,
    /// Commands run, as typed.
    pub ran: Mutex<Vec<String>>,
}

/// The sample system's work, changeable by tests.
pub struct State {
    /// The active jobs.
    pub jobs: Vec<Job>,
    /// Jobs whose attributes `CHGJOB` changed.
    pub changed: BTreeMap<String, JobProps>,
    /// Notes waiting for a user's next look at the active jobs.
    pub notes: BTreeMap<String, Vec<Message>>,
    /// Job queues.
    pub jobqs: Vec<JobQueue>,
    /// Subsystem descriptions.
    pub sbs: Vec<SubsystemDesc>,
    /// Output queues.
    pub outqs: Vec<OutQueue>,
    /// Spooled files.
    pub splfs: Vec<Splf>,
    /// Writers.
    pub writers: Vec<Writer>,
    /// Job schedule entries.
    pub schedule: Vec<ScheduleEntry>,
}

impl Default for FixtureWork {
    fn default() -> Self {
        let jobs = data::jobs();
        let queues = data::job_queues();
        let sbs = data::subsystems()
            .iter()
            .map(|s| {
                let monitor = jobs
                    .iter()
                    .find(|j| j.kind == "SBS" && j.name == s.name && s.active)
                    .map(Job::qualified);
                data::description(s, monitor, &queues)
            })
            .collect();
        let note = msg(
            Kind::Info,
            "CPF1321",
            &[
                "123499/QPGMR/RGZPFM00",
                "10/29/26",
                "14:01:55",
                "1 spooled file",
            ],
        );
        Self {
            state: Mutex::new(State {
                jobs,
                changed: BTreeMap::new(),
                notes: BTreeMap::from([("QSECOFR".to_string(), vec![note])]),
                jobqs: queues,
                sbs,
                outqs: data::output_queues(),
                splfs: data::spooled_files(),
                writers: data::writers(),
                schedule: data::schedule(),
            }),
            ran: Mutex::new(Vec::new()),
        }
    }
}

/// Message `id` with `data` from the fixture's message texts.
fn msg(kind: Kind, id: &str, data: &[&str]) -> Message {
    let t = text::second_level(id, data)
        .map(|s| s.text)
        .unwrap_or_default();
    Message::new(kind, id, &t)
}

#[allow(clippy::unnecessary_wraps)]
fn done(id: &str, data: &[&str]) -> Result<Message, Message> {
    Ok(msg(Kind::Done, id, data))
}

fn fail(id: &str, data: &[&str]) -> Result<Message, Message> {
    Err(msg(Kind::Error, id, data))
}

/// A CL command's name and its `KEYWORD(value)` parameters, values with
/// their parentheses' contents as typed.
pub fn parse(cmd: &str) -> (String, BTreeMap<String, String>) {
    let cmd = cmd.trim();
    let (name, rest) = cmd.split_once(' ').unwrap_or((cmd, ""));
    let mut parms = BTreeMap::new();
    let b: Vec<char> = rest.chars().collect();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && b[i] == ' ' {
            i += 1;
        }
        let start = i;
        while i < b.len() && b[i] != '(' && b[i] != ' ' {
            i += 1;
        }
        let key: String = b[start..i].iter().collect();
        if i < b.len() && b[i] == '(' {
            let mut depth = 0;
            let mut quoted = false;
            let vs = i + 1;
            while i < b.len() {
                match b[i] {
                    '\'' => quoted = !quoted,
                    '(' if !quoted => depth += 1,
                    ')' if !quoted => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            let v: String = b[vs..i.min(b.len())].iter().collect();
            parms.insert(key.to_ascii_uppercase(), v);
            i += 1;
        } else if !key.is_empty() {
            parms.insert(key.to_ascii_uppercase(), String::new());
        }
    }
    (name.to_ascii_uppercase(), parms)
}

/// A quoted CL string's text.
fn unquote(s: &str) -> String {
    let s = s.trim();
    match s.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')) {
        Some(t) => t.replace("''", "'"),
        None => s.to_string(),
    }
}

/// `lib/name` split; a bare name gets `lib`.
fn split<'a>(q: &'a str, lib: &'a str) -> (&'a str, &'a str) {
    q.split_once('/').unwrap_or((lib, q))
}

fn special(user: &str, what: &str) -> bool {
    PROFILES
        .iter()
        .find(|p| p.name == user)
        .is_some_and(|p| p.special.iter().any(|s| *s == what || *s == "*ALLOBJ"))
}

/// Page `n` (from 1) of a spooled file's text.
fn page_text(f: &Splf, n: u32) -> String {
    if f.file == "INVRPT" && f.number == 402 && n == 1 {
        return data::INVRPT_PAGE1.join("\n") + "\n";
    }
    let (q, _) = f
        .outq
        .split_once('/')
        .map_or(("QPRINT", ""), |(_, q)| (q, ""));
    let mut s = format!(
        "{:<10}{:<12}{:<55}QSHQ400   10/29/26  06:14:02   PAGE {n:>4}\n\n",
        q,
        f.file,
        format!("{} OUTPUT", f.file)
    );
    for l in 1..=20 {
        let _ = writeln!(
            s,
            "  {:<8}  LINE {l:>3} OF PAGE {n:>3}  JOB {}",
            f.file, f.job
        );
    }
    s
}

impl State {
    fn job_mut(&mut self, q: &str) -> Option<&mut Job> {
        self.jobs.iter_mut().find(|j| j.qualified() == q)
    }

    fn props(&self, q: &str) -> Option<JobProps> {
        if let Some(p) = self.changed.get(q) {
            let mut p = p.clone();
            if let Some(j) = self.jobs.iter().find(|j| j.qualified() == q) {
                p.job = j.clone();
            }
            return Some(p);
        }
        self.jobs
            .iter()
            .find(|j| j.qualified() == q)
            .map(job::props)
    }

    fn splf_mut(&mut self, p: &BTreeMap<String, String>) -> Option<&mut Splf> {
        let file = p.get("FILE").cloned().unwrap_or_default();
        let job = p.get("JOB").cloned().unwrap_or_default();
        let nbr: u32 = p.get("SPLNBR").and_then(|n| n.parse().ok()).unwrap_or(0);
        self.splfs
            .iter_mut()
            .find(|s| s.file == file && s.job == job && s.number == nbr)
    }

    fn sbs_mut(&mut self, q: &str) -> Option<&mut SubsystemDesc> {
        let (_, name) = split(q, "*LIBL");
        self.sbs.iter_mut().find(|s| s.sbs.name == name)
    }

    fn entry_mut(&mut self, p: &BTreeMap<String, String>) -> Option<&mut ScheduleEntry> {
        let name = p.get("JOB").cloned().unwrap_or_default();
        let n: u32 = p.get("ENTRYNBR").and_then(|n| n.parse().ok()).unwrap_or(0);
        self.schedule
            .iter_mut()
            .find(|e| e.name == name && (n == 0 || e.entry == n))
    }
}

#[allow(clippy::too_many_lines, clippy::many_single_char_names)]
fn run_in(
    st: &mut State,
    user: &str,
    name: &str,
    p: &BTreeMap<String, String>,
) -> Result<Message, Message> {
    let get = |k: &str| p.get(k).map(String::as_str).unwrap_or_default();
    let jobctl = special(user, "*JOBCTL");
    match name {
        "HLDJOB" | "RLSJOB" | "ENDJOB" | "CHGJOB" => {
            let q = get("JOB").to_string();
            let Some(j) = st.job_mut(&q) else {
                return fail("CPF1070", &[&q]);
            };
            if j.user != user && !jobctl {
                return fail("CPF1344", &[&q]);
            }
            match name {
                "HLDJOB" if q == "123460/QSECOFR/QPADEV0001" => fail("CPF1317", &[&q, "Hold"]),
                "HLDJOB" if j.status == "HLD" => fail("CPF1035", &[&q]),
                "HLDJOB" => {
                    j.status = "HLD".into();
                    done("CPC1117", &[&q])
                }
                "RLSJOB" if j.status != "HLD" => fail("CPF1036", &[&q]),
                "RLSJOB" => {
                    j.status = "RUN".into();
                    done("CPC1118", &[&q])
                }
                "ENDJOB" => {
                    st.jobs.retain(|j| j.qualified() != q);
                    st.changed.remove(&q);
                    done("CPC1125", &[&q])
                }
                _ => {
                    let mut props = st.props(&q).unwrap_or_default();
                    if let Ok(n) = get("RUNPTY").parse() {
                        props.run_priority = n;
                    }
                    if let Ok(n) = get("TIMESLICE").parse() {
                        props.time_slice = n;
                    }
                    if p.contains_key("LOG") {
                        props.logging = get("LOG").to_string();
                    }
                    if p.contains_key("LOGCLPGM") {
                        props.log_cl = get("LOGCLPGM") == "*YES";
                    }
                    if p.contains_key("INQMSGRPY") {
                        props.inquiry = get("INQMSGRPY").to_string();
                    }
                    st.changed.insert(q.clone(), props);
                    done("CPC1128", &[&q])
                }
            }
        }
        "DSPJOBLOG" => done("CPC1234", &[get("JOB")]),
        "HLDJOBQ" | "RLSJOBQ" | "CLRJOBQ" => {
            if !jobctl {
                return fail("CPF2207", &[get("JOBQ"), "", "*JOBQ"]);
            }
            let (lib, q) = split(get("JOBQ"), "QGPL");
            let (lib, q) = (lib.to_string(), q.to_string());
            let Some(jq) = st.jobqs.iter_mut().find(|j| j.name == q && j.lib == lib) else {
                return fail("CPF3307", &[&q, &lib]);
            };
            match name {
                "HLDJOBQ" if jq.held => fail("CPF3425", &[&q, &lib]),
                "HLDJOBQ" => {
                    jq.held = true;
                    done("CPC3420", &[&q, &lib])
                }
                "RLSJOBQ" if !jq.held => fail("CPF3426", &[&q, &lib]),
                "RLSJOBQ" => {
                    jq.held = false;
                    done("CPC3421", &[&q, &lib])
                }
                _ => {
                    jq.waiting = 0;
                    jq.held_jobs = 0;
                    jq.scheduled = 0;
                    done("CPC3423", &[&q, &lib])
                }
            }
        }
        "STRSBS" | "ENDSBS" => {
            if !jobctl {
                return fail("CPF2207", &[get("SBSD"), "", "*SBSD"]);
            }
            let q = if name == "STRSBS" {
                get("SBSD")
            } else {
                get("SBS")
            }
            .to_string();
            let Some(d) = st.sbs_mut(&q) else {
                return fail("CPF1080", &[&q]);
            };
            let n = d.sbs.name.clone();
            if name == "STRSBS" {
                if d.sbs.active {
                    return fail("CPF1010", &[&n]);
                }
                d.sbs.active = true;
                d.since = Some(super::now());
                d.monitor = Some(format!("124001/QSYS/{n}"));
                done("CPC1110", &[&n, &d.sbs.lib.clone()])
            } else {
                if !d.sbs.active {
                    return fail("CPF1054", &[&n]);
                }
                d.sbs.active = false;
                d.sbs.jobs = 0;
                d.since = None;
                d.monitor = None;
                st.jobs.retain(|j| j.subsystem != n);
                done("CPC1112", &[&n])
            }
        }
        "CHGSBSD" | "ADDRTGE" | "CHGRTGE" | "RMVRTGE" | "ADDAJE" | "CHGAJE" | "RMVAJE" => {
            if !jobctl {
                return fail("CPF2207", &[get("SBSD"), "", "*SBSD"]);
            }
            let q = get("SBSD").to_string();
            let Some(d) = st.sbs_mut(&q) else {
                return fail("CPF1080", &[&q]);
            };
            let n = d.sbs.name.clone();
            let lib = d.sbs.lib.clone();
            let seq: u32 = get("SEQNBR").parse().unwrap_or(0);
            match name {
                "CHGSBSD" => {
                    if p.contains_key("MAXJOBS") {
                        d.sbs.max_jobs = get("MAXJOBS").parse().ok();
                    }
                    if p.contains_key("SGNDSPF") {
                        d.signon = get("SGNDSPF").to_string();
                    }
                    if p.contains_key("TEXT") {
                        d.sbs.text = unquote(get("TEXT"));
                    }
                    if let Some(pools) = p.get("POOLS") {
                        let inner = pools.trim().trim_start_matches('(').trim_end_matches(')');
                        let parts: Vec<&str> = inner.split_whitespace().collect();
                        let id: u32 = parts.first().and_then(|x| x.parse().ok()).unwrap_or(0);
                        if parts.get(1) == Some(&"*RMV") {
                            d.pools.retain(|x| x.id != id);
                        } else if let Some(storage) = parts.get(1) {
                            let pool = crate::env::work::SbsPool {
                                id,
                                storage: (*storage).to_string(),
                                activity: parts.get(2).and_then(|a| a.parse().ok()),
                                system_pool: 2,
                                size_mb: 32_768,
                                ..crate::env::work::SbsPool::default()
                            };
                            match d.pools.iter_mut().find(|x| x.id == id) {
                                Some(x) => *x = pool,
                                None => d.pools.push(pool),
                            }
                        }
                        d.sbs.pools = d.pools.iter().map(|x| x.storage.clone()).collect();
                    }
                    done("CPC1601", &[&n, &lib])
                }
                "ADDRTGE" | "CHGRTGE" => {
                    let exists = d.routing.iter().any(|r| r.seq == seq);
                    if name == "ADDRTGE" && exists {
                        return fail("CPF1697", &[&seq.to_string(), &n]);
                    }
                    if name == "CHGRTGE" && !exists {
                        return fail("CPF1698", &[&seq.to_string(), &n]);
                    }
                    let (pl, pn) = split(get("PGM"), "QSYS");
                    let r = Routing {
                        seq,
                        compare: unquote(get("CMPVAL")),
                        start: 1,
                        program: pn.to_string(),
                        lib: pl.to_string(),
                        class: get("CLS").to_string(),
                        max_active: get("MAXACT").parse().ok(),
                        pool: get("POOLID").parse().unwrap_or(1),
                    };
                    d.routing.retain(|x| x.seq != seq);
                    d.routing.push(r);
                    d.routing.sort_by_key(|x| x.seq);
                    done(
                        if name == "ADDRTGE" {
                            "CPC1602"
                        } else {
                            "CPC1603"
                        },
                        &[&seq.to_string(), &n],
                    )
                }
                "RMVRTGE" => {
                    if !d.routing.iter().any(|r| r.seq == seq) {
                        return fail("CPF1698", &[&seq.to_string(), &n]);
                    }
                    d.routing.retain(|x| x.seq != seq);
                    done("CPC1604", &[&seq.to_string(), &n])
                }
                _ => {
                    let job = get("JOB").to_string();
                    let exists = d.autostart.iter().any(|a| a.job == job);
                    if name == "ADDAJE" && exists {
                        return fail("CPF1699", &[&job, &n]);
                    }
                    if name != "ADDAJE" && !exists {
                        return fail("CPF1700", &[&job, &n]);
                    }
                    d.autostart.retain(|a| a.job != job);
                    if name != "RMVAJE" {
                        let (jl, jn) = split(get("JOBD"), "QGPL");
                        d.autostart.push(crate::env::work::Autostart {
                            job: job.clone(),
                            jobd: jn.to_string(),
                            lib: jl.to_string(),
                        });
                    }
                    let id = match name {
                        "ADDAJE" => "CPC1605",
                        "CHGAJE" => "CPC1606",
                        _ => "CPC1607",
                    };
                    done(id, &[&job, &n])
                }
            }
        }
        "HLDOUTQ" | "RLSOUTQ" | "CLROUTQ" => {
            if !jobctl {
                return fail("CPF2207", &[get("OUTQ"), "", "*OUTQ"]);
            }
            let (lib, q) = split(get("OUTQ"), "QGPL");
            let (lib, q) = (lib.to_string(), q.to_string());
            let Some(oq) = st.outqs.iter_mut().find(|o| o.name == q && o.lib == lib) else {
                return fail("CPF3357", &[&q, &lib]);
            };
            match name {
                "HLDOUTQ" => {
                    oq.held = true;
                    done("CPC3330", &[&q, &lib])
                }
                "RLSOUTQ" => {
                    oq.held = false;
                    done("CPC3331", &[&q, &lib])
                }
                _ => {
                    oq.files = 0;
                    let full = format!("{lib}/{q}");
                    let before = st.splfs.len();
                    st.splfs.retain(|s| s.outq != full);
                    done(
                        "CPC3332",
                        &[&q, &lib, &(before - st.splfs.len()).to_string()],
                    )
                }
            }
        }
        "HLDSPLF" | "RLSSPLF" | "DLTSPLF" | "CHGSPLFA" => {
            let file = get("FILE").to_string();
            let nbr = get("SPLNBR").to_string();
            let target = get("OUTQ").to_string();
            let Some(f) = st.splf_mut(p) else {
                return fail("CPF3303", &[&file, get("JOB")]);
            };
            if f.user != user && !jobctl && !special(user, "*SPLCTL") {
                return fail("CPF3492", &[&file, &nbr]);
            }
            match name {
                "HLDSPLF" => {
                    f.status = "HLD".into();
                    done("CPC3301", &[&file, &nbr])
                }
                "RLSSPLF" => {
                    f.status = "RDY".into();
                    done("CPC3302", &[&file, &nbr])
                }
                "CHGSPLFA" => {
                    let from = f.outq.clone();
                    let (tl, tq) = split(&target, "QGPL");
                    let to = format!("{tl}/{tq}");
                    f.outq.clone_from(&to);
                    for o in &mut st.outqs {
                        let q = format!("{}/{}", o.lib, o.name);
                        if q == from {
                            o.files = o.files.saturating_sub(1);
                        } else if q == to {
                            o.files += 1;
                        }
                    }
                    done("CPC3304", &[&file, &nbr, &to])
                }
                _ => {
                    let from = f.outq.clone();
                    let job = get("JOB").to_string();
                    st.splfs.retain(|s| {
                        !(s.file == file && s.job == job && s.number.to_string() == nbr)
                    });
                    for o in &mut st.outqs {
                        if format!("{}/{}", o.lib, o.name) == from {
                            o.files = o.files.saturating_sub(1);
                        }
                    }
                    done("CPC3303", &[&file, &nbr])
                }
            }
        }
        "STRPRTWTR" | "STRRMTWTR" | "ENDWTR" | "HLDWTR" | "RLSWTR" => {
            if !jobctl {
                return fail("CPF2207", &[get("DEV"), "", "*DEVD"]);
            }
            let w = match name {
                "STRPRTWTR" => get("DEV").to_string(),
                "STRRMTWTR" => st
                    .writers
                    .iter()
                    .find(|x| x.outq == get("OUTQ"))
                    .map(|x| x.device.clone())
                    .unwrap_or_default(),
                _ => get("WTR").to_string(),
            };
            let Some(wr) = st.writers.iter_mut().find(|x| x.device == w) else {
                return fail("CPF3310", &[&w]);
            };
            match name {
                "STRPRTWTR" | "STRRMTWTR" if wr.started => fail("CPF3310", &[&w]),
                "STRPRTWTR" if wr.varied_on == Some(false) => fail("CPF3317", &[&w]),
                "STRPRTWTR" | "STRRMTWTR" => {
                    wr.started = true;
                    if let Some(oq) = p.get("OUTQ").filter(|o| !o.is_empty() && *o != "*DEV") {
                        wr.outq.clone_from(oq);
                    }
                    done("CPC3305", &[&w])
                }
                _ if !wr.started => fail("CPF3313", &[&w]),
                "ENDWTR" => {
                    wr.started = false;
                    wr.held = false;
                    wr.file.clear();
                    wr.user.clear();
                    wr.pages_left = None;
                    done("CPC3306", &[&w])
                }
                "HLDWTR" => {
                    wr.held = true;
                    done("CPC3307", &[&w])
                }
                _ => {
                    wr.held = false;
                    done("CPC3308", &[&w])
                }
            }
        }
        "VRYCFG" => {
            if !jobctl {
                return fail("CPF2207", &[get("CFGOBJ"), "QSYS", "*DEVD"]);
            }
            let dev = get("CFGOBJ").to_string();
            let on = get("STATUS") == "*ON";
            let Some(wr) = st
                .writers
                .iter_mut()
                .find(|x| x.device == dev && x.varied_on.is_some())
            else {
                return fail("CPF2702", &[&dev]);
            };
            if !on && wr.started {
                return fail("CPF2706", &[&dev]);
            }
            wr.varied_on = Some(on);
            done(if on { "CPC2605" } else { "CPC2606" }, &[&dev])
        }
        "ADDJOBSCDE" | "CHGJOBSCDE" | "HLDJOBSCDE" | "RLSJOBSCDE" | "RMVJOBSCDE" => {
            let jobname = get("JOB").to_string();
            if name == "ADDJOBSCDE" {
                if jobname.is_empty() || get("CMD").is_empty() {
                    return fail("CPF1629", &[&jobname]);
                }
                let entry = st.schedule.iter().map(|e| e.entry).max().unwrap_or(0) + 1;
                let mut e = ScheduleEntry {
                    name: jobname.clone(),
                    entry,
                    user: user.to_string(),
                    jobq: "QGPL/QBATCH".into(),
                    ..ScheduleEntry::default()
                };
                apply_scde(&mut e, p);
                st.schedule.push(e);
                return done("CPC1238", &[&jobname, &entry.to_string()]);
            }
            let Some(e) = st.entry_mut(p) else {
                return fail("CPF1630", &[&jobname]);
            };
            if e.user != user && !jobctl {
                return fail("CPF1637", &[&jobname]);
            }
            let n = e.entry.to_string();
            match name {
                "CHGJOBSCDE" => {
                    apply_scde(e, p);
                    done("CPC1239", &[&jobname, &n])
                }
                "HLDJOBSCDE" => {
                    e.held = true;
                    done("CPC1240", &[&jobname, &n])
                }
                "RLSJOBSCDE" => {
                    e.held = false;
                    done("CPC1241", &[&jobname, &n])
                }
                _ => {
                    st.schedule
                        .retain(|x| !(x.name == jobname && x.entry.to_string() == n));
                    done("CPC1242", &[&jobname, &n])
                }
            }
        }
        "SBMJOB" => {
            let jobname = get("JOB").to_string();
            let q = get("JOBQ").to_string();
            let (ql, qn) = split(&q, "QGPL");
            let (ql, qn) = (ql.to_string(), qn.to_string());
            if let Some(jq) = st.jobqs.iter_mut().find(|j| j.name == qn && j.lib == ql) {
                jq.waiting += 1;
            }
            done("CPC1221", &[&format!("124100/{user}/{jobname}"), &qn, &ql])
        }
        _ => fail("CPD0030", &[name]),
    }
}

/// `ADDJOBSCDE`/`CHGJOBSCDE` parameters onto `e`.
fn apply_scde(e: &mut ScheduleEntry, p: &BTreeMap<String, String>) {
    if let Some(c) = p.get("CMD") {
        e.command = c.trim().to_string();
    }
    if let Some(f) = p.get("FRQ") {
        e.frequency.clone_from(f);
    }
    if let Some(d) = p.get("SCDDATE") {
        e.date = parse_date(d);
    }
    if let Some(d) = p.get("SCDDAY") {
        e.days = d
            .split_whitespace()
            .filter(|d| *d != "*NONE")
            .map(ToString::to_string)
            .collect();
    }
    if let Some(t) = p.get("SCDTIME") {
        let t = unquote(t).replace(':', "");
        if t.len() >= 4 {
            e.time = format!("{}:{}", &t[..2], &t[2..4]);
        }
    }
    if let Some(q) = p.get("JOBQ") {
        e.jobq.clone_from(q);
    }
    if let Some(t) = p.get("TEXT") {
        e.text = unquote(t);
    }
    let (h, m): (u32, u32) = e.time.split_once(':').map_or((0, 0), |(h, m)| {
        (h.parse().unwrap_or(0), m.parse().unwrap_or(0))
    });
    let base = e.date.unwrap_or(secs(2026, 10, 30, 0, 0, 0));
    e.next = Some(base + u64::from(h) * 3600 + u64::from(m) * 60);
}

/// `MM/DD/YY` as seconds since 1970, `None` for `*NONE` or bad dates.
fn parse_date(d: &str) -> Option<u64> {
    let d = unquote(d);
    let mut it = d.split('/').map(|x| x.parse::<u32>().ok());
    let (m, day, y) = (it.next()??, it.next()??, it.next()??);
    if !(1..=12).contains(&m) || !(1..=31).contains(&day) {
        return None;
    }
    Some(secs(2000 + i64::from(y % 100), m, day, 0, 0, 0))
}

impl Work for FixtureWork {
    fn active_jobs(&self, user: &str, _reset: bool) -> ActiveJobs {
        let mut st = self.state.lock().expect("work");
        let notes = st.notes.remove(user).unwrap_or_default();
        ActiveJobs {
            cpu: 234,
            elapsed: 60,
            jobs: st.jobs.clone(),
            notes,
        }
    }

    fn choices(&self, _user: &str, what: Choices) -> Vec<String> {
        let v: &[&str] = match what {
            Choices::Subsystems => &[
                "QINTER", "QBATCH", "QHTTPSVR", "QSYSWRK", "QUSRWRK", "QCTL", "QSPL",
            ],
            Choices::JobqLibraries => &["QGPL", "QSYS", "QUSRSYS", "QHTTPSVR"],
            Choices::OutqLibraries => &["QGPL", "QUSRSYS", "QSYS"],
        };
        v.iter().map(ToString::to_string).collect()
    }

    fn job(&self, _user: &str, job: &str) -> Option<JobProps> {
        self.state.lock().expect("work").props(job)
    }

    fn job_log(&self, _user: &str, j: &str) -> Vec<LogEntry> {
        self.state
            .lock()
            .expect("work")
            .props(j)
            .map(|p| job::log(&p))
            .unwrap_or_default()
    }

    fn library_list(&self, _user: &str, j: &str) -> Vec<LibEntry> {
        self.state
            .lock()
            .expect("work")
            .props(j)
            .map(|p| job::libl(&p))
            .unwrap_or_default()
    }

    fn locks(&self, _user: &str, j: &str) -> Vec<Lock> {
        self.state
            .lock()
            .expect("work")
            .props(j)
            .map(|p| job::locks(&p))
            .unwrap_or_default()
    }

    fn call_stack(&self, _user: &str, j: &str) -> Vec<Thread> {
        self.state
            .lock()
            .expect("work")
            .props(j)
            .map(|p| job::stack(&p))
            .unwrap_or_default()
    }

    fn job_queues(&self, _user: &str) -> Vec<JobQueue> {
        self.state.lock().expect("work").jobqs.clone()
    }

    fn subsystems(&self, _user: &str) -> Vec<Subsystem> {
        self.state
            .lock()
            .expect("work")
            .sbs
            .iter()
            .map(|d| d.sbs.clone())
            .collect()
    }

    fn subsystem(&self, _user: &str, name: &str) -> Option<SubsystemDesc> {
        self.state
            .lock()
            .expect("work")
            .sbs
            .iter()
            .find(|d| d.sbs.name == name)
            .cloned()
    }

    fn output_queues(&self, _user: &str) -> Vec<OutQueue> {
        self.state.lock().expect("work").outqs.clone()
    }

    fn spooled_files(&self, _user: &str, outq: &str, job: &str) -> Vec<Splf> {
        let st = self.state.lock().expect("work");
        st.splfs
            .iter()
            .filter(|s| {
                if job.is_empty() {
                    s.outq == outq
                } else {
                    s.job == job
                }
            })
            .cloned()
            .collect()
    }

    fn spooled_text(
        &self,
        _user: &str,
        job: &str,
        file: &str,
        number: u32,
    ) -> Option<(Splf, Vec<String>)> {
        let st = self.state.lock().expect("work");
        let f = st
            .splfs
            .iter()
            .find(|s| s.job == job && s.file == file && s.number == number)?
            .clone();
        let pages = (1..=f.pages.max(1)).map(|n| page_text(&f, n)).collect();
        Some((f, pages))
    }

    fn spooled_pdf(&self, user: &str, job: &str, file: &str, number: u32) -> Option<Vec<u8>> {
        let (_, pages) = self.spooled_text(user, job, file, number)?;
        let pages: Vec<spool::scs::Page> = pages
            .iter()
            .map(|t| {
                let mut p = spool::scs::Page::default();
                for (i, l) in t.lines().enumerate() {
                    p.lines
                        .insert(u16::try_from(i + 1).unwrap_or(u16::MAX), l.to_string());
                }
                p
            })
            .collect();
        Some(spool::pdf::render(
            &pages,
            spool::scs::decode::Format::default(),
        ))
    }

    fn writers(&self, _user: &str) -> Vec<Writer> {
        self.state.lock().expect("work").writers.clone()
    }

    fn schedule(&self, _user: &str) -> Vec<ScheduleEntry> {
        self.state.lock().expect("work").schedule.clone()
    }

    fn run(&self, user: &str, command: &str) -> Result<Message, Message> {
        self.ran.lock().expect("ran").push(command.to_string());
        let (name, p) = parse(command);
        let mut st = self.state.lock().expect("work");
        run_in(&mut st, user, &name, &p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_parse() {
        let (n, p) = parse("chgjob JOB(1/A/B) LOG(4 00 *SECLVL) TEXT('a (b)') LOGCLPGM(*YES)");
        assert_eq!(n, "CHGJOB");
        assert_eq!(p["JOB"], "1/A/B");
        assert_eq!(p["LOG"], "4 00 *SECLVL");
        assert_eq!(unquote(&p["TEXT"]), "a (b)");
        assert_eq!(p["LOGCLPGM"], "*YES");
        let (_, p) = parse("CHGSBSD SBSD(QSYS/X) POOLS((2 *SHRPOOL1 10))");
        assert_eq!(p["POOLS"], "(2 *SHRPOOL1 10)");
        assert_eq!(parse_date("11/15/26"), Some(secs(2026, 11, 15, 0, 0, 0)));
    }

    #[test]
    fn counts() {
        let w = FixtureWork::default();
        let st = w.state.lock().unwrap();
        assert_eq!(st.jobs.len(), 318);
        assert_eq!(st.jobs.iter().filter(|j| j.status == "MSGW").count(), 2);
        assert_eq!(
            st.splfs.iter().filter(|s| s.outq == "QGPL/QPRINT").count(),
            148
        );
        for s in &st.sbs {
            let n = st.jobs.iter().filter(|j| j.subsystem == s.sbs.name).count();
            assert_eq!(n, s.sbs.jobs as usize, "{}", s.sbs.name);
        }
    }
}
