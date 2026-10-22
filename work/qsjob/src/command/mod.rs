// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Running a command string in the job: the analyzer resolves it
//! against the `*CMD` objects, then the processing program runs where
//! the inventory says: in this job (calls, the debugger, the job's own
//! displays), in the storage service over the store, or, for a user's
//! command, as a program called with the parameter block. Every
//! message a command produces goes to the job log; the text goes back
//! to whoever typed the line. `svc.` lines reach the storage service's
//! own console table (the development commands).

pub mod jobsite;
pub mod natives;
pub mod prompt;

use std::rc::Rc;

use cmd::analyze::{Env, Setting};
use cmd::block::{self, Parms};
use cmd::def::Mode;
use cmd::{CmdError, Diag, Resolved};
use mi::Job;
use qir::MsgType;
use stg::Client;
use stg::proto::CmdReply;
use sys_cpps::Site;

use crate::console::host::Host;

/// A message that ended a command.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fail {
    /// The identifier.
    pub msgid: String,
    /// Its data.
    pub data: Vec<u8>,
    /// Text to show instead of the description's, when there is any.
    pub text: String,
}

impl Fail {
    /// From an identifier and detail text.
    #[must_use]
    pub fn new(msgid: &str, text: impl Into<String>) -> Self {
        Self {
            msgid: msgid.into(),
            data: Vec::new(),
            text: text.into(),
        }
    }
}

/// What running a line produced.
#[derive(Clone, Debug, Default)]
pub struct Outcome {
    /// Output for the console.
    pub text: String,
    /// The escape that ended the command, if one did.
    pub escape: Option<Fail>,
    /// The command name, upper case, once resolved.
    pub name: String,
    /// The command string as analyzed (what `QCMDCHK` returns).
    pub changed: String,
    /// The console state the service reported, when it did.
    pub prompt: Option<String>,
    /// SRC codes raised.
    pub srcs: Vec<String>,
    /// The system values when they changed.
    pub sysvals: Option<Vec<u8>>,
    /// The parameter block as the processing program left it, for
    /// `RTNVAL(*YES)` parameters.
    pub segments: Option<Vec<cmd::Segment>>,
}

/// The mode: run, check only, prompt then run, prompt and return.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunMode {
    /// Run.
    Run,
    /// Check the syntax only.
    Check,
    /// Prompt, then run.
    PromptRun,
    /// Prompt and return the changed string.
    PromptReturn,
}

impl RunMode {
    /// From `QCAPCMD`'s code.
    #[must_use]
    pub fn from_code(c: u8) -> Self {
        match c {
            1 => Self::Check,
            2 => Self::PromptRun,
            3 => Self::PromptReturn,
            _ => Self::Run,
        }
    }
}

/// Run a line typed at a console or command line.
pub fn run(job: &mut Job, host: &Host, text: &str) -> Outcome {
    let setting = Setting {
        interactive: host.interactive(),
        ..Setting::default()
    };
    run_with(job, host, text, RunMode::Run, setting)
}

/// Run a line from `QCMDEXC` or a program.
pub fn run_exec(job: &mut Job, host: &Host, text: &str, mode: RunMode) -> Outcome {
    let setting = Setting {
        interactive: host.interactive(),
        exec: true,
        ..Setting::default()
    };
    run_with(job, host, text, mode, setting)
}

/// Run a line in a setting.
pub fn run_with(
    job: &mut Job,
    host: &Host,
    text: &str,
    mode: RunMode,
    setting: Setting,
) -> Outcome {
    let text = text.trim();
    if let Some(rest) = text.strip_prefix("svc.") {
        return passthrough(job, host, rest);
    }
    let resolved = {
        let mut cmds = host.cmds.borrow_mut();
        let attrs = host.attrs();
        let mut env = Env::new(&mut *cmds);
        env.setting = setting;
        env.mode = Mode::PROD
            | if job.debug.is_some() { Mode::DEBUG } else { 0 }
            | if host.start.role == "QSVCJOB" {
                Mode::SERVICE
            } else {
                0
            };
        env.limited = host.limited();
        env.opts.decfmt = attrs.decfmt.chars().next().unwrap_or(' ');
        env.datfmt.clone_from(&attrs.datfmt);
        env.datsep = attrs.datsep.chars().next().unwrap_or('/');
        cmd::analyze(&mut env, text)
    };
    let r = match resolved {
        Ok(r) => r,
        // A line of the old console table, until every harness script
        // says `svc.` (`MIGRATE(phase7)`: the shim goes at the end of
        // the phase).
        Err(e) if e.msgid() == "CPD0030" && is_old_word(text) => {
            return passthrough(job, host, text);
        }
        Err(e) => return failed(job, &e),
    };
    let mut out = Outcome {
        name: r.name.clone(),
        changed: r.text(),
        ..Outcome::default()
    };
    if matches!(mode, RunMode::Check | RunMode::PromptReturn) && !r.prompt && !r.selective() {
        return out;
    }
    let r = if r.prompt
        || r.selective()
        || matches!(mode, RunMode::PromptRun | RunMode::PromptReturn)
    {
        match prompt(job, host, &r, mode) {
            Ok(Some(r)) => r,
            Ok(None) => {
                let f = Fail::new("CPF6801", "");
                log(job, &f.msgid, &f.data, MsgType::Escape, None);
                out.escape = Some(f);
                return out;
            }
            Err(f) => {
                log(job, &f.msgid, &f.data, MsgType::Escape, Some(&f.text));
                out.escape = Some(f);
                return out;
            }
        }
    } else {
        r
    };
    out.changed = r.text();
    if mode == RunMode::PromptReturn {
        return out;
    }
    dispatch(job, host, &r, out)
}

