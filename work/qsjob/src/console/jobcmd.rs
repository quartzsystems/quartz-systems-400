// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The commands that act on the job itself: activation groups, the
//! call stack, message queues, replies, dumps, the debugger outside a
//! stop, `STRSBS`, `SBMJOB`, `DLYJOB`, `PWRDWNSYS`, `DSCJOB`, `CHGPWD`
//! and the job's displays. They take the keyword form the bridge makes
//! from the parameter block.

use std::fmt::Write as _;

use mi::{Job, QueueRef};
use wm::cmd::Params;

use super::host::Host;

/// Run `cmd`; its output. An error is a line starting with the message
/// identifier that ended the command.
pub fn command(job: &mut Job, host: &Host, cmd: &str, line: &str) -> String {
    let p = Params::parse(line);
    let r = match cmd {
        "dspactgrp" => Ok(job.actgrp_text()),
        "rclactgrp" => rclactgrp(job, &p),
        "dspcallstk" => Ok(job.call_stack_text()),
        "dspmsgq" => Ok(dspmsgq(job, &p)),
        "rpy" => rpy(job, &p),
        "dmp" => Ok(job.last_dump.clone().unwrap_or_else(|| job.dump(true))),
        "strdbg" => strdbg(job, &p),
        "chgdbg" => Ok(chgdbg(job, &p)),
        "enddbg" => {
            job.enddbg();
            Ok("Debug ended.\n".to_string())
        }
        "addbkp" => addbkp(job, &p, true),
        "rmvbkp" => addbkp(job, &p, false),
        "dspbkp" => Ok(dspbkp(job)),
        "dspjoblog" => Ok(super::log::dspjoblog(job, host)),
        "strsbs" => strsbs(host, &p),
        "dlyjob" => dlyjob(job, host, &p),
        "sbmjob" => sbmjob(host, &p),
        "pwrdwnsys" => pwrdwnsys(host, &p),
        "wrkjob" => wrkjob(job, host, &p),
        "chgpwd" => chgpwd(job, host, &p),
        "dscjob" => host
            .ctl
            .borrow_mut()
            .send(&wm::ctl::FromJob::Disconnect)
            .map(|()| "Job disconnected; sign on again to return to it.\n".to_string())
            .map_err(|e| e.to_string()),
        "endsys" => {
            let opt = p.upper("OPTION").unwrap_or_else(|| "*CNTRLD".into());
            let delay = p.upper("DELAY").unwrap_or_else(|| "*NOLIMIT".into());
            Ok(crate::command::run(
                job,
                host,
                &format!("ENDSBS SBS(*ALL) OPTION({opt}) DELAY({delay})"),
            )
            .text)
        }
        other => Err(format!("CPF0006 {other} is not a job command")),
    };
    match r {
        Ok(s) => s,
        Err(e) => format!("{e}\n"),
    }
}

/// A parameter after a call: its text when it is text, else hex.
#[must_use]
pub fn parm_text(b: &[u8]) -> String {
    let text = ccsid::from_ebcdic(b);
    if text.chars().all(|c| c.is_ascii_graphic() || c == ' ') {
        format!("'{}'", text.trim_end())
    } else {
        b.iter().fold(String::new(), |mut s, x| {
            let _ = write!(s, "{x:02X}");
            s
        })
    }
}

/// `CHGPWD CURPWD() NEWPWD()`: the job's own password. The validation
/// program runs here, in the job, before the service changes it.
fn chgpwd(job: &mut Job, host: &Host, p: &Params) -> Result<String, String> {
    let current = p.text("CURPWD");
    let new = p.text("NEWPWD");
    if new.is_empty() {
        return Err("CPD0071 NEWPWD is required".into());
    }
    let user = host.link.borrow_mut().with(stg::Client::whoami).user;
    if !crate::work::pwdpgm::validate(job, &host.link, &user, &new, &current)? {
        return Err(
            "CPF22C6 The password validation program did not accept the new password.".into(),
        );
    }
    host.link
        .borrow_mut()
        .with(|l| stg::Client::chg_pwd(l, current, new, true))?;
    Ok("Password changed successfully.\n".into())
}

