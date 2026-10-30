// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The TCP servers framework (`net/servers`) in the job: [`JobEnv`]
//! gives the servers' hooks this job's reach (server jobs start as its
//! children, like subsystem monitors; attributes come from the storage
//! service's server table), and the commands `STRTCPSVR`, `ENDTCPSVR`
//! and `CHGTELNA` run here. `STRTCP` and `ENDTCP` (`tcp.rs`) start and
//! end the autostart servers through the same framework.

use std::fmt::Write as _;

use cmd::{Resolved, Value};
use mi::Job;
use servers::attrs::telnet::{self, TelnetAttrs};
use servers::{Env, Framework, Instance, JobStart};
use stg::Client;
use stg::tcp::{ServerInfo, TcpOp, TcpReply};
use tcpcfg::model::svr::ServerAttrs;

use super::{Fail, parms_of};
use crate::console::host::Host;

/// The servers this release runs. A server built later registers its
/// hooks here and its job's role in `start.rs`.
pub fn framework() -> Framework {
    let mut f = Framework::standard();
    f.register(Box::new(servers::framework::Ntp));
    f.register(Box::new(servers::framework::Lpd));
    f
}

/// Every server's autostart and attributes from the service.
pub fn server_table(host: &Host) -> Vec<ServerInfo> {
    match host.link.borrow_mut().with(|l| l.tcp(TcpOp::Servers)) {
        TcpReply::Servers { list } => list,
        _ => Vec::new(),
    }
}

/// One server's attributes from the service's table (defaults when
/// it has no entry).
pub fn server_attrs(host: &Host, server: &str) -> ServerAttrs {
    let found = server_table(host)
        .into_iter()
        .find(|s| s.server.eq_ignore_ascii_case(server));
    match found {
        Some(s) => ServerAttrs {
            server: s.server,
            autostart: s.autostart,
            bag: s.bag,
        },
        None => tcpcfg::model::svr::Servers::default()
            .get(server)
            .cloned()
            .unwrap_or(ServerAttrs {
                server: server.to_string(),
                autostart: false,
                bag: Vec::new(),
            }),
    }
}

/// The hooks' view of this job.
pub struct JobEnv<'a> {
    job: &'a mut Job,
    host: &'a Host,
}

impl<'a> JobEnv<'a> {
    /// This job's.
    pub fn new(job: &'a mut Job, host: &'a Host) -> Self {
        Self { job, host }
    }

    fn state(&self, name: &str) -> u8 {
        self.host.link.borrow_mut().with(|l| l.job_state(name.into()))
    }

    fn command(&mut self, line: &str) -> Result<String, String> {
        let out = super::run(self.job, self.host, line);
        match out.escape {
            Some(f) => Err(if f.text.is_empty() { f.msgid } else { f.text }),
            None => Ok(out.text),
        }
    }
}

impl Env for JobEnv<'_> {
    fn active(&mut self, job: &str) -> bool {
        self.state(job) == 1
    }

    fn start_job(&mut self, j: &JobStart) -> Result<(), String> {
        let number = self
            .host
            .link
            .borrow_mut()
            .with(Client::job_number)?;
        let start = wm::ctl::Start {
            name: j.name.clone(),
            user: j.user.clone(),
            number,
            handle: u64::from(number),
            kind: 'B',
            subsystem: j.subsystem.clone(),
            role: j.role.clone(),
            jobd: "QSYS/QSYSJOBD".into(),
            class: "QSYS/QSYSCLS20".into(),
            pool: "*BASE".into(),
            rqsdta: j.rqsdta.clone(),
            ..wm::ctl::Start::default()
        };
        let mut s = crate::work::spawn::job(start)?;
        crate::work::spawn::ready(&mut s)?;
        self.host.children.borrow_mut().push(s);
        Ok(())
    }

    fn end_job(&mut self, job: &str, immediate: bool) -> Result<(), String> {
        // A server job this job started is told over its channel: the
        // ENDJOB request goes to subsystem monitors, which do not hold it.
        {
            let mut children = self.host.children.borrow_mut();
            children.retain_mut(|s| matches!(s.child.try_wait(), Ok(None)));
            if let Some(s) = children.iter_mut().find(|s| s.start.name.eq_ignore_ascii_case(job)) {
                return s
                    .chan
                    .send(&wm::ctl::ToJob::End { immediate })
                    .map_err(|e| e.to_string());
            }
        }
        let option = if immediate { "*IMMED" } else { "*CNTRLD" };
        self.command(&format!("ENDJOB JOB({job}) OPTION({option})")).map(|_| ())
    }

    fn attrs(&mut self, server: &str) -> ServerAttrs {
        server_attrs(self.host, server)
    }

    fn start_subsystem(&mut self, name: &str) -> Result<(), String> {
        if self.state(name) == 1 {
            return Ok(());
        }
        self.command(&format!("STRSBS SBSD(QSYS/{name})")).map(|_| ())
    }
}

/// The text of a value: a number, a name, `LIB/NAME`.
fn text(v: &Value) -> String {
    match v {
        Value::Char(s) => s.trim().to_string(),
        // A single value of a qualified name (`*SAME`) stands alone.
        Value::Qual(parts) if parts.first().is_some_and(|p| p.source().starts_with('*')) => {
            parts[0].source().trim().to_string()
        }
        other => other.source().trim().to_string(),
    }
}

