// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Environment variables: `ADDENVVAR`, `CHGENVVAR`, `RMVENVVAR` and
//! `WRKENVVAR` (the `QUIENVVAR` panel at a display, text otherwise).
//! The job's variables live in the job (`sysapi::envproc`); the
//! system's in the storage service, reached through [`LinkEnv`].

use std::fmt::Write as _;

use cmd::{Resolved, Value};
use mi::Job;
use stg::Shared;
use stg::qsh::{EnvVar, QshOp, QshReply, set_mode};
use sysapi::envproc::{EnvFail, SetMode, SysEnv, Var};

use stg::Client;

use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{CMDLINE_VAR, Response};

/// The system-level variables over the job's link.
pub struct LinkEnv {
    link: Shared,
}

impl LinkEnv {
    /// Over a link.
    pub fn new(link: Shared) -> Self {
        Self { link }
    }

    fn ask(&self, op: QshOp) -> QshReply {
        self.link.borrow_mut().with(|l| l.qsh(op))
    }
}

fn fail_of(r: QshReply) -> Result<(), EnvFail> {
    match r {
        QshReply::Done => Ok(()),
        QshReply::Failed { msgid, text } => Err(EnvFail::new(&msgid, text)),
        _ => Err(EnvFail::new("CPF9898", "The storage service answered out of turn.")),
    }
}

impl SysEnv for LinkEnv {
    fn list(&self) -> Result<Vec<Var>, EnvFail> {
        match self.ask(QshOp::SysEnv) {
            QshReply::Env { vars } => Ok(vars
                .into_iter()
                .map(|v| Var {
                    name: v.name,
                    value: v.value,
                    ccsid: v.ccsid,
                })
                .collect()),
            other => fail_of(other).map(|()| Vec::new()),
        }
    }

    fn set(&self, var: Var, mode: SetMode) -> Result<(), EnvFail> {
        let mode = match mode {
            SetMode::Add => set_mode::ADD,
            SetMode::Change => set_mode::CHANGE,
            SetMode::Put => set_mode::PUT,
        };
        fail_of(self.ask(QshOp::SysEnvSet {
            var: EnvVar {
                name: var.name,
                value: var.value,
                ccsid: var.ccsid,
            },
            mode,
        }))
    }

    fn remove(&self, name: &str) -> Result<(), EnvFail> {
        fail_of(self.ask(QshOp::SysEnvRemove { name: name.into() }))
    }
}

/// A parameter's value as given or defaulted, case kept, quotes off.
fn parm(r: &Resolved, kwd: &str) -> String {
    let v = r
        .get(kwd)
        .and_then(|b| b.value.as_ref())
        .map(Value::source)
        .unwrap_or_default();
    let t = v.trim();
    if t.len() >= 2 && t.starts_with('\'') && t.ends_with('\'') {
        t[1..t.len() - 1].replace("''", "'")
    } else {
        t.to_string()
    }
}

fn is_sys(r: &Resolved) -> bool {
    parm(r, "LEVEL").eq_ignore_ascii_case("*SYS")
}

fn level(sys: bool) -> &'static str {
    if sys { "*SYS" } else { "*JOB" }
}

fn fail(e: &EnvFail) -> Fail {
    Fail::new(&e.msgid, format!("{} {}", e.msgid, e.text))
}

fn system() -> Result<std::rc::Rc<dyn SysEnv>, Fail> {
    sysapi::envproc::system().ok_or_else(|| Fail::new("CPF9898", "System-level variables are not available."))
}

/// The variables of a level.
pub fn vars(sys: bool) -> Result<Vec<Var>, Fail> {
    if sys {
        system()?.list().map_err(|e| fail(&e))
    } else {
        Ok(sysapi::envproc::job_vars())
    }
}

fn set(sys: bool, var: Var, mode: SetMode) -> Result<(), Fail> {
    let r = if sys {
        system()?.set(var, mode)
    } else {
        sysapi::envproc::set_job(var, mode)
    };
    r.map_err(|e| fail(&e))
}

fn ccsid(r: &Resolved, kwd: &str) -> Option<i32> {
    match parm(r, kwd).to_ascii_uppercase().as_str() {
        "*SAME" | "" => None,
        "*JOB" => Some(0),
        "*HEX" => Some(65535),
        n => n.parse().ok(),
    }
}