/// `WRKJOB [JOB(n/u/name)] [OPTION(...)]`: the job's displays, by
/// option. Another job's call stack and log are its own business
/// (`MIGRATE(phase8)`: shown through the service).
fn wrkjob(job: &mut Job, host: &Host, p: &Params) -> Result<String, String> {
    let which = p.upper("JOB");
    let own = which.is_none() || which.as_deref() == Some("*");
    let jobkw = which
        .as_ref()
        .filter(|j| *j != "*")
        .map_or_else(String::new, |j| format!(" JOB({j})"));
    let option = p.upper("OPTION").unwrap_or_else(|| "*SELECT".into());
    let s = &host.start;
    match option.as_str() {
        "*STSA" | "*DFNA" | "*RUNA" => {
            Ok(crate::command::run(job, host, &format!("DSPJOB{jobkw}")).text)
        }
        "*SPLF" => {
            let spec = if own {
                format!("{:06}/{}/{}", s.number, s.user, s.name)
            } else {
                which.clone().unwrap_or_default()
            };
            Ok(crate::command::run(job, host, &format!("WRKSPLF SELECT(*ALL) JOB({spec})")).text)
        }
        "*JOBLOG" if own => Ok(super::log::dspjoblog(job, host)),
        "*PGMSTK" if own => Ok(job.call_stack_text()),
        "*LIBL" if own => Ok(crate::command::run(job, host, "DSPLIBL").text),
        "*JOBLOG" | "*PGMSTK" | "*LIBL" => {
            Err(format!("CPF1343 {option} of another job is not shown yet"))
        }
        "*SELECT" => Ok(format!(
            "Work with Job: {:06}/{}/{}\n\n  1. Display job status attributes        (OPTION(*STSA))\n  2. Display job definition attributes    (OPTION(*DFNA))\n  3. Display job run attributes           (OPTION(*RUNA))\n  4. Work with spooled files              (OPTION(*SPLF))\n 10. Display job log                      (OPTION(*JOBLOG))\n 11. Display call stack                   (OPTION(*PGMSTK))\n 13. Display library list                 (OPTION(*LIBL))\n",
            s.number, s.user, s.name
        )),
        o => Err(format!("CPD0084 OPTION({o}) is not valid")),
    }
}

/// `RCLACTGRP ACTGRP(NAME|*ELIGIBLE)`.
fn rclactgrp(job: &mut Job, p: &Params) -> Result<String, String> {
    let name = p.pos_or(0, "ACTGRP")?.to_ascii_uppercase();
    if name == "*ELIGIBLE" {
        job.rclactgrp_eligible();
        return Ok("Eligible activation groups reclaimed.\n".to_string());
    }
    Ok(match job.rclactgrp(&name) {
        Ok(()) => format!("Activation group {name} reclaimed.\n"),
        Err(e) => format!("{e}\n"),
    })
}

/// `DSPMSGQ [*PGM|*EXT|QSYSOPR]`: the job's own queues (`svc.`-era
/// display).
fn dspmsgq(job: &mut Job, p: &Params) -> String {
    let which = p
        .pos
        .first()
        .cloned()
        .or_else(|| p.upper("MSGQ"))
        .map_or_else(|| "*EXT".to_string(), |s| s.to_ascii_uppercase());
    let q = match which.as_str() {
        "*PGM" | "*SAME" => QueueRef::Same,
        "QSYSOPR" | "*SYSOPR" => QueueRef::SysOpr,
        _ => QueueRef::Ext,
    };
    job.msgq_text(q)
}

/// `rpy KEY REPLY`: answer an inquiry on the external queue, or queue
/// the reply for the next one.
fn rpy(job: &mut Job, p: &Params) -> Result<String, String> {
    let (key, reply) = match p.pos.as_slice() {
        [k, r] => (k.clone(), r.to_ascii_uppercase()),
        [r] => (String::new(), r.to_ascii_uppercase()),
        _ => (
            p.upper("MSGKEY").unwrap_or_default(),
            p.upper("RPY").unwrap_or_default(),
        ),
    };
    if reply.is_empty() {
        return Err("CPD0015 RPY is required".into());
    }
    let k = u32::from_str_radix(&key, 16).ok();
    if let Some(k) = k
        && let Some(m) = job.ext_msgq.msgs.iter_mut().find(|m| m.key == k)
    {
        m.reply = Some(reply.clone());
        return Ok(format!("Reply {reply} recorded for message {key}.\n"));
    }
    job.inquiry_replies.push_back(reply.clone());
    Ok(format!("Reply {reply} queued for the next inquiry.\n"))
}

