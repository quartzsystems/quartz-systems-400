// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `trap`, `kill`, `wait`, `jobs`, `fg`, `bg`, `times`, `ulimit`.

use std::time::Duration;

use super::{fail, opts_or_fail};
use crate::msg::Msg;
use crate::shell::jobs::{JobState, state_text};
use crate::shell::traps::Action;
use crate::shell::vars::quote;
use crate::shell::{Res, Shell};
use crate::signal;
use crate::sys::{Limit, SigAction, WaitStatus};

fn cond_name(sig: i32) -> String {
    if sig == 0 {
        "EXIT".to_string()
    } else {
        signal::name(sig).map_or_else(|| sig.to_string(), str::to_string)
    }
}

/// `trap [action condition...]`.
pub(crate) fn trap(sh: &mut Shell, args: &[String]) -> Res {
    let mut a = &args[1..];
    if a.first().is_some_and(|x| x == "--") {
        a = &a[1..];
    }
    if a.is_empty() || (a.len() == 1 && a[0] == "-p") {
        let lines: Vec<String> = sh
            .traps
            .iter()
            .map(|(sig, act)| {
                let text = match act {
                    Action::Run(c) => quote(c),
                    Action::Ignore => "''".to_string(),
                };
                format!("trap -- {text} {}\n", cond_name(*sig))
            })
            .collect();
        for l in lines {
            sh.out(&l)?;
        }
        return Ok(0);
    }
    // A first operand that is a number: every operand is reset.
    let (action, conds) = if a[0].parse::<u32>().is_ok() {
        (None, a)
    } else {
        (Some(a[0].as_str()), &a[1..])
    };
    let mut st = 0;
    for c in conds {
        let Some(sig) = signal::parse(c) else {
            sh.report(&Msg::BadSignal("trap".to_string(), c.clone()));
            st = 1;
            continue;
        };
        if matches!(sig, signal::SIGKILL | signal::SIGSTOP) {
            sh.report(&Msg::Usage(
                "trap".to_string(),
                format!("signal {c} cannot be trapped."),
            ));
            st = 1;
            continue;
        }
        let act = match action {
            None | Some("-") => None,
            Some("") => Some(Action::Ignore),
            Some(cmd) => Some(Action::Run(cmd.to_string())),
        };
        if sig != 0 {
            let sa = match &act {
                None => SigAction::Default,
                Some(Action::Ignore) => SigAction::Ignore,
                Some(Action::Run(_)) => SigAction::Catch,
            };
            sh.sys.signal_action(sh.self_pid, sig, sa);
        }
        sh.traps.set(sig, act);
    }
    Ok(st)
}

/// Resolve a `kill` or `wait` operand to a process ID.
fn target(sh: &mut Shell, who: &str, op: &str) -> Result<i32, String> {
    if op.starts_with('%') {
        sh.jobs.poll();
        return match sh.jobs.find(op) {
            Some(id) => Ok(sh.jobs.iter().find(|j| j.id == id).map_or(0, |j| j.pid)),
            None => Err(Msg::NoJob(who.to_string(), op.to_string()).to_string()),
        };
    }
    op.parse::<i32>()
        .map_err(|_| Msg::NotNumber(who.to_string(), op.to_string()).to_string())
}

/// `kill [-s sig | -sig] pid...`, `kill -l [status]`.
pub(crate) fn kill(sh: &mut Shell, args: &[String]) -> Res {
    let Some(first) = args.get(1) else {
        return Err(fail(
            sh,
            &Msg::Usage(
                "kill".to_string(),
                "a process ID or job is required.".to_string(),
            ),
            2,
        ));
    };
    if first == "-l" || first == "-L" {
        if let Some(s) = args.get(2) {
            let n: i32 = s.parse().unwrap_or(-1);
            let n = if n > 128 { n - 128 } else { n };
            return match signal::name(n) {
                Some(name) => {
                    sh.out(&format!("{name}\n"))?;
                    Ok(0)
                }
                None => Err(fail(sh, &Msg::BadSignal("kill".to_string(), s.clone()), 1)),
            };
        }
        let names: Vec<&str> = signal::SIGNALS.iter().map(|(_, n)| *n).collect();
        sh.out(&format!("{}\n", names.join(" ")))?;
        return Ok(0);
    }
    let (sig, rest) = if first == "-s" || first == "-n" {
        let Some(s) = args.get(2) else {
            return Err(fail(
                sh,
                &Msg::Usage("kill".to_string(), "a signal is required.".to_string()),
                2,
            ));
        };
        (s.clone(), &args[3..])
    } else if let Some(s) = first
        .strip_prefix('-')
        .filter(|s| !s.is_empty() && *s != "-")
    {
        (s.to_string(), &args[2..])
    } else if first == "--" {
        ("TERM".to_string(), &args[2..])
    } else {
        ("TERM".to_string(), &args[1..])
    };
    let Some(n) = signal::parse(&sig) else {
        return Err(fail(sh, &Msg::BadSignal("kill".to_string(), sig), 1));
    };
    if rest.is_empty() {
        return Err(fail(
            sh,
            &Msg::Usage(
                "kill".to_string(),
                "a process ID or job is required.".to_string(),
            ),
            2,
        ));
    }
    let mut st = 0;
    for op in rest {
        match target(sh, "kill", op) {
            Ok(pid) => {
                if let Err(e) = sh.sys.kill(pid, n) {
                    sh.report(&Msg::Failed("kill".to_string(), format!("{op}: {e}")));
                    st = 1;
                }
            }
            Err(m) => {
                sh.err(&format!("qsh: {m}\n"));
                st = 1;
            }
        }
    }
    Ok(st)
}

