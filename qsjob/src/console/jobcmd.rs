// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The console commands that act on the job itself: calls, activation
//! groups, the call stack, message queues, replies, dumps and the
//! debugger outside a stop.

use std::fmt::Write as _;

use mi::QueueRef;
use wm::cmd::Params;

use super::Console;

/// Run `cmd`; its output.
pub fn command(c: &mut Console, cmd: &str, line: &str) -> String {
    let p = Params::parse(line);
    let r = match cmd {
        "call" => super::call::call(c, line),
        "dspactgrp" => Ok(c.job().actgrp_text()),
        "rclactgrp" => rclactgrp(c, &p),
        "dspcallstk" => Ok(c.job().call_stack_text()),
        "dspmsgq" => Ok(dspmsgq(c, &p)),
        "rpy" => rpy(c, &p),
        "dmp" => {
            let j = c.job();
            Ok(j.last_dump.clone().unwrap_or_else(|| j.dump(true)))
        }
        "strdbg" => strdbg(c, &p),
        "enddbg" => {
            c.job().enddbg();
            Ok("Debug ended.\n".to_string())
        }
        "addbkp" => addbkp(c, &p, true),
        "rmvbkp" => addbkp(c, &p, false),
        "dspbkp" => Ok(dspbkp(c)),
        "dspjoblog" => Ok(c.dspjoblog()),
        "strsbs" => strsbs(c, &p),
        "dlyjob" => dlyjob(c, &p),
        "sbmjob" => sbmjob(c, &p),
        other => Err(format!("{other} is not a job command")),
    };
    match r {
        Ok(s) => s,
        Err(e) => format!("{e}\n"),
    }
}

/// `rclactgrp NAME|*ELIGIBLE`.
fn rclactgrp(c: &mut Console, p: &Params) -> Result<String, String> {
    let name = p.pos_or(0, "ACTGRP")?.to_ascii_uppercase();
    let j = c.job();
    if name == "*ELIGIBLE" {
        j.rclactgrp_eligible();
        return Ok("Eligible activation groups reclaimed.\n".to_string());
    }
    Ok(match j.rclactgrp(&name) {
        Ok(()) => format!("Activation group {name} reclaimed.\n"),
        Err(e) => format!("{e}\n"),
    })
}

/// `dspmsgq [*PGM|*EXT|QSYSOPR]`.
fn dspmsgq(c: &mut Console, p: &Params) -> String {
    let which = p
        .pos
        .first()
        .map_or_else(|| "*EXT".to_string(), |s| s.to_ascii_uppercase());
    let q = match which.as_str() {
        "*PGM" | "*SAME" => QueueRef::Same,
        "QSYSOPR" | "*SYSOPR" => QueueRef::SysOpr,
        _ => QueueRef::Ext,
    };
    c.job().msgq_text(q)
}

/// `rpy KEY REPLY`: answer an inquiry on the external queue, or queue
/// the reply for the next one.
fn rpy(c: &mut Console, p: &Params) -> Result<String, String> {
    let (key, reply) = match p.pos.as_slice() {
        [k, r] => (k.clone(), r.to_ascii_uppercase()),
        [r] => (String::new(), r.to_ascii_uppercase()),
        _ => return Err("rpy [KEY] REPLY".into()),
    };
    let j = c.job();
    let k = u32::from_str_radix(&key, 16).ok();
    if let Some(k) = k
        && let Some(m) = j.ext_msgq.msgs.iter_mut().find(|m| m.key == k)
    {
        m.reply = Some(reply.clone());
        return Ok(format!("Reply {reply} recorded for message {key}.\n"));
    }
    j.inquiry_replies.push_back(reply.clone());
    Ok(format!("Reply {reply} queued for the next inquiry.\n"))
}

/// `strdbg LIB/PGM`.
fn strdbg(c: &mut Console, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "PGM")?;
    let (lib, name) = qual
        .split_once('/')
        .map_or(("*LIBL", qual.as_str()), |(l, n)| (l, n));
    let (lib, name) = (lib.to_ascii_uppercase(), name.to_ascii_uppercase());
    Ok(match c.job().strdbg(&name, &lib) {
        Ok(key) => format!("{key} is under debug; its next call runs the debug view.\n"),
        Err(e) => format!("{e}\n"),
    })
}

/// `addbkp N [PGM(LIB/PGM)]` and `rmvbkp N|*ALL [PGM(..)]`.
fn addbkp(c: &mut Console, p: &Params, add: bool) -> Result<String, String> {
    let n = p.pos_or(0, "STMT")?;
    let pgm = p.upper("PGM");
    let j = c.job();
    let program = pgm.unwrap_or_else(|| {
        j.debug
            .as_ref()
            .and_then(|d| d.programs.last().cloned())
            .unwrap_or_default()
    });
    if program.is_empty() {
        return Ok("No program is under debug; strdbg first or give PGM().\n".into());
    }
    if add {
        return Ok(match n.parse::<u32>() {
            Ok(stmt) => {
                j.addbkp(&program, stmt);
                format!("Breakpoint at {program} statement {stmt}.\n")
            }
            Err(_) => "addbkp N\n".into(),
        });
    }
    let gone = match n.parse::<u32>() {
        Ok(stmt) => j.rmvbkp(&program, Some(stmt)),
        Err(_) => j.rmvbkp(&program, None),
    };
    Ok(format!("{gone} breakpoint(s) removed.\n"))
}