/// `STRDBG PGM(LIB/PGM)`.
fn strdbg(job: &mut Job, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "PGM")?;
    let qual = qual.split_whitespace().next().unwrap_or("").to_string();
    let (lib, name) = qual
        .split_once('/')
        .map_or(("*LIBL", qual.as_str()), |(l, n)| (l, n));
    let (lib, name) = (lib.to_ascii_uppercase(), name.to_ascii_uppercase());
    let updprod = p.upper("UPDPROD").is_some_and(|v| v == "*YES");
    Ok(match job.strdbg(&name, &lib) {
        Ok(key) => {
            if let Some(d) = job.debug.as_mut() {
                d.updprod = updprod;
            }
            format!("{key} is under debug; its next call runs the debug view.\n")
        }
        Err(e) => format!("{e}\n"),
    })
}

/// `CHGDBG UPDPROD(*YES|*NO)`: whether production files may change
/// under debug.
fn chgdbg(job: &mut Job, p: &Params) -> String {
    let Some(d) = job.debug.as_mut() else {
        return "CPF1999 the job is not in debug mode; STRDBG first.\n".into();
    };
    if let Some(v) = p.upper("UPDPROD") {
        d.updprod = v == "*YES";
    }
    format!(
        "Debug attributes changed: UPDPROD({}).\n",
        if d.updprod { "*YES" } else { "*NO" }
    )
}

/// `ADDBKP STMT(n) [PGM(LIB/PGM)]` and `RMVBKP STMT(n|*ALL) [PGM(..)]`.
fn addbkp(job: &mut Job, p: &Params, add: bool) -> Result<String, String> {
    let n = p.pos_or(0, "STMT")?;
    let pgm = p
        .upper("PGM")
        .map(|q| q.split('/').next_back().unwrap_or("").to_string());
    let program = pgm.unwrap_or_else(|| {
        job.debug
            .as_ref()
            .and_then(|d| d.programs.last().cloned())
            .unwrap_or_default()
    });
    if program.is_empty() {
        return Ok("No program is under debug; STRDBG first or give PGM().\n".into());
    }
    if add {
        return Ok(match n.parse::<u32>() {
            Ok(stmt) => {
                job.addbkp(&program, stmt);
                format!("Breakpoint at {program} statement {stmt}.\n")
            }
            Err(_) => "CPD0084 STMT is a statement number\n".into(),
        });
    }
    let gone = match n.parse::<u32>() {
        Ok(stmt) => job.rmvbkp(&program, Some(stmt)),
        Err(_) => job.rmvbkp(&program, None),
    };
    Ok(format!("{gone} breakpoint(s) removed.\n"))
}

/// `DSPBKP`.
fn dspbkp(job: &mut Job) -> String {
    let mut out = String::new();
    if let Some(d) = &job.debug {
        for (pgm, n) in &d.breakpoints {
            let _ = writeln!(out, "  {pgm:<21} statement {n}");
        }
        for pgm in &d.programs {
            let _ = writeln!(out, "  {pgm} under debug");
        }
    }
    if out.is_empty() {
        "No breakpoints.\n".into()
    } else {
        out
    }
}