/// `SERVER`'s list.
fn server_list(r: &Resolved) -> Vec<String> {
    let parms = parms_of(r);
    let list: Vec<String> = parms
        .list("SERVER")
        .into_iter()
        .map(|v| text(v).to_ascii_uppercase())
        .filter(|s| !s.is_empty())
        .collect();
    if list.is_empty() { vec!["*ALL".into()] } else { list }
}

/// The instance `HTTPSVR` and `INSTANCE` name.
fn instance(r: &Resolved) -> Instance {
    let parms = parms_of(r);
    let http = match parms.get("HTTPSVR") {
        Some(Value::Elems(items)) => items.first().cloned().flatten().map(|v| text(&v)),
        Some(v) => Some(text(v)),
        None => None,
    }
    .map(|s| s.to_ascii_uppercase())
    .unwrap_or_default();
    let inst = parms.name("INSTANCE");
    let pick = if !http.is_empty() && http != "*ALL" {
        http
    } else if !inst.is_empty() && inst != "*DFT" {
        inst
    } else {
        http
    };
    match pick.as_str() {
        "" | "*DFT" => Instance::Default,
        "*ALL" => Instance::All,
        "*AUTOSTART" => Instance::Autostart,
        name => Instance::Named(name.to_string()),
    }
}

/// The answers as the command's output; an escape when nothing went
/// and something was refused.
fn report(answers: Vec<(String, Result<String, servers::Refusal>)>) -> Result<String, Fail> {
    let mut out = String::new();
    let mut first_refusal = None;
    let mut any_ok = false;
    for (_, r) in answers {
        match r {
            Ok(t) => {
                any_ok = true;
                if !t.is_empty() {
                    let _ = writeln!(out, "{t}");
                }
            }
            Err(e) => {
                let _ = writeln!(out, "{}", e.line());
                first_refusal.get_or_insert(e);
            }
        }
    }
    match first_refusal {
        Some(e) if !any_ok => Err(Fail::new(&e.msgid, e.line())),
        _ => Ok(out),
    }
}

/// `STRTCPSVR SERVER RESTART HTTPSVR INSTANCE`.
pub fn strtcpsvr(job: &mut Job, host: &Host, r: &Resolved) -> Result<String, Fail> {
    let fw = framework();
    let list = server_list(r);
    let inst = instance(r);
    let restart = parms_of(r).name("RESTART");
    let mut env = JobEnv::new(job, host);
    let mut lead = String::new();
    if restart == "*HTTP" {
        for (_, a) in fw.run(&mut env, &["*HTTP".to_string()], &inst, false, false) {
            if let Ok(t) = a.as_ref()
                && !t.is_empty()
            {
                let _ = writeln!(lead, "{t}");
            }
        }
    }
    let out = report(fw.run(&mut env, &list, &inst, true, false))?;
    Ok(lead + &out)
}

/// `ENDTCPSVR SERVER HTTPSVR INSTANCE`.
pub fn endtcpsvr(job: &mut Job, host: &Host, r: &Resolved) -> Result<String, Fail> {
    let fw = framework();
    let list = server_list(r);
    let inst = instance(r);
    let mut env = JobEnv::new(job, host);
    report(fw.run(&mut env, &list, &inst, false, false))
}

/// `CHGTELNA`: the attributes into the server table, checked first.
pub fn chgtelna(host: &Host, r: &Resolved) -> Result<String, Fail> {
    let parms = parms_of(r);
    let current = server_attrs(host, "*TELNET");
    let mut attrs = TelnetAttrs::from_bag(&current.bag);
    for kwd in telnet::KEYWORDS {
        let Some(v) = parms.get(kwd) else {
            continue;
        };
        attrs
            .set(kwd, &text(v))
            .map_err(|e| Fail::new(servers::msg::BAD_VALUE, format!("Value not valid: {e}")))?;
    }
    let autostart = match parms.name("AUTOSTART").as_str() {
        "*YES" => Some(true),
        "*NO" => Some(false),
        _ => None,
    };
    let op = TcpOp::SetServer {
        server: "*TELNET".into(),
        autostart,
        bag: attrs.to_bag(),
    };
    match host.link.borrow_mut().with(|l| l.tcp(op)) {
        TcpReply::Done { .. } => Ok(format!("{} TELNET attributes changed.\n", servers::msg::ATTRS_CHANGED)),
        TcpReply::Failed { note } => Err(Fail::new(&note.msgid, note.line())),
        other => Err(Fail::new("CPF9898", format!("unexpected answer {other:?}"))),
    }
}

/// The server commands; `None` for another command.
pub fn run(job: &mut Job, host: &Host, r: &Resolved) -> Option<Result<String, Fail>> {
    Some(match r.name.as_str() {
        "STRTCPSVR" => strtcpsvr(job, host, r),
        "ENDTCPSVR" => endtcpsvr(job, host, r),
        "CHGTELNA" => chgtelna(host, r),
        _ => return None,
    })
}