/// `ADDENVVAR ENVVAR() VALUE() CCSID() LEVEL() REPLACE()`.
pub fn addenvvar(r: &Resolved) -> Result<String, Fail> {
    let name = parm(r, "ENVVAR");
    let value = parm(r, "VALUE");
    let value = (!value.eq_ignore_ascii_case("*NULL")).then_some(value);
    let replace = parm(r, "REPLACE").eq_ignore_ascii_case("*YES");
    let sys = is_sys(r);
    let var = Var {
        name: name.clone(),
        value,
        ccsid: ccsid(r, "CCSID").unwrap_or(0),
    };
    set(sys, var, if replace { SetMode::Put } else { SetMode::Add })?;
    Ok(format!("CPCA980 Added the environment variable {name}.\n"))
}

/// `CHGENVVAR ENVVAR() VALUE() CCSID() LEVEL()`.
pub fn chgenvvar(r: &Resolved) -> Result<String, Fail> {
    let name = parm(r, "ENVVAR");
    let sys = is_sys(r);
    let old = vars(sys)?
        .into_iter()
        .find(|v| v.name == name)
        .ok_or_else(|| {
            Fail::new(
                "CPFA981",
                format!("CPFA981 There is no environment variable {name} at level {}.", level(sys)),
            )
        })?;
    let value = parm(r, "VALUE");
    let value = match value.to_ascii_uppercase().as_str() {
        "*SAME" | "" => old.value.clone(),
        "*NULL" => None,
        _ => Some(value),
    };
    let var = Var {
        name: name.clone(),
        value,
        ccsid: ccsid(r, "CCSID").unwrap_or(old.ccsid),
    };
    set(sys, var, SetMode::Change)?;
    Ok(format!("CPCA981 Changed the environment variable {name}.\n"))
}

/// `RMVENVVAR ENVVAR() LEVEL()`.
pub fn rmvenvvar(r: &Resolved) -> Result<String, Fail> {
    let name = parm(r, "ENVVAR");
    let name = if name.eq_ignore_ascii_case("*ALL") { "*ALL".to_string() } else { name };
    let res = if is_sys(r) {
        system()?.remove(&name)
    } else {
        sysapi::envproc::remove_job(&name)
    };
    res.map_err(|e| fail(&e))?;
    Ok(format!("CPCA988 Removed the environment variable {name}.\n"))
}

/// A name cut to the list's column: 24 characters and ` >` when longer.
#[must_use]
pub fn name_cell(name: &str) -> String {
    if name.chars().count() > 24 {
        format!("{} >", name.chars().take(24).collect::<String>())
    } else {
        name.to_string()
    }
}

/// A value as the list shows it: quoted, cut to 34 characters with
/// ` >` after the closing quote when longer; `*NULL` for none.
#[must_use]
pub fn value_cell(value: Option<&str>) -> String {
    match value {
        None => "*NULL".to_string(),
        Some(v) if v.chars().count() > 34 => {
            format!("'{}' >", v.chars().take(34).collect::<String>())
        }
        Some(v) => format!("'{v}'"),
    }
}

/// `WRKENVVAR` as text: one variable a line.
fn text(sys: bool) -> Result<String, Fail> {
    let mut out = String::new();
    let _ = writeln!(out, "Environment variables ({})", level(sys));
    for v in vars(sys)? {
        let _ = writeln!(
            out,
            "  {:<26}  {}  {}",
            v.name,
            v.value.map_or_else(|| "*NULL".into(), |x| format!("'{x}'")),
            v.ccsid
        );
    }
    Ok(out)
}