/// `STRSBS SBSD(LIB/NAME)`: start the subsystem's monitor and wait for
/// it to register.
fn strsbs(host: &Host, p: &Params) -> Result<String, String> {
    use stg::Client;
    let (_, name) = wm::obj::params::qualified(&p.pos_or(0, "SBSD")?);
    let qual = p.pos_or(0, "SBSD")?.to_ascii_uppercase();
    let (lib, _) = host.link.borrow_mut().with(|l| l.sbs_check(qual.clone()))?;
    let number = host.link.borrow_mut().with(Client::job_number)?;
    let start = wm::ctl::Start {
        name: name.clone(),
        user: "QSYS".into(),
        number,
        handle: u64::from(number),
        kind: 'M',
        subsystem: name.clone(),
        role: "MONITOR".into(),
        jobd: "QSYS/QSYSJOBD".into(),
        class: "QSYS/QSYSCLS20".into(),
        rqsdta: format!("{lib}/{name}"),
        pool: "*BASE".into(),
        ..wm::ctl::Start::default()
    };
    let mut s = crate::work::spawn::job(start)?;
    crate::work::spawn::ready(&mut s)?;
    host.children.borrow_mut().push(s);
    // The monitor registers the subsystem; wait for it, a few seconds.
    for _ in 0..50 {
        let active = host.link.borrow_mut().with(Client::sbs_list);
        if active.iter().any(|a| a.name == name) {
            return Ok(format!("Subsystem {name} started.\n"));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Err(format!("CPF1012 subsystem {name} did not start"))
}

/// `PWRDWNSYS OPTION(*CNTRLD|*IMMED) DELAY(n|*NOLIMIT) RESTART(*NO|*YES)
/// IPLSRC(*PANEL) CONFIRM(...)`: `slic` ends the subsystems, the system
/// jobs and the storage service, then powers down.
fn pwrdwnsys(host: &Host, p: &Params) -> Result<String, String> {
    let me = host.link.borrow_mut().with(stg::Client::whoami);
    me.require(sec::profile::SpecialAuth::JOBCTL, "power down the system")
        .map_err(|e| e.to_string())?;
    let immediate = match p.upper("OPTION").as_deref() {
        None | Some("*CNTRLD") => false,
        Some("*IMMED") => true,
        Some(o) => return Err(format!("CPD0084 OPTION({o}) is not valid")),
    };
    let delay = match p.upper("DELAY").as_deref() {
        None | Some("*NOLIMIT") => 3600,
        Some(d) => d
            .parse()
            .map_err(|_| format!("CPD0084 DELAY({d}) is not valid"))?,
    };
    let restart = match p
        .upper("RESTART")
        .as_deref()
        .map(|r| r.split_whitespace().next().unwrap_or(""))
    {
        None | Some("*NO") => false,
        Some("*YES") => true,
        Some(r) => return Err(format!("CPD0084 RESTART({r}) is not valid")),
    };
    match p.upper("IPLSRC").as_deref() {
        None | Some("*PANEL") => {}
        Some(s) => {
            return Err(format!(
                "CPD0084 IPLSRC({s}): the IPL source is chosen at the panel (*PANEL)"
            ));
        }
    }
    host.ctl
        .borrow_mut()
        .send(&wm::ctl::FromJob::PowerDown {
            immediate,
            delay,
            restart,
        })
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "Power down {}{}.\n",
        if immediate {
            "immediately"
        } else {
            "in a controlled manner"
        },
        if restart { ", then IPL" } else { "" }
    ))
}

/// `DLYJOB DLY(seconds)`: the job waits, in `DLYW`, taking break
/// messages as they come.
fn dlyjob(job: &mut Job, host: &Host, p: &Params) -> Result<String, String> {
    let secs: u64 = p
        .pos_or(0, "DLY")?
        .trim()
        .parse()
        .map_err(|_| "CPD0084 DLY is a number of seconds".to_string())?;
    let until = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    stg::remote::status(&host.link, wm::job::Status::Dlyw, &format!("DLY-{secs}"));
    while std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(250));
        host.breaks(job);
    }
    stg::remote::status(&host.link, wm::job::Status::Run, "");
    Ok(String::new())
}

/// Keywords of `SBMJOB` the job's attributes take as `CHGJOB` would.
const SBM_CHANGES: &[&str] = &[
    "LOG",
    "LOGCLPGM",
    "LOGOUTPUT",
    "INQMSGRPY",
    "OUTPTY",
    "PRTDEV",
    "OUTQ",
    "PRTTXT",
    "DATE",
    "SWS",
    "SRTSEQ",
    "LANGID",
    "CNTRYID",
    "CCSID",
    "DUPJOBOPT",
];

