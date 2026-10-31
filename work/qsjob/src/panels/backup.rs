// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `GO SAVE` options 20 to 23 (`QSMNSAVE`), `GO RESTORE` 21 to 23
//! (`QSMNRST`), `RUNBCKUP` and the `DSPBCKUP` display (`QEZBCKUP`). The
//! options and plans come from the service (`QSBCKUP`); the commands run
//! here, in the job, because `STRSBS` is the job's: `ENDSBS *ALL`, the
//! saves or restores, the controlling subsystem started again.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::time::{Duration, Instant};

use cmd::Resolved;
use mi::Job;
use stg::Client;
use wm::opa::gosave::{self, Defaults, Menu};

use super::certs::{parm, show};
use crate::command::{Fail, Outcome};
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{Response, UimDialog};

/// The `DSPBCKUP` panel, at a display.
pub fn run(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved, _out: &mut Outcome) -> Option<Result<String, Fail>> {
    if r.name != "DSPBCKUP" {
        return None;
    }
    Some(dspbckup(job, host, surface, &parm(r, "BCKUPOPT")))
}

/// `QSMNSAVE`, `QSMNRST`, `RUNBCKUP`, with or without a display.
pub fn command(job: &mut Job, host: &Host, r: &Resolved) -> Result<String, Fail> {
    match r.name.as_str() {
        "QSMNSAVE" => menu_option(job, host, Menu::Save, &parm(r, "OPTION")),
        "QSMNRST" => menu_option(job, host, Menu::Restore, &parm(r, "OPTION")),
        _ => runbckup(job, host, &parm(r, "BCKUPOPT"), &parm(r, "DEV")),
    }
}

/// Run a command; its text, or its escape.
fn exec(job: &mut Job, host: &Host, text: &str) -> Result<String, Fail> {
    let out = crate::command::run(job, host, text);
    match out.escape {
        Some(f) => Err(f),
        None => Ok(out.text),
    }
}

fn dialog(host: &Host) -> Result<UimDialog, Fail> {
    let mut d = crate::panels::dialog(host, "QEZBCKUP")?;
    d.copyright = false;
    Ok(d)
}

fn dspbckup(job: &mut Job, host: &Host, surface: &mut Surface, which: &str) -> Result<String, Fail> {
    let text = exec(job, host, &format!("QSBCKUP ACTION(*FIELDS) BCKUPOPT({which})"))?;
    let mut d = dialog(host)?;
    for l in text.lines() {
        if let Some((k, v)) = l.split_once('=') {
            d.set(k, v);
        }
    }
    let panel = match which {
        "*WEEKLY" => "DSPBCKW",
        "*MONTHLY" => "DSPBCKM",
        _ => "DSPBCKD",
    };
    loop {
        match show(&mut d, surface, panel)? {
            Response::Return(16 | 17) => {
                d.message = Some(("Every user library and folder is in the backup list.".into(), false));
            }
            Response::Return(_) | Response::Exit | Response::Cancel | Response::Gone => return Ok(String::new()),
            _ => {}
        }
    }
}

/// The job runs inside a subsystem: `ENDSBS *ALL` would end it.
fn in_subsystem(host: &Host) -> Result<(), Fail> {
    if host.start.subsystem.is_empty() {
        return Ok(());
    }
    Err(Fail::new(
        "CPF1E36",
        format!(
            "CPF1E36 This job runs in subsystem {}; ending the subsystems would end it. Run the option from the console.",
            host.start.subsystem
        ),
    ))
}

/// The controlling subsystem, `LIB/NAME`.
fn controlling(host: &Host) -> String {
    let v = host
        .link
        .borrow_mut()
        .with(|l| l.sysval("QCTLSBSD".into()))
        .map(|v| ccsid::from_ebcdic(&v.data))
        .unwrap_or_default();
    let mut parts = v.split_whitespace();
    match (parts.next(), parts.next()) {
        (Some(n), Some(l)) => format!("{l}/{n}"),
        (Some(n), None) => format!("QSYS/{n}"),
        _ => "QSYS/QCTL".into(),
    }
}