/// A prompted command: the resolved command after the user changed
/// it, `None` when the prompt was cancelled.
fn prompt(
    job: &mut Job,
    host: &Host,
    r: &Resolved,
    _mode: RunMode,
) -> Result<Option<Resolved>, Fail> {
    if !host.has_display() {
        return Err(Fail::new("CPF6802", ""));
    }
    prompt::session(job, host, r)
}

/// Run a resolved command where its processing program lives.
/// Run a command already resolved (a processing program called by
/// name with its block).
pub fn dispatch_resolved(job: &mut Job, host: &Host, r: &Resolved) -> Outcome {
    let out = Outcome {
        name: r.name.clone(),
        changed: r.text(),
        ..Outcome::default()
    };
    dispatch(job, host, r, out)
}

fn dispatch(job: &mut Job, host: &Host, r: &Resolved, mut out: Outcome) -> Outcome {
    let entry = (r.lib == "QSYS")
        .then(|| sys_cpps::entry(&r.name))
        .flatten();
    let result = match entry.map(|e| e.site) {
        Some(Site::Job) => jobsite::run(job, host, r),
        Some(Site::Service | Site::Stub) => service(job, host, r, &mut out),
        None => user_cpp(job, host, r),
    };
    match result {
        Ok(text) => out.text.push_str(&text),
        Err(f) => {
            let last = f.text.lines().last().unwrap_or("");
            log(job, &f.msgid, &f.data, MsgType::Escape, Some(last));
            if out.text.is_empty() {
                out.text = shown(job, &f);
            }
            out.escape = Some(f);
        }
    }
    out
}

/// A system command in the storage service.
fn service(job: &mut Job, host: &Host, r: &Resolved, out: &mut Outcome) -> Result<String, Fail> {
    let built = block::build(&r.def, &r.parms).map_err(|e| Fail::new(e.msgid(), e.to_string()))?;
    let specified: Vec<bool> = r.parms.iter().map(|b| b.specified).collect();
    let reply: CmdReply = host
        .link
        .borrow_mut()
        .with(|l| l.run_cmd(r.name.clone(), built.segments, specified));
    for m in &reply.messages {
        log(job, &m.msgid, &m.data, kind_of(m.kind), None);
    }
    out.prompt = Some(reply.prompt);
    out.srcs = reply.srcs;
    out.sysvals = reply.sysvals;
    out.segments = Some(reply.segments);
    match reply.escape {
        Some(e) => {
            let mut text = reply.text;
            if text.is_empty() {
                text = shown(job, &Fail::new(&e.msgid, ""));
            }
            Err(Fail {
                msgid: e.msgid,
                data: e.data,
                text: text.trim_end().to_string(),
            })
        }
        None => Ok(reply.text),
    }
}

/// A user's command: its processing program called with the block.
fn user_cpp(job: &mut Job, _host: &Host, r: &Resolved) -> Result<String, Fail> {
    let built = block::build(&r.def, &r.parms).map_err(|e| Fail::new(e.msgid(), e.to_string()))?;
    let mut buffers: Vec<Option<Vec<u8>>> = built.segments;
    let addrs: Vec<u64> = buffers
        .iter_mut()
        .map(|b| b.as_mut().map_or(0, |v| v.as_mut_ptr() as u64))
        .collect();
    let cpp = &r.def.cpp;
    let before = job.console.len();
    let called = job.call_program(&cpp.name, &cpp.lib, &addrs);
    let lines: Vec<String> = job.console.drain(before..).collect();
    let mut text = lines.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    if let Err(e) = called {
        return Err(Fail::new(
            &e.msgid(),
            format!(
                "processing program {}/{} could not be called",
                cpp.lib, cpp.name
            ),
        ));
    }
    if let Some(e) = job.clear() {
        return Err(Fail {
            msgid: e.msgid.clone(),
            data: e.data.clone(),
            text,
        });
    }
    Ok(text)
}

/// Whether a line starts with a word of the old console tables.
fn is_old_word(text: &str) -> bool {
    let word = text.split_whitespace().next().unwrap_or("");
    jobsite::OLD_JOB_WORDS.contains(&word) || OLD_SERVICE_WORDS.contains(&word)
}