/// `WRKENVVAR LEVEL()`: the panel at a display, the list as text
/// otherwise.
#[allow(clippy::many_single_char_names)] // the panels' names: r, d, l, c, v
pub fn wrkenvvar(job: &mut Job, host: &Host, r: &Resolved) -> Result<String, Fail> {
    let sys = is_sys(r);
    if !crate::panels::shows_panels(host, r) {
        return text(sys);
    }
    let mut d = crate::panels::dialog(host, "QUIENVVAR")?;
    let panel = if sys { "WRKENVSYS" } else { "WRKENVJOB" };
    let mut surface = Surface::new(host.clone());
    let mut ccsids = false;
    loop {
        let list = vars(sys)?;
        let top = d.list_mut("VARS").top;
        d.clear_list("VARS");
        let row = |n: String, v: String| {
            vec![
                ("OPT".to_string(), String::new()),
                ("EVNAME".to_string(), n),
                ("EVVALUE".to_string(), v),
            ]
        };
        // The blank entry option 1 adds from.
        d.add_row("VARS", row(String::new(), String::new()));
        for v in &list {
            let shown = if ccsids {
                v.ccsid.to_string()
            } else {
                value_cell(v.value.as_deref())
            };
            d.add_row("VARS", row(name_cell(&v.name), shown));
        }
        let l = d.list_mut("VARS");
        l.complete = true;
        l.top = top.min(l.rows.len().saturating_sub(1));
        let answer = d
            .show(&mut surface, panel)
            .map_err(|e| Fail::new("CPF6A12", e.to_string()))?;
        match answer {
            Response::Options(choices) => {
                let mut shown = None;
                for c in &choices {
                    let var = c.row.checked_sub(1).and_then(|i| list.get(i));
                    let q = |n: &str| n.replace('\'', "''");
                    let line = match (c.option.trim(), var) {
                        ("1", _) => Some(format!("?ADDENVVAR LEVEL({})", level(sys))),
                        ("2", Some(v)) => Some(format!("?CHGENVVAR ENVVAR('{}') LEVEL({})", q(&v.name), level(sys))),
                        ("4", Some(v)) => Some(format!("RMVENVVAR ENVVAR('{}') LEVEL({})", q(&v.name), level(sys))),
                        ("5", Some(v)) => {
                            shown = Some((
                                format!("{}={} (CCSID {})", v.name, v.value.as_deref().unwrap_or("*NULL"), v.ccsid),
                                false,
                            ));
                            None
                        }
                        ("6", Some(_)) => {
                            shown = Some(("Printing is not available in this release.".into(), true));
                            None
                        }
                        (o, _) => {
                            shown = Some((format!("Option {o} is not valid here."), true));
                            None
                        }
                    };
                    if let Some(line) = line {
                        shown = run_line(job, host, &line);
                    }
                }
                d.list_mut("VARS").options.clear();
                d.message = shown;
            }
            Response::Command(text) => {
                let text = d.substitute(&text, None);
                d.message = run_line(job, host, &text);
                d.set(CMDLINE_VAR, "");
            }
            Response::Return(11) => ccsids = !ccsids,
            Response::Return(17) => d.list_mut("VARS").top = 0,
            Response::Return(18) => {
                let l = d.list_mut("VARS");
                l.top = l.rows.len().saturating_sub(1);
            }
            Response::Return(16) | Response::Print => {
                d.message = Some(("Printing is not available in this release.".into(), true));
            }
            Response::Return(22) => {
                d.message = Some(("Type 5 by an entry to see all of it.".into(), false));
            }
            Response::Exit | Response::Cancel | Response::Gone | Response::Menu(_) | Response::Panel(_) => {
                return Ok(String::new());
            }
            Response::Return(_) | Response::Retrieve => {}
        }
    }
}

/// Run a command from the panel; the message to show after it.
fn run_line(job: &mut Job, host: &Host, text: &str) -> Option<(String, bool)> {
    let o = crate::command::run(job, host, text);
    if let Some(f) = &o.escape {
        if f.msgid == "CPF6801" {
            return None;
        }
        let t = f.text.lines().last().unwrap_or("").trim();
        return Some((if t.is_empty() { f.msgid.clone() } else { t.to_string() }, true));
    }
    o.text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(|l| (l.to_string(), false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_as_the_list_shows_them() {
        assert_eq!(name_cell("QIBM_PASE_USE_PRESTART_JOBS"), "QIBM_PASE_USE_PRESTART_J >");
        assert_eq!(name_cell("LANG"), "LANG");
        assert_eq!(
            value_cell(Some("/QopenSys/QIBM/ProdData/JavaVM/jdk17/64bit")),
            "'/QopenSys/QIBM/ProdData/JavaVM/jdk' >"
        );
        assert_eq!(value_cell(Some("Y")), "'Y'");
        assert_eq!(value_cell(None), "*NULL");
    }
}