/// `dspbkp`.
fn dspbkp(c: &mut Console) -> String {
    let j = c.job();
    let mut out = String::new();
    if let Some(d) = &j.debug {
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

/// `dtaara get *LDA` and `dtaara set *LDA TEXT`: the job's own area.
pub fn lda(c: &mut Console, args: &[&str]) -> String {
    match args {
        ["get", _] => {
            let text = ccsid::from_ebcdic(&c.lda.borrow());
            format!("*LDA: '{}'\n", text.trim_end())
        }
        ["set", _, rest @ ..] => {
            let text = rest.join(" ");
            let Ok(mut b) = ccsid::to_ebcdic(&text) else {
                return format!("{text}: not CP037\n");
            };
            b.resize(stg::remote::LDA_LEN, 0x40);
            *c.lda.borrow_mut() = b;
            "*LDA changed.\n".into()
        }
        _ => "Usage: dtaara get *LDA | set *LDA TEXT\n".into(),
    }
}

/// `STRSBS SBSD(LIB/NAME)`: start the subsystem's monitor and wait for
/// it to register.
fn strsbs(c: &mut Console, p: &Params) -> Result<String, String> {
    use stg::Client;
    let (_, name) = wm::obj::params::qualified(&p.pos_or(0, "SBSD")?);
    let qual = p.pos_or(0, "SBSD")?.to_ascii_uppercase();
    let (lib, _) = c.link.borrow_mut().with(|l| l.sbs_check(qual.clone()))?;
    let number = c.link.borrow_mut().with(Client::job_number)?;
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
    c.children.push(s);
    // The monitor registers the subsystem; wait for it, a few seconds.
    for _ in 0..50 {
        let active = c.link.borrow_mut().with(Client::sbs_list);
        if active.iter().any(|a| a.name == name) {
            return Ok(format!("Subsystem {name} started.\n"));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Err(format!("CPF1012 subsystem {name} did not start"))
}

/// `DLYJOB DLY(seconds)`: the job waits, in `DLYW`, taking break
/// messages as they come.
fn dlyjob(c: &mut Console, p: &Params) -> Result<String, String> {
    let secs: u64 = p
        .pos_or(0, "DLY")?
        .trim()
        .parse()
        .map_err(|_| "CPD0084 DLY is a number of seconds".to_string())?;
    let until = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    stg::remote::status(&c.link, wm::job::Status::Dlyw, &format!("DLY-{secs}"));
    while std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(250));
        c.breaks();
    }
    stg::remote::status(&c.link, wm::job::Status::Run, "");
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
/// Keywords accepted and kept for later phases.
const SBM_LATER: &[&str] = &[
    "DSPSBMJOB",
    "CPYENVVAR",
    "ALWMLTTHD",
    "INLASPGRP",
    "SYSLIBL",
    "SPLFACN",
];

/// `SCDDATE` (`YYYYMMDD` or `MMDDYY`, `*CURRENT`) and `SCDTIME`
/// (`HHMMSS`, `*CURRENT`) as Unix seconds; 0 for now.
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
/// `*LDA`; `MIGRATE(phase7)`: `CMD` is checked by the command analyzer.
fn sbmjob(c: &mut Console, p: &Params) -> Result<String, String> {
    use stg::Client;
    let cmd = p
        .kw("CMD")
        .or_else(|| p.kw("RQSDTA"))
        .map(|v| v.trim().trim_matches('\'').replace("''", "'"))
        .ok_or("CPD0071 CMD or RQSDTA is needed")?;
    let up = |k: &str| p.upper(k).unwrap_or_default();
    let mut req = wm::obj::jobq::SbmJob {
        cmd,
        job: up("JOB"),
        jobd: p
            .upper("JOBD")
            .filter(|v| v != "*USRPRF")
            .unwrap_or_default(),
        jobq: up("JOBQ"),
        jobpty: match p.upper("JOBPTY").as_deref() {
            None | Some("*JOBD") => None,
            Some(v) => Some(v.parse().map_err(|_| format!("CPD0084 JOBPTY({v})"))?),
        },
        user: up("USER"),
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
        msgq: up("MSGQ"),
        inllibl: match p.upper("INLLIBL").as_deref() {
            None | Some("*CURRENT") => None,
            Some("*JOBD") => Some(Vec::new()),
            Some(v) => Some(v.split_whitespace().map(str::to_string).collect()),
        },
        curlib: up("CURLIB"),
        changes: Vec::new(),
        lda: c.lda.borrow().clone(),
    };
    for (k, v) in &p.kw {
        if SBM_CHANGES.contains(&k.as_str()) {
            if !v.trim().eq_ignore_ascii_case("*JOBD") && !v.trim().eq_ignore_ascii_case("*CURRENT")
            {
                req.changes.push((k.clone(), v.trim().to_string()));
            }
        } else if !SBM_LATER.contains(&k.as_str())
            && !matches!(
                k.as_str(),
                "CMD"
                    | "RQSDTA"
                    | "JOB"
                    | "JOBD"
                    | "JOBQ"
                    | "JOBPTY"
                    | "USER"
                    | "RTGDTA"
                    | "HOLD"
                    | "SCDDATE"
                    | "SCDTIME"
                    | "MSGQ"
                    | "INLLIBL"
                    | "CURLIB"
            )
        {
            return Err(format!("CPD0043 keyword {k} not valid for SBMJOB"));
        }
    }
    let (_, msg) = c.link.borrow_mut().with(|l| l.sbmjob(req))?;
    Ok(format!("{msg}\n"))
}