/// The words of the storage service's console table that have no
/// command of their own (the development commands).
const OLD_SERVICE_WORDS: &[&str] = &[
    "asp",
    "unit",
    "mirror",
    "lib",
    "obj",
    "dtaara",
    "dtaq",
    "usrspc",
    "rclstg",
    "chkstg",
    "txn",
    "xfer",
    "srcput",
    "srcget",
    "sql",
    "sqlrun",
    "naming",
    "curlib",
    "rla",
    "dbbench",
    "qir",
    "prtcapture",
    "whoami",
    "wmstats",
    "sysval",
    "fuzzptr",
    "callbench",
    "help",
    "release",
    "dsplog",
];

/// A `svc.` line: a job command of the old table, else the storage
/// service's console table.
fn passthrough(job: &mut Job, host: &Host, rest: &str) -> Outcome {
    let word = rest.split_whitespace().next().unwrap_or("");
    if jobsite::OLD_JOB_WORDS.contains(&word) {
        return Outcome {
            text: crate::console::jobcmd::command(job, host, word, rest),
            ..Outcome::default()
        };
    }
    let reply = host.exec(rest);
    Outcome {
        text: reply.text,
        prompt: Some(reply.prompt),
        srcs: reply.srcs,
        sysvals: reply.sysvals,
        ..Outcome::default()
    }
}

/// Analysis failed: the diagnostics and the escape, logged and shown.
fn failed(job: &mut Job, e: &CmdError) -> Outcome {
    let mut text = String::new();
    for d in &e.diags {
        log_diag(job, d, MsgType::Diag);
        text.push_str(&shown_diag(job, d));
    }
    log_diag(job, &e.escape, MsgType::Escape);
    text.push_str(&shown_diag(job, &e.escape));
    Outcome {
        text,
        escape: Some(Fail {
            msgid: e.escape.msgid.clone(),
            data: e.escape.data.clone(),
            text: e.escape.detail.clone(),
        }),
        ..Outcome::default()
    }
}

fn log_diag(job: &mut Job, d: &Diag, kind: MsgType) {
    let help = (!d.detail.is_empty()).then(|| d.detail.clone());
    job.log_external_with(&d.msgid, "QCPFMSG", &d.data, kind, None, help);
}

/// A diagnostic as the console shows it: the identifier and its text,
/// or our detail when there is one.
fn shown_diag(job: &mut Job, d: &Diag) -> String {
    if !d.detail.is_empty() {
        return format!("{} {}\n", d.msgid, d.detail);
    }
    let (text, _, _) = job.describe_message(&d.msgid, "QCPFMSG", "*LIBL", &d.data);
    format!("{} {}\n", d.msgid, text.trim_end())
}

/// A failure as the console shows it.
fn shown(job: &mut Job, f: &Fail) -> String {
    if !f.text.is_empty() {
        // Output before the escape stands as it is; the last line is
        // the escape's.
        let t = f.text.trim_end();
        let (head, last) = match t.rsplit_once('\n') {
            Some((h, l)) => (format!("{h}\n"), l),
            None => (String::new(), t),
        };
        if last.starts_with(&f.msgid) {
            return format!("{head}{last}\n");
        }
        return format!("{head}{} {last}\n", f.msgid);
    }
    let (text, _, _) = job.describe_message(&f.msgid, "QCPFMSG", "*LIBL", &f.data);
    format!("{} {}\n", f.msgid, text.trim_end())
}

/// A message to the job log, on the external queue.
pub(crate) fn log(job: &mut Job, msgid: &str, data: &[u8], kind: MsgType, text: Option<&str>) {
    let text = text.filter(|t| !t.is_empty()).map(|t| {
        let t = t.trim_end();
        t.strip_prefix(msgid).map_or(t, str::trim_start).to_string()
    });
    job.log_external_with(msgid, "QCPFMSG", data, kind, text, None);
}

/// The message type of a `CmdMsg` code.
fn kind_of(code: u8) -> MsgType {
    match code {
        2 => MsgType::Inq,
        3 => MsgType::Rpy,
        4 => MsgType::Comp,
        5 => MsgType::Diag,
        6 => MsgType::Notify,
        7 => MsgType::Escape,
        8 => MsgType::Rqs,
        9 => MsgType::Status,
        _ => MsgType::Info,
    }
}

/// The parameters of a resolved command, as a processing program in
/// this job reads them.
#[must_use]
pub fn parms_of(r: &Resolved) -> Parms {
    Parms::from_bound(&r.def, r.parms.clone())
}

/// Install the `QCMDEXC` hook: command strings a program runs go
/// through the analyzer with the same reach as a typed line.
pub fn install_hook(job: &mut Job, host: &Host) {
    let host = host.clone();
    job.cmdexc = Some(Rc::new(move |job: &mut Job, text: &str, mode: u8| {
        let o = run_exec(job, &host, text, RunMode::from_code(mode));
        for l in o.text.lines() {
            job.console.push(l.to_string());
        }
        match o.escape {
            Some(f) => Err((f.msgid, f.data)),
            None => Ok(o.changed),
        }
    }));
}