/// Wait for the subsystems to end, five minutes at most.
fn wait_restricted(host: &Host) -> bool {
    let until = Instant::now() + Duration::from_secs(300);
    while Instant::now() < until {
        if host.link.borrow_mut().with(Client::sbs_list).is_empty() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    false
}

/// Show the commands an option runs; whether the operator went on.
fn confirm(host: &Host, cmds: &[String]) -> Result<bool, Fail> {
    if !crate::panels::shows_host_panels(host) {
        return Ok(true);
    }
    let mut surface = Surface::new(host.clone());
    let mut d = dialog(host)?;
    d.clear_list("CMDS");
    for c in cmds {
        d.add_row("CMDS", vec![("CMDLINE".to_string(), c.clone())]);
    }
    d.list_mut("CMDS").complete = true;
    loop {
        match show(&mut d, &mut surface, "SAVCMDS")? {
            Response::Return(_) => return Ok(true),
            Response::Exit | Response::Cancel | Response::Gone => return Ok(false),
            _ => {}
        }
    }
}

/// The command defaults: the panel at a display, the kept ones
/// otherwise. `None` when the operator left.
fn defaults(job: &mut Job, host: &Host) -> Result<Option<Defaults>, Fail> {
    let kept = Defaults::decode(&exec(job, host, "QSBCKUP ACTION(*DFT)")?);
    if !crate::panels::shows_host_panels(host) {
        // Nobody to answer a prompt.
        return Ok(Some(Defaults { prompt: false, ..kept }));
    }
    let mut surface = Surface::new(host.clone());
    let mut d = dialog(host)?;
    for (k, v) in kept.fields() {
        d.set(k, &v);
    }
    loop {
        match show(&mut d, &mut surface, "SAVDFT")? {
            Response::Return(_) => {
                let typed: BTreeMap<String, String> = kept.fields().iter().map(|(k, _)| ((*k).to_string(), d.get(k).clone())).collect();
                match Defaults::from_fields(|k| typed.get(k).cloned().unwrap_or_default()) {
                    Ok(v) => return Ok(Some(v)),
                    Err(gosave::Bad(f, v)) => d.message = Some((format!("Value '{v}' for {f} is not valid."), true)),
                }
            }
            Response::Exit | Response::Cancel | Response::Gone => return Ok(None),
            _ => {}
        }
    }
}

/// Run `cmds` in order; each one's outcome. A failed save does not stop
/// the others; the subsystems are started whatever happened.
fn run_all(job: &mut Job, host: &Host, cmds: &[String]) -> Vec<(String, bool, String)> {
    let mut out = Vec::new();
    for c in cmds {
        let verb = c.trim_start_matches('?').split_whitespace().next().unwrap_or("").to_string();
        let r = exec(job, host, c);
        if verb == "ENDSBS" && r.is_ok() && !wait_restricted(host) {
            out.push((verb, false, "the subsystems did not end in five minutes".into()));
            break;
        }
        let (ok, line) = match r {
            Ok(t) => (true, t.lines().map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("").to_string()),
            Err(f) => (false, f.text.lines().last().unwrap_or(&f.msgid).to_string()),
        };
        out.push((verb, ok, line));
    }
    // ENDSBS that never finished: start the controlling subsystem anyway.
    if out.last().is_some_and(|(v, ok, _)| v == "ENDSBS" && !ok)
        && let Some(s) = cmds.last()
    {
        let _ = exec(job, host, s);
    }
    out
}

fn report(results: &[(String, bool, String)]) -> String {
    let mut s = String::new();
    for (v, ok, line) in results {
        let _ = writeln!(s, "{v}: {} {line}", if *ok { "ok" } else { "failed" });
    }
    s
}

/// Record a run in the backup history.
fn record(job: &mut Job, host: &Host, option: &str, dev: &str, tapset: &str, started: u64, results: &[(String, bool, String)]) -> Result<String, Fail> {
    let ok = results.iter().all(|(_, ok, _)| *ok);
    let steps: Vec<String> = results
        .iter()
        .map(|(v, ok, _)| format!("{v} {}", if *ok { "completed" } else { "failed" }).replace(['\'', ';'], " "))
        .collect();
    let data = format!("{};{tapset};{started};{}", u8::from(ok), steps.join(";"));
    exec(
        job,
        host,
        &format!("QSBCKUP ACTION(*DONE) BCKUPOPT({option}) DEV({dev}) DATA('{data}')"),
    )
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `GO SAVE` 20 to 23, `GO RESTORE` 21 to 23.
fn menu_option(job: &mut Job, host: &Host, menu: Menu, option: &str) -> Result<String, Fail> {
    let Some(d) = defaults(job, host)? else {
        return Ok(String::new());
    };
    if option == "20" {
        let data = d.encode().replace('\n', ";");
        exec(job, host, &format!("QSBCKUP ACTION(*SETDFT) DATA('{data}')"))?;
        return Ok("Save defaults kept for options 21, 22 and 23.\n".into());
    }
    let n: u8 = option.parse().map_err(|_| Fail::new("CPD0084", format!("OPTION({option}) is not valid")))?;
    in_subsystem(host)?;
    let have = |c: &str| sys_cpps::entry(c).is_some();
    let cmds = gosave::sequence(menu, n, &d, &controlling(host), &have);
    if !confirm(host, &cmds)? {
        return Ok(String::new());
    }
    let started = now();
    let results = run_all(job, host, &cmds);
    let what = match menu {
        Menu::Save => format!("SAVE{option}"),
        Menu::Restore => format!("RESTORE{option}"),
    };
    let dev = d.devices.first().cloned().unwrap_or_default();
    let summary = report(&results);
    if menu == Menu::Save {
        record(job, host, &what, &dev, "*NONE", started, &results)
            .map(|t| format!("{summary}{t}"))
            .map_err(|mut f| {
                f.text = format!("{summary}{}", f.text);
                f
            })
    } else if results.iter().all(|(_, ok, _)| *ok) {
        Ok(summary)
    } else {
        Err(Fail::new("CPF1E32", summary))
    }
}

/// `RUNBCKUP BCKUPOPT() DEV()`.
fn runbckup(job: &mut Job, host: &Host, which: &str, dev: &str) -> Result<String, Fail> {
    let dev = dev.trim_matches(|c| c == '(' || c == ')').split_whitespace().next().unwrap_or("*BCKUPOPT").to_string();
    let plan = exec(job, host, &format!("QSBCKUP ACTION(*PLAN) BCKUPOPT({which}) DEV({dev})"))?;
    let mut lines = plan.lines();
    let head: Vec<&str> = lines.next().unwrap_or("").split_whitespace().collect();
    let field = |k: &str| head.iter().position(|w| *w == k).and_then(|i| head.get(i + 1)).copied().unwrap_or("");
    let dev = field("DEV").to_string();
    let tapset = field("TAPSET").to_string();
    let steps: Vec<(bool, String)> = lines
        .filter_map(|l| l.split_once(' ').map(|(m, c)| (m == "R", c.to_string())))
        .collect();
    let restricted = steps.iter().any(|(r, _)| *r) && field("RESTRICTED") != "Y";
    // Submitted to batch when asked, unless the subsystems must end.
    if field("SBMJOB") == "Y" && host.interactive() && !restricted {
        return exec(
            job,
            host,
            &format!("SBMJOB CMD(RUNBCKUP BCKUPOPT({which}) DEV({dev})) JOB(QEZBCKUP) JOBQ(QSYS/QBATCH)"),
        );
    }
    let mut cmds: Vec<String> = Vec::new();
    if restricted {
        in_subsystem(host)?;
        cmds.push("ENDSBS SBS(*ALL) OPTION(*IMMED)".into());
    }
    cmds.extend(steps.iter().map(|(_, c)| c.clone()));
    if restricted {
        cmds.push(format!("STRSBS SBSD({})", controlling(host)));
        if !confirm(host, &cmds)? {
            return Ok(String::new());
        }
    }
    let started = now();
    let results = run_all(job, host, &cmds);
    let summary = report(&results);
    let r = record(job, host, which, &dev, &tapset, started, &results);
    if field("IPL") == "Y" && results.iter().all(|(_, ok, _)| *ok) {
        let _ = exec(job, host, "PWRDWNSYS OPTION(*CNTRLD) RESTART(*YES) CONFIRM(*NO)");
    }
    r.map(|t| format!("{summary}{t}")).map_err(|mut f| {
        f.text = format!("{summary}{}", f.text);
        f
    })
}
