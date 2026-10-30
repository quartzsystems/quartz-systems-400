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

pub mod ddm;
pub mod hostsvr;
pub mod jobsite;
pub mod natives;
pub mod nettools;
pub mod prompt;
pub mod tcp;
pub mod tcpsvr;
pub mod uiapi;

use std::fmt::Write as _;
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
    let libs = DefLibs::enter(host, r);
    let result = match entry.map(|e| e.site) {
        Some(_) if lda::is_lda(r) => lda::command(host, r),
        Some(Site::Job) => jobsite::run(job, host, r, &mut out),
        Some(Site::Service | Site::Stub) => service(job, host, r, &mut out),
        None => user_cpp(job, host, r),
    };
    libs.leave(host);
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

/// A definition's `PRDLIB` and `CURLIB`: in the job's library list
/// while the command runs, the list as it was afterwards.
struct DefLibs {
    prd: Option<Vec<String>>,
    /// Whether `CURLIB` was changed, and what it was.
    cur_changed: bool,
    cur: Option<String>,
}

impl DefLibs {
    fn enter(host: &Host, r: &Resolved) -> Self {
        let mut saved = Self {
            prd: None,
            cur_changed: false,
            cur: None,
        };
        if r.def.prdlib.is_none() && r.def.curlib.is_none() {
            return saved;
        }
        let before = host.link.borrow_mut().with(stg::Client::rtvjoba).libl;
        if let Some(p) = &r.def.prdlib {
            let reply = host.exec(&format!("prdlib LIB({p})"));
            if reply.text.trim().is_empty() {
                saved.prd = Some(before.prd.clone());
            }
        }
        if let Some(c) = &r.def.curlib {
            let reply = host.exec(&format!("chgcurlib CURLIB({c})"));
            if reply.text.trim().is_empty() {
                saved.cur_changed = true;
                saved.cur.clone_from(&before.cur);
            }
        }
        if saved.prd.is_some() || saved.cur_changed {
            host.attrs_changed();
        }
        saved
    }

    fn leave(self, host: &Host) {
        let changed = self.prd.is_some() || self.cur_changed;
        if let Some(prd) = self.prd {
            let lib = prd.first().map_or("*NONE", String::as_str);
            let _ = host.exec(&format!("prdlib LIB({lib})"));
        }
        if self.cur_changed {
            let lib = self.cur.as_deref().unwrap_or("*CRTDFT");
            let _ = host.exec(&format!("chgcurlib CURLIB({lib})"));
        }
        if changed {
            host.attrs_changed();
        }
    }
}

/// The job's local data area at the command line: `DSPDTAARA` and
/// `CHGDTAARA` on `*LDA` never leave the job; on `*GDA`, the group's,
/// they reach the storage service's copy.
mod lda {
    use std::fmt::Write as _;

    use cmd::{Resolved, Value};
    use stg::Client;
    use stg::remote::LDA_LEN;

    use super::Fail;
    use crate::console::host::Host;

    /// Whether the command's data area is `*LDA` or `*GDA`.
    pub fn is_lda(r: &Resolved) -> bool {
        let name = area(r).0;
        matches!(r.name.as_str(), "DSPDTAARA" | "CHGDTAARA")
            && (name.eq_ignore_ascii_case("*LDA") || name.eq_ignore_ascii_case("*GDA"))
    }

    /// The group data area's length.
    const GDA_LEN: usize = 512;

    /// `CHGDTAARA DTAARA(*GDA ...)` and `DSPDTAARA DTAARA(*GDA)`.
    fn gda(host: &Host, r: &Resolved, sub: Option<(usize, usize)>) -> Result<String, Fail> {
        let fail = |e: String| {
            let (id, text) = e.split_once(' ').unwrap_or(("CPF1311", e.as_str()));
            Fail::new(id, text.to_string())
        };
        let (start, len) = sub.unwrap_or((1, GDA_LEN));
        if start == 0 || start + len - 1 > GDA_LEN {
            return Err(Fail::new("CPF1015", "*GDA: the substring lies outside the area"));
        }
        if r.name == "CHGDTAARA" {
            let value = r
                .get("VALUE")
                .and_then(|b| b.value.as_ref())
                .map(Value::source)
                .unwrap_or_default();
            let mut b = ccsid::to_ebcdic(&value).unwrap_or_default();
            b.resize(len, 0x40);
            let at = u32::try_from(start).unwrap_or(1);
            return host
                .link
                .borrow_mut()
                .with(|l| l.gda_write(at, b))
                .map(|()| String::new())
                .map_err(fail);
        }
        let all = host.link.borrow_mut().with(Client::gda_read).map_err(fail)?;
        let text = ccsid::from_ebcdic(&all);
        let part: String = text.chars().skip(start - 1).take(len).collect();
        let mut out = String::new();
        out.push_str("Data area . . . . . . . :   *GDA\n");
        out.push_str("  Type  . . . . . . . . :   *CHAR\n");
        let _ = writeln!(out, "  Length  . . . . . . . :   {GDA_LEN}");
        out.push_str("  Text  . . . . . . . . :   Group data area\n");
        out.push_str("           Value\n");
        out.push_str("Offset      *...+....1....+....2....+....3....+....4....+....5\n");
        let shown = part.trim_end();
        let rows = shown.chars().count().div_ceil(50).max(1);
        for i in 0..rows {
            let chunk: String = shown.chars().skip(i * 50).take(50).collect();
            let _ = writeln!(out, "{:>6}      '{chunk}'", start - 1 + i * 50);
        }
        Ok(out)
    }