/// `wait [pid|%job...]`.
pub(crate) fn wait(sh: &mut Shell, args: &[String]) -> Res {
    if args.len() == 1 {
        for id in sh.jobs.ids() {
            sh.jobs.wait(id);
        }
        return Ok(0);
    }
    let mut st = 0;
    for op in &args[1..] {
        st = match target(sh, "wait", op) {
            Ok(pid) => match sh.jobs.find(&pid.to_string()) {
                Some(id) => sh.jobs.wait(id).map_or(127, WaitStatus::code),
                None => 127,
            },
            Err(m) => {
                sh.err(&format!("qsh: {m}\n"));
                127
            }
        };
    }
    Ok(st)
}

/// `jobs [-l|-p] [job...]`.
pub(crate) fn jobs(sh: &mut Shell, args: &[String]) -> Res {
    let o = opts_or_fail(sh, args, "lp", "", false)?;
    sh.jobs.poll();
    let cur = sh.jobs.current();
    let prev = sh.jobs.previous();
    let wanted: Option<Vec<u32>> = if o.operands.is_empty() {
        None
    } else {
        let mut v = Vec::new();
        for op in &o.operands {
            match sh.jobs.find(op) {
                Some(id) => v.push(id),
                None => {
                    sh.report(&Msg::NoJob("jobs".to_string(), op.clone()));
                    return Ok(1);
                }
            }
        }
        Some(v)
    };
    let mut text = String::new();
    for j in sh.jobs.iter() {
        if wanted.as_ref().is_some_and(|w| !w.contains(&j.id)) {
            continue;
        }
        if o.has('p') {
            text.push_str(&format!("{}\n", j.pid));
            continue;
        }
        let mark = if Some(j.id) == cur {
            '+'
        } else if Some(j.id) == prev {
            '-'
        } else {
            ' '
        };
        if o.has('l') {
            text.push_str(&format!(
                "[{}] {mark} {} {} {}\n",
                j.id,
                j.pid,
                state_text(j.state),
                j.text
            ));
        } else {
            text.push_str(&format!(
                "[{}] {mark} {} {}\n",
                j.id,
                state_text(j.state),
                j.text
            ));
        }
    }
    sh.out(&text)?;
    sh.jobs.reap();
    Ok(0)
}

/// `fg [job]`: wait for the job in the foreground.
pub(crate) fn fg(sh: &mut Shell, args: &[String]) -> Res {
    sh.jobs.poll();
    let spec = args.get(1).map_or("%+", String::as_str);
    let Some(id) = sh.jobs.find(spec) else {
        return Err(fail(sh, &Msg::NoJob("fg".to_string(), spec.to_string()), 1));
    };
    let text = sh
        .jobs
        .iter()
        .find(|j| j.id == id)
        .map(|j| j.text.clone())
        .unwrap_or_default();
    sh.out(&format!("{text}\n"))?;
    if let Some(jc) = sh.sys.job_control() {
        let pid = sh.jobs.iter().find(|j| j.id == id).map_or(0, |j| j.pid);
        let _ = jc.give_terminal(pid);
        let _ = sh.sys.kill(pid, signal::SIGCONT);
    }
    let st = sh.jobs.wait(id).map_or(127, WaitStatus::code);
    if let Some(jc) = sh.sys.job_control() {
        let _ = jc.take_terminal();
    }
    Ok(st)
}