/// `SCDDATE` (`CYYMMDD` from the analyzer, `YYYYMMDD` or `MMDDYY`,
/// `*CURRENT`) and `SCDTIME` (`HHMMSS`, `*CURRENT`) as Unix seconds; 0
/// for now.
fn scheduled(date: Option<&str>, time: Option<&str>) -> Result<u64, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let date = date
        .map(str::trim)
        .filter(|d| !d.eq_ignore_ascii_case("*CURRENT"));
    let time = time
        .map(str::trim)
        .filter(|t| !t.eq_ignore_ascii_case("*CURRENT"));
    if date.is_none() && time.is_none() {
        return Ok(0);
    }
    let day = match date {
        None => now / 86_400 * 86_400,
        Some(d) => {
            let (y, m, dd) = match d.len() {
                8 => (
                    d[0..4].parse::<i64>(),
                    d[4..6].parse::<u32>(),
                    d[6..8].parse::<u32>(),
                ),
                7 => (
                    d[1..3]
                        .parse::<i64>()
                        .map(|y| if &d[..1] == "1" { 2000 + y } else { 1900 + y }),
                    d[3..5].parse::<u32>(),
                    d[5..7].parse::<u32>(),
                ),
                6 => (
                    d[4..6].parse::<i64>().map(|y| 2000 + y),
                    d[0..2].parse::<u32>(),
                    d[2..4].parse::<u32>(),
                ),
                _ => return Err(format!("CPD0084 SCDDATE({d})")),
            };
            let (Ok(y), Ok(m), Ok(dd)) = (y, m, dd) else {
                return Err(format!("CPD0084 SCDDATE({d})"));
            };
            let unix = sysapi::Civil {
                year: y,
                month: m,
                day: dd,
                hour: 0,
                min: 0,
                sec: 0,
                ms: 0,
            }
            .unix();
            u64::try_from(unix).map_err(|_| format!("CPD0084 SCDDATE({d})"))?
        }
    };
    let secs = match time {
        None => now % 86_400,
        Some(t) if t.len() == 6 && t.bytes().all(|b| b.is_ascii_digit()) => {
            let n = |a: usize| {
                u64::from(t.as_bytes()[a] - b'0') * 10 + u64::from(t.as_bytes()[a + 1] - b'0')
            };
            n(0) * 3600 + n(2) * 60 + n(4)
        }
        Some(t) => return Err(format!("CPD0084 SCDTIME({t})")),
    };
    Ok(day + secs)
}

/// `SBMJOB`: the request goes to the storage service with the job's
/// `*LDA`; the analyzer checked `CMD`.
fn sbmjob(host: &Host, p: &Params) -> Result<String, String> {
    use stg::Client;
    let cmd = p
        .kw("CMD")
        .or_else(|| p.kw("RQSDTA"))
        .map(|v| v.trim().trim_matches('\'').replace("''", "'"))
        .ok_or("CPD0015 CMD or RQSDTA is needed")?;
    let up = |k: &str| p.upper(k).unwrap_or_default();
    let optional = |k: &str| {
        p.upper(k)
            .filter(|v| v != "*JOBD" && v != "*USRPRF" && v != "*CURRENT")
            .unwrap_or_default()
    };
    let mut req = wm::obj::jobq::SbmJob {
        cmd,
        job: optional("JOB"),
        jobd: optional("JOBD"),
        jobq: optional("JOBQ"),
        jobpty: match p.upper("JOBPTY").as_deref() {
            None | Some("*JOBD" | "0") => None,
            Some(v) => Some(v.parse().map_err(|_| format!("CPD0084 JOBPTY({v})"))?),
        },
        user: optional("USER"),
        rtgdta: p
            .kw("RTGDTA")
            .map(|v| v.trim().trim_matches('\'').to_string())
            .unwrap_or_default(),
        hold: match p.upper("HOLD").as_deref() {
            None | Some("*JOBD") => None,
            Some("*YES") => Some(true),
            Some("*NO") => Some(false),
            Some(v) => return Err(format!("CPD0084 HOLD({v})")),
        },
        scheduled: scheduled(p.kw("SCDDATE"), p.kw("SCDTIME"))?,
        msgq: optional("MSGQ"),
        inllibl: match p.upper("INLLIBL").as_deref() {
            None | Some("*CURRENT") => None,
            Some("*JOBD") => Some(Vec::new()),
            Some(v) => Some(v.split_whitespace().map(str::to_string).collect()),
        },
        curlib: optional("CURLIB"),
        changes: Vec::new(),
        lda: host.lda.borrow().clone(),
    };
    for (k, v) in &p.kw {
        if SBM_CHANGES.contains(&k.as_str())
            && !v.trim().eq_ignore_ascii_case("*JOBD")
            && !v.trim().eq_ignore_ascii_case("*CURRENT")
        {
            req.changes.push((k.clone(), v.trim().to_string()));
        }
    }
    let _ = up;
    let (_, msg) = host.link.borrow_mut().with(|l| l.sbmjob(req))?;
    Ok(format!("{msg}\n"))
}