    /// The area's name and the substring asked for.
    fn area(r: &Resolved) -> (String, Option<(usize, usize)>) {
        let Some(v) = r.get("DTAARA").and_then(|b| b.value.as_ref()) else {
            return (String::new(), None);
        };
        let name = |val: &Value| match val {
            Value::Qual(parts) => parts.first().map(Value::source).unwrap_or_default(),
            other => other.source(),
        };
        match v {
            Value::Elems(items) => {
                let nm = items
                    .first()
                    .cloned()
                    .flatten()
                    .map(|val| name(&val))
                    .unwrap_or_default();
                let sub = match items.get(1).cloned().flatten() {
                    Some(Value::Elems(se)) => {
                        let num = |ix: usize| {
                            se.get(ix)
                                .cloned()
                                .flatten()
                                .and_then(|val| val.source().parse::<usize>().ok())
                        };
                        match (num(0), num(1)) {
                            (Some(st), Some(ln)) => Some((st, ln)),
                            _ => None,
                        }
                    }
                    _ => None,
                };
                (nm, sub)
            }
            other => (name(other), None),
        }
    }

    /// The whole area as text.
    pub fn text(host: &Host) -> String {
        ccsid::from_ebcdic(&host.lda.borrow())
    }

    /// Put `value` at `start` (1-based), padded with blanks to the end
    /// of the substring asked for, or the area.
    pub fn set(host: &Host, start: usize, value: &str) {
        set_sub(host, start, None, value);
    }

    fn set_sub(host: &Host, start: usize, len: Option<usize>, value: &str) {
        let mut b = ccsid::to_ebcdic(value).unwrap_or_default();
        let room = LDA_LEN.saturating_sub(start.saturating_sub(1));
        let n = len.unwrap_or(room).min(room);
        b.resize(n, 0x40);
        let mut lda = host.lda.borrow_mut();
        lda.resize(LDA_LEN, 0x40);
        lda[start - 1..start - 1 + n].copy_from_slice(&b);
    }

    /// Run the command.
    pub fn command(host: &Host, r: &Resolved) -> Result<String, Fail> {
        let (name, sub) = area(r);
        if name.eq_ignore_ascii_case("*GDA") {
            return gda(host, r, sub);
        }
        let (start, len) = sub.unwrap_or((1, LDA_LEN));
        if start == 0 || start + len - 1 > LDA_LEN {
            return Err(Fail::new(
                "CPF1015",
                "*LDA: the substring lies outside the area",
            ));
        }
        if r.name == "CHGDTAARA" {
            let value = r
                .get("VALUE")
                .and_then(|b| b.value.as_ref())
                .map(Value::source)
                .unwrap_or_default();
            set_sub(host, start, Some(len), &value);
            return Ok(String::new());
        }
        let all = text(host);
        let part: String = all.chars().skip(start - 1).take(len).collect();
        let mut out = String::new();
        out.push_str("Data area . . . . . . . :   *LDA\n");
        out.push_str("  Type  . . . . . . . . :   *CHAR\n");
        let _ = writeln!(out, "  Length  . . . . . . . :   {LDA_LEN}");
        out.push_str("  Text  . . . . . . . . :   Local data area\n");
        out.push_str("           Value\n");
        out.push_str("Offset      *...+....1....+....2....+....3....+....4....+....5\n");
        let shown = part.trim_end();
        let rows = shown.chars().count().div_ceil(50).max(1);
        for i in 0..rows {
            let chunk: String = shown.chars().skip(i * 50).take(50).collect();
            let _ = writeln!(out, "{:>6}      '{chunk}'", start - 1 + i * 50);
        }
        Ok(out)
    }
}