/// `bg [job...]`: continue stopped jobs in the background.
pub(crate) fn bg(sh: &mut Shell, args: &[String]) -> Res {
    sh.jobs.poll();
    let specs: Vec<String> = if args.len() > 1 {
        args[1..].to_vec()
    } else {
        vec!["%+".to_string()]
    };
    for spec in specs {
        let Some(id) = sh.jobs.find(&spec) else {
            return Err(fail(sh, &Msg::NoJob("bg".to_string(), spec), 1));
        };
        let (pid, text) = sh
            .jobs
            .iter()
            .find(|j| j.id == id)
            .map(|j| (j.pid, j.text.clone()))
            .unwrap_or_default();
        if let Some(j) = sh.jobs.get_mut(id)
            && matches!(j.state, JobState::Stopped(_))
        {
            j.state = JobState::Running;
            let _ = sh.sys.kill(pid, signal::SIGCONT);
        }
        sh.out(&format!("[{id}] {text} &\n"))?;
    }
    Ok(0)
}

fn mins_secs(d: Duration) -> String {
    let s = d.as_secs_f64();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let m = (s / 60.0).floor() as u64;
    #[allow(clippy::cast_precision_loss)]
    let rest = s - (m as f64) * 60.0;
    format!("{m}m{rest:.2}s")
}

/// `times`.
pub(crate) fn times(sh: &mut Shell, _args: &[String]) -> Res {
    let t = sh.sys.times();
    let text = format!(
        "{} {}\n{} {}\n",
        mins_secs(t.user),
        mins_secs(t.system),
        mins_secs(t.child_user),
        mins_secs(t.child_system)
    );
    sh.out(&text)?;
    Ok(0)
}

const LIMITS: &[(char, Limit, &str)] = &[
    ('t', Limit::CpuTime, "time(seconds)"),
    ('f', Limit::FileSize, "file(blocks)"),
    ('d', Limit::Data, "data(kbytes)"),
    ('s', Limit::Stack, "stack(kbytes)"),
    ('c', Limit::Core, "coredump(blocks)"),
    ('n', Limit::OpenFiles, "nofiles(descriptors)"),
    ('v', Limit::Memory, "vmemory(kbytes)"),
];

/// `ulimit [-HS] [-a | -tfdscnv] [limit]`.
pub(crate) fn ulimit(sh: &mut Shell, args: &[String]) -> Res {
    let o = opts_or_fail(sh, args, "HSatfdscnv", "", false)?;
    let hard = o.has('H') && !o.has('S');
    let show = |v: Option<u64>| v.map_or_else(|| "unlimited".to_string(), |n| n.to_string());
    if o.has('a') {
        let mut text = String::new();
        for (_, l, label) in LIMITS {
            let v = sh.sys.get_limit(*l, hard).unwrap_or(None);
            text.push_str(&format!("{label:<21}{}\n", show(v)));
        }
        sh.out(&text)?;
        return Ok(0);
    }
    let which = o
        .flags
        .iter()
        .rev()
        .find_map(|(c, _)| LIMITS.iter().find(|(l, _, _)| l == c))
        .map_or(Limit::FileSize, |(_, l, _)| *l);
    match o.operands.first() {
        None => {
            let v = match sh.sys.get_limit(which, hard) {
                Ok(v) => v,
                Err(e) => {
                    return Err(fail(
                        sh,
                        &Msg::Failed("ulimit".to_string(), e.to_string()),
                        1,
                    ));
                }
            };
            sh.out(&format!("{}\n", show(v)))?;
            Ok(0)
        }
        Some(v) => {
            let value = if v == "unlimited" {
                None
            } else {
                match v.parse::<u64>() {
                    Ok(n) => Some(n),
                    Err(_) => {
                        return Err(fail(
                            sh,
                            &Msg::NotNumber("ulimit".to_string(), v.clone()),
                            1,
                        ));
                    }
                }
            };
            let both = !o.has('H') && !o.has('S');
            let r = if both {
                sh.sys
                    .set_limit(which, true, value)
                    .and_then(|()| sh.sys.set_limit(which, false, value))
            } else {
                sh.sys.set_limit(which, hard, value)
            };
            match r {
                Ok(()) => Ok(0),
                Err(e) => Err(fail(
                    sh,
                    &Msg::Failed("ulimit".to_string(), e.to_string()),
                    1,
                )),
            }
        }
    }
}
