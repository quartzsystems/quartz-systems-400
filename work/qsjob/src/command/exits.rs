// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The security and work management exit points the job calls around
//! a command (`docs/HOSTSVR.md` § Security and work management points):
//! the profile notifications before and after the profile commands,
//! the password check after a password is set (`VLDP0100` is
//! `work::pwdpgm`'s), and the preparation rounds before `ENDSYS`,
//! `ENDSBS *ALL` and `PWRDWNSYS`. Every program runs here, in the job
//! that ran the command; a system or subsystem job calls none.

use std::rc::Rc;

use cmd::{Resolved, Value};
use mi::Job;
use regfac::ExitRequest;
use regfac::formats::sec::Password;
use stg::remote::RemoteRegistry;

use super::{Fail, Outcome, service};
use crate::console::host::Host;

/// Whether this job calls exit programs: not a system job or a
/// subsystem monitor.
fn calls(host: &Host) -> bool {
    !matches!(host.start.role.as_str(), "SYSTEM" | "MONITOR")
}

fn registry(host: &Host) -> RemoteRegistry {
    RemoteRegistry::new(Rc::clone(&host.link))
}

/// A profile notification (`CRTP0100`, `CHGP0200` ...) for `user`.
pub fn profile(job: &mut Job, host: &Host, point: &str, format: &str, user: &str) {
    if !calls(host) || user.is_empty() || user.starts_with('*') {
        return;
    }
    let req = ExitRequest::Profile {
        point: point.into(),
        format: format.into(),
        user: user.into(),
    };
    let answer = regfac::call_exit(&mut registry(host), job, point, &req);
    failures(job, &answer);
}

/// A program the point ignores the failure of still leaves a
/// diagnostic in the job log (`CPF0A26`).
fn failures(job: &mut Job, answer: &regfac::ExitAnswer) {
    for c in answer.called.iter().filter(|c| c.failed.is_some()) {
        let data = cmd::error::data(&[(&c.pgm, 10), (&c.lib, 10)]);
        super::log(
            job,
            "CPF0A26",
            &data,
            qir::MsgType::Diag,
            c.failed.as_deref(),
        );
    }
}

fn level(host: &Host) -> i32 {
    i32::from(crate::work::pwdpgm::level(&host.link))
}

/// `QIBM_QSY_CHK_PASSWRD` (`CHKP0100`) after a password was set: a
/// program's answer is only audited (the registry's `GR` entry).
pub fn check_password(job: &mut Job, host: &Host, user: &str, old: Option<&str>, new: &str) {
    if !calls(host) || new.is_empty() || new.starts_with('*') {
        return;
    }
    let req = ExitRequest::Password(Password {
        format: "CHKP0100".into(),
        point: "QIBM_QSY_CHK_PASSWRD".into(),
        level: level(host),
        user: user.into(),
        old: old.map(str::to_string),
        new: new.into(),
    });
    let answer = regfac::call_exit(&mut registry(host), job, "QIBM_QSY_CHK_PASSWRD", &req);
    failures(job, &answer);
}

/// A parameter as typed or defaulted, upper case, quotes removed.
fn parm(r: &Resolved, kwd: &str) -> String {
    r.get(kwd)
        .and_then(|b| b.value.as_ref())
        .map(Value::source)
        .unwrap_or_default()
        .trim_matches('\'')
        .trim()
        .to_ascii_uppercase()
}

/// A parameter as typed (a password keeps its case).
fn parm_text(r: &Resolved, kwd: &str) -> String {
    r.get(kwd)
        .and_then(|b| b.value.as_ref())
        .map(Value::source)
        .unwrap_or_default()
        .trim()
        .trim_matches('\'')
        .to_string()
}

/// The delay of `ENDSYS`, `ENDSBS` or `PWRDWNSYS`: -1 `*IMMED`, -2
/// `*NOLIMIT`, else seconds.
fn delay(r: &Resolved) -> i32 {
    if parm(r, "OPTION") == "*IMMED" {
        return regfac::formats::wm::delay::IMMED;
    }
    match parm(r, "DELAY").as_str() {
        "*NOLIMIT" | "" => regfac::formats::wm::delay::NOLIMIT,
        n => n.parse().unwrap_or(regfac::formats::wm::delay::NOLIMIT),
    }
}

/// `PRER0100` or `PWRD0200`: `Err` with IBM's refusal when a program
/// said the state cannot be reached.
fn prepare(
    job: &mut Job,
    host: &Host,
    point: &str,
    format: &str,
    r: &Resolved,
) -> Result<(), Fail> {
    if !calls(host) {
        return Ok(());
    }
    // BLOCKED(rel): a program running past the wait time it asked for
    // is not ended (CPF18C8); the job waits for it.
    let done = regfac::prepare::prepare(&mut registry(host), job, point, format, delay(r));
    if done.allowed {
        return Ok(());
    }
    let who = done.refused_by.unwrap_or_default();
    Err(if format == "PWRD0200" {
        Fail::new("CPF18C7", "")
    } else {
        let pgm = who.rsplit('/').next().unwrap_or_default();
        Fail::msg("CPF18C3", &[(pgm, 10)])
    })
}