/// A system command in the storage service.
pub(crate) fn service(job: &mut Job, host: &Host, r: &Resolved, out: &mut Outcome) -> Result<String, Fail> {
    let built = block::build(&r.def, &r.parms).map_err(|e| Fail::new(e.msgid(), e.to_string()))?;
    let specified: Vec<bool> = r.parms.iter().map(|b| b.specified).collect();
    // The keyword form as typed, for the service's bridge: the block
    // alone cannot tell a given element from its default.
    let mut typed = sys_cpps::entry(&r.name)
        .map(|e| sys_cpps::bridge::specified_line(&e.word, &parms_of(r)))
        .unwrap_or_default();
    // An override belongs to the call level that issued it.
    if matches!(r.name.as_str(), "OVRDBF" | "OVRPRTF" | "OVRMSGF" | "DLTOVR") {
        let _ = write!(typed, " CALLLVL({})", job.depth);
        if r.name != "DLTOVR" {
            job.ovr_top = job.ovr_top.max(job.depth);
        }
    }
    let reply: CmdReply = host
        .link
        .borrow_mut()
        .with(|l| l.run_cmd(r.name.clone(), built.segments, specified, typed));
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
        None if reply.text.is_empty() => {
            // Nothing printed: the completion messages say what was done.
            let mut text = String::new();
            for m in reply.messages.iter().filter(|m| m.kind == 4) {
                let (t, _, _) = job.describe_message(&m.msgid, sysapi::param::file_of(&m.msgid), "*LIBL", &m.data);
                let _ = writeln!(text, "{}", t.trim_end());
            }
            Ok(text)
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
    // `VLDCKR`: the validity checking program sees the block first and
    // refuses with an escape.
    if let Some(v) = &r.def.vldckr {
        if let Err(e) = job.call_program(&v.name, &v.lib, &addrs) {
            return Err(Fail::new(
                &e.msgid(),
                format!("validity checking program {}/{} could not be called", v.lib, v.name),
            ));
        }
        if let Some(e) = job.clear() {
            return Err(Fail {
                msgid: e.msgid.clone(),
                data: e.data.clone(),
                text: String::new(),
            });
        }
    }
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

/// A `svc.` line: a job command of the old table, else the storage
/// service's console table.
fn passthrough(job: &mut Job, host: &Host, rest: &str) -> Outcome {
    let word = rest.split_whitespace().next().unwrap_or("");
    let args: Vec<&str> = rest.split_whitespace().collect();
    if word == "dtaara" && args.get(2).is_some_and(|a| a.eq_ignore_ascii_case("*LDA")) {
        // The job's own area: `dtaara get *LDA`, `dtaara set *LDA TEXT`.
        let text = match args.get(1).copied() {
            Some("get") => format!("*LDA: '{}'\n", lda::text(host).trim_end()),
            Some("set") => {
                lda::set(host, 1, &args[3..].join(" "));
                "*LDA changed.\n".into()
            }
            _ => "Usage: dtaara get *LDA | set *LDA TEXT\n".into(),
        };
        return Outcome {
            text,
            ..Outcome::default()
        };
    }
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
    job.log_external_with(&d.msgid, sysapi::param::file_of(&d.msgid), &d.data, kind, None, help);
}

/// A diagnostic as the console shows it: the identifier and its text,
/// or our detail when there is one.
fn shown_diag(job: &mut Job, d: &Diag) -> String {
    if !d.detail.is_empty() {
        return format!("{} {}\n", d.msgid, d.detail);
    }
    let (text, _, _) = job.describe_message(&d.msgid, sysapi::param::file_of(&d.msgid), "*LIBL", &d.data);
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
    let (text, _, _) = job.describe_message(&f.msgid, sysapi::param::file_of(&f.msgid), "*LIBL", &f.data);
    format!("{} {}\n", f.msgid, text.trim_end())
}

/// A message to the job log, on the external queue.
pub(crate) fn log(job: &mut Job, msgid: &str, data: &[u8], kind: MsgType, text: Option<&str>) {
    let text = text.filter(|t| !t.is_empty()).map(|t| {
        let t = t.trim_end();
        t.strip_prefix(msgid).map_or(t, str::trim_start).to_string()
    });
    job.log_external_with(msgid, sysapi::param::file_of(msgid), data, kind, text, None);
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
/// through the analyzer with the same reach as a typed line. A call
/// level's file overrides go when it returns.
pub fn install_hook(job: &mut Job, host: &Host) {
    let ovr_host = host.clone();
    job.ovr_end = Some(Rc::new(move |job: &mut Job| {
        let _ = ovr_host.exec(&format!("dltovr ENDLVL({})", job.depth + 1));
        job.ovr_top = job.depth;
    }));
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
