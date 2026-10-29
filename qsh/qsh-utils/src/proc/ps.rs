// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `ps [-aAdefl] [-o format]... [-p pidlist] [-u userlist] [-g grouplist]
//! [-t termlist]`: process status from the system. The default
//! columns are Qshell's: `PID DEVICE TIME FUNCTION STATUS JOBID`.

use qsh::sys::{ProcInfo, UtilCtx};

use crate::common::args::getopt;
use crate::common::io::Out;
use crate::common::{complain_sys, usage};

/// A column: keyword, default header, left-justified.
struct Col {
    key: &'static str,
    header: String,
    left: bool,
}

fn known(key: &str) -> Option<(&'static str, &'static str, bool)> {
    Some(match key {
        "pid" => ("pid", "PID", false),
        "ppid" => ("ppid", "PPID", false),
        "pgid" => ("pgid", "PGID", false),
        "user" | "ruser" => ("user", "USER", true),
        "uid" | "ruid" => ("uid", "UID", false),
        "group" | "rgroup" => ("group", "GROUP", true),
        "gid" | "rgid" => ("gid", "GID", false),
        "tty" | "device" => ("device", "DEVICE", true),
        "time" => ("time", "TIME", false),
        "etime" => ("etime", "ELAPSED", false),
        "comm" => ("comm", "COMMAND", true),
        "args" => ("args", "COMMAND", true),
        "jobid" => ("jobid", "JOBID", true),
        "jobname" => ("jobname", "JOBNAME", true),
        "jobuser" => ("jobuser", "JOBUSER", true),
        "jobnumber" => ("jobnumber", "JOBNUMBER", true),
        "status" => ("status", "STATUS", true),
        "function" => ("function", "FUNCTION", true),
        "nice" => ("nice", "NI", false),
        "pcpu" => ("pcpu", "%CPU", false),
        "vsz" => ("vsz", "VSZ", false),
        _ => return None,
    })
}

fn cols_of(spec: &str) -> Option<Vec<Col>> {
    let mut out = Vec::new();
    for item in spec.split([',', ' ']).filter(|s| !s.is_empty()) {
        let (key, header) = match item.split_once('=') {
            Some((k, h)) => (k, Some(h)),
            None => (item, None),
        };
        let (key, dflt, left) = known(key)?;
        out.push(Col {
            key,
            header: header.unwrap_or(dflt).to_string(),
            left,
        });
    }
    Some(out)
}

/// `[[dd-]hh:]mm:ss`.
fn clock(secs: u64) -> String {
    let (d, h, m, s) = (secs / 86400, secs / 3600 % 24, secs / 60 % 60, secs % 60);
    if d > 0 {
        format!("{d}-{h:02}:{m:02}:{s:02}")
    } else if h > 0 {
        format!("{h:02}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

fn value(cx: &UtilCtx<'_>, key: &str, p: &ProcInfo) -> String {
    let job: Vec<&str> = p.job.split('/').collect();
    match key {
        "pid" => p.pid.to_string(),
        "ppid" => p.ppid.to_string(),
        "pgid" => p.pgid.to_string(),
        "user" => cx
            .sys
            .user_name(p.uid)
            .unwrap_or_else(|| p.uid.to_string()),
        "uid" => p.uid.to_string(),
        "group" | "gid" => "0".to_string(),
        "device" => p.device.clone(),
        "time" => clock(p.cpu.as_secs()),
        "etime" => clock(0),
        "comm" => p
            .args
            .first()
            .map(|a| a.rsplit('/').next().unwrap_or(a).to_string())
            .unwrap_or_default(),
        "args" => p.args.join(" "),
        "jobid" => p.job.clone(),
        "jobnumber" => job.first().copied().unwrap_or("").to_string(),
        "jobuser" => job.get(1).copied().unwrap_or("").to_string(),
        "jobname" => job.get(2).copied().unwrap_or("").to_string(),
        "status" => p.status.clone(),
        "function" => p.function.clone(),
        "nice" => "0".to_string(),
        "pcpu" => "0.0".to_string(),
        "vsz" => "0".to_string(),
        _ => String::new(),
    }
}

fn list(s: &str) -> Vec<String> {
    s.split([',', ' '])
        .filter(|x| !x.is_empty())
        .map(ToString::to_string)
        .collect()
}

/// Run `ps`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "ps";
    let p = match getopt(&cx.argv[1..], "aAdeflo:p:u:g:G:t:") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    let mut cols: Vec<Col> = Vec::new();
    for spec in p.values('o') {
        match cols_of(spec) {
            Some(c) => cols.extend(c),
            None => return usage(cx, name, &format!("The format {spec} is not valid.")),
        }
    }
    if cols.is_empty() {
        let spec = if p.has('l') {
            "uid,pid,ppid,device,time,function,status,jobid"
        } else if p.has('f') {
            "user,pid,ppid,device,time,args"
        } else {
            "pid,device,time,function,status,jobid"
        };
        cols = cols_of(spec).unwrap_or_default();
    }
    let procs = match cx.sys.processes() {
        Ok(v) => v,
        Err(e) => {
            complain_sys(cx, name, "processes", e);
            return 1;
        }
    };
    let pids: Vec<i32> = list(p.value('p').unwrap_or(""))
        .iter()
        .filter_map(|s| s.parse().ok())
        .collect();
    let users = list(p.value('u').unwrap_or(""));
    let devices = list(p.value('t').unwrap_or(""));
    let selecting = !pids.is_empty() || !users.is_empty() || !devices.is_empty();
    let rows: Vec<&ProcInfo> = procs
        .iter()
        .filter(|pr| {
            if !selecting {
                return true;
            }
            pids.contains(&pr.pid)
                || users.iter().any(|u| {
                    u.parse::<u32>().ok() == Some(pr.uid)
                        || cx
                            .sys
                            .user_name(pr.uid)
                            .is_some_and(|n| n.eq_ignore_ascii_case(u))
                })
                || devices.contains(&pr.device)
        })
        .collect();
    let table: Vec<Vec<String>> = rows
        .iter()
        .map(|pr| cols.iter().map(|c| value(cx, c.key, pr)).collect())
        .collect();
    let widths: Vec<usize> = cols
        .iter()
        .enumerate()
        .map(|(i, c)| {
            table
                .iter()
                .map(|r| r[i].len())
                .max()
                .unwrap_or(0)
                .max(c.header.len())
        })
        .collect();
    let mut out = Out::stdout(cx);
    let line = |cells: Vec<String>| -> String {
        let mut s = String::new();
        let n = cells.len();
        for (i, cell) in cells.iter().enumerate() {
            let w = widths[i];
            if i > 0 {
                s.push(' ');
            }
            if cols[i].left {
                if i + 1 == n {
                    s.push_str(cell);
                } else {
                    s.push_str(&format!("{cell:<w$}"));
                }
            } else {
                s.push_str(&format!("{cell:>w$}"));
            }
        }
        s.push('\n');
        s
    };
    if cols.iter().any(|c| !c.header.is_empty()) {
        out.text(&line(cols.iter().map(|c| c.header.clone()).collect()));
    }
    for r in table {
        out.text(&line(r));
    }
    out.finish(cx, name, 0)
}