/// `CHGPWD`'s change of the job's own password, with the profile
/// notifications around it and the password check after.
pub fn chgpwd(
    job: &mut Job,
    host: &Host,
    user: &str,
    current: String,
    new: String,
) -> Result<(), String> {
    profile(job, host, "QIBM_QSY_CHG_PROFILE", "CHGP0200", user);
    let (old, set) = (current.clone(), new.clone());
    let changed = host
        .link
        .borrow_mut()
        .with(|l| stg::Client::chg_pwd(l, current, new, true));
    if changed.is_ok() {
        profile(job, host, "QIBM_QSY_CHG_PROFILE", "CHGP0100", user);
        check_password(job, host, user, Some(&old), &set);
    }
    changed
}

/// A command run in the service, with the exit points around it.
pub(crate) fn service_with_exits(
    job: &mut Job,
    host: &Host,
    r: &Resolved,
    out: &mut Outcome,
) -> Result<String, Fail> {
    match r.name.as_str() {
        "CRTUSRPRF" => {
            let user = parm(r, "USRPRF");
            let res = service(job, host, r, out);
            if res.is_ok() {
                profile(job, host, "QIBM_QSY_CRT_PROFILE", "CRTP0100", &user);
                check_password(job, host, &user, None, &parm_text(r, "PASSWORD"));
            }
            res
        }
        "CHGUSRPRF" | "CHGUSRAUD" | "CHGPRF" => {
            let users: Vec<String> = if r.name == "CHGPRF" {
                vec![host.link.borrow_mut().with(stg::Client::whoami).user]
            } else if r.name == "CHGUSRAUD" {
                match r.get("USRPRF").and_then(|b| b.value.as_ref()) {
                    Some(Value::List(items)) => items
                        .iter()
                        .map(|v| v.source().to_ascii_uppercase())
                        .collect(),
                    Some(v) => vec![v.source().to_ascii_uppercase()],
                    None => Vec::new(),
                }
            } else {
                vec![parm(r, "USRPRF")]
            };
            for u in &users {
                profile(job, host, "QIBM_QSY_CHG_PROFILE", "CHGP0200", u);
            }
            let res = service(job, host, r, out);
            if res.is_ok() {
                for u in &users {
                    profile(job, host, "QIBM_QSY_CHG_PROFILE", "CHGP0100", u);
                }
                if r.name == "CHGUSRPRF" {
                    let pwd = parm_text(r, "PASSWORD");
                    if !pwd.is_empty() && !pwd.starts_with('*') {
                        check_password(job, host, &users[0], None, &pwd);
                    }
                }
            }
            res
        }
        "DLTUSRPRF" => {
            let user = parm(r, "USRPRF");
            profile(job, host, "QIBM_QSY_DLT_PROFILE", "DLTP0200", &user);
            let res = service(job, host, r, out);
            if res.is_ok() {
                profile(job, host, "QIBM_QSY_DLT_PROFILE", "DLTP0100", &user);
            }
            res
        }
        "ENDSBS" if parm(r, "SBS") == "*ALL" => {
            prepare(job, host, "QIBM_QWC_PRERESTRICT", "PRER0100", r)?;
            service(job, host, r, out)
        }
        "ENDSYS" => {
            prepare(job, host, "QIBM_QWC_PRERESTRICT", "PRER0100", r)?;
            if calls(host) {
                regfac::prepare::bare(&mut registry(host), job, "QIBM_QWC_PWRDWNSYS", "PWRD0100");
            }
            service(job, host, r, out)
        }
        "PWRDWNSYS" => {
            prepare(job, host, "QIBM_QWC_PWRDWNSYS", "PWRD0200", r)?;
            if calls(host) {
                regfac::prepare::bare(&mut registry(host), job, "QIBM_QWC_PWRDWNSYS", "PWRD0100");
            }
            service(job, host, r, out)
        }
        _ => service(job, host, r, out),
    }
}

/// The FTP client and server's function usage (`QIBM_QTMF_CLIENT_REQ_n`,
/// `QIBM_QTMF_SERVER_REQ_n`) for a `VLRQ0100` request: the session's
/// function (`_0`) and the operation's, where IBM registers one, for
/// the request's user. `None` when the request is not one of these.
pub fn ftp_usage(link: &stg::Shared, req: &ExitRequest) -> Option<bool> {
    let ExitRequest::Validate(v) = req else {
        return None;
    };
    let side = match v.app {
        regfac::formats::tcp::app::FTP_SERVER => "SERVER",
        regfac::formats::tcp::app::FTP_CLIENT => "CLIENT",
        _ => return None,
    };
    let user = if v.user.trim().is_empty() || v.user.starts_with('*') {
        "*CURRENT".to_string()
    } else {
        v.user.trim().to_ascii_uppercase()
    };
    let mut ids = vec![format!("QIBM_QTMF_{side}_REQ_0")];
    if v.op != 0 {
        ids.push(format!("QIBM_QTMF_{side}_REQ_{}", v.op));
    }
    for id in ids.iter().filter(|id| sec::fcnusg::function(id).is_some()) {
        let q = sec::api::Query::FunctionUsage {
            user: user.clone(),
            function: id.clone(),
        };
        match link.borrow_mut().with(|l| stg::Client::sec_api(l, q)) {
            // A profile that cannot be read uses nothing.
            Ok(sec::api::Answer::Yes { yes: false }) | Err(_) => return Some(false),
            Ok(_) => {}
        }
    }
    Some(true)
}
