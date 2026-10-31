// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Work-management displays: `WRKACTJOB` and its views, `WRKUSRJOB`,
//! `WRKSBSJOB`, `WRKJOB` and its options, `DSPJOB`, `WRKSBS`,
//! `DSPSBSD`, `WRKJOBD`/`DSPJOBD`, `WRKCLS`/`DSPCLS`, `WRKSPLF`,
//! `DSPJOBLOG`, `WRKOUTQ`, `WRKJOBQ`, `WRKWTR`, `WRKJOBSCDE`. Each
//! driver fetches its data through the storage service, fills a dialog
//! and shows a shipped panel group; the list options run their
//! commands, F5 fetches again, F3 and F12 leave. The statistics
//! displays are in `work2`.

mod actjob;
mod job;
mod jobs;
mod objs;
mod queues;
mod sbs;
mod splf;

use cmd::{Resolved, Value};
use mi::Job;
use stg::Client;
use uim::model::Action;

use crate::command::{Fail, Outcome};
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{CMDLINE_VAR, Choice, Response, UimDialog, UimError};

/// The driver of a work-management command, if it has one.
pub fn run(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
    _out: &mut Outcome,
) -> Option<Result<String, Fail>> {
    let res = match r.name.as_str() {
        "WRKACTJOB" => actjob::wrkactjob(job, host, surface, r),
        "WRKJOB" => job::wrkjob(job, host, surface, r),
        "WRKUSRJOB" => jobs::wrkusrjob(job, host, surface, r),
        "WRKSBSJOB" => jobs::wrksbsjob(job, host, surface, r),
        "WRKSBS" => sbs::wrksbs(job, host, surface),
        "DSPSBSD" => sbs::dspsbsd(job, host, surface, r),
        "WRKJOBD" => objs::wrkjobd(job, host, surface, r),
        "DSPJOBD" => objs::dspjobd(job, host, surface, r),
        "WRKCLS" => objs::wrkcls(job, host, surface, r),
        "DSPCLS" => objs::dspcls(job, host, surface, r),
        "WRKSPLF" => splf::wrksplf(job, host, surface, r),
        "WRKOUTQ" => queues::wrkoutq(job, host, surface, r),
        "WRKJOBQ" => queues::wrkjobq(job, host, surface, r),
        "WRKWTR" => queues::wrkwtr(job, host, surface, r),
        "WRKJOBSCDE" => queues::wrkjobscde(job, host, surface, r),
        "DSPJOB" => job::dspjob(job, host, surface, r),
        "DSPJOBLOG" => job::dspjoblog(job, host, surface, r),
        _ => return None,
    };
    Some(res)
}

/// A parameter's value as typed or defaulted, upper case; `None` when
/// the command has no value for it.
fn parm(r: &Resolved, kwd: &str) -> Option<String> {
    r.get(kwd)
        .and_then(|b| b.value.as_ref())
        .map(|v| v.source().to_ascii_uppercase())
        .filter(|s| !s.is_empty())
}

/// A qualified `LIB/NAME` parameter as `(lib, name)`, the library
/// `*LIBL` when not given.
fn qualified(r: &Resolved, kwd: &str) -> Option<(String, String)> {
    let v = r.get(kwd).and_then(|b| b.value.as_ref())?;
    let text = match v {
        Value::Qual(_) | Value::Char(_) => v.source(),
        other => other.source(),
    };
    let text = text.to_ascii_uppercase();
    Some(match text.split_once('/') {
        Some((lib, name)) => (lib.to_string(), name.to_string()),
        None => ("*LIBL".to_string(), text),
    })
}

/// The panel engine refused: the group or panel is not what we ship.
fn render_fail(e: &UimError) -> Fail {
    crate::panels::failed(format!("panel: {e}"))
}

/// The date, time and zone for `TOPSEP=DATETIME`, in the job's date
/// format: `MM/DD/YY  HH:MM:SS UTC`.
pub(crate) fn stamp(host: &Host) -> String {
    let now = host.link.borrow_mut().with(Client::now);
    let a = host.attrs();
    let (date, time) = date_time(now.unix, now.offset_min, &a.datfmt, &a.datsep);
    let zone: String = now
        .zone
        .trim()
        .trim_start_matches(|c: char| !c.is_ascii_digit())
        .trim_start_matches(|c: char| c.is_ascii_digit())
        .to_string();
    let zone = if zone.is_empty() { "UTC".to_string() } else { zone };
    format!("{date}  {time} {zone}")
}

/// A Unix time as the job shows dates and times.
fn date_time(unix: u64, offset_min: i32, datfmt: &str, datsep: &str) -> (String, String) {
    let local = i64::try_from(unix).unwrap_or(0) + i64::from(offset_min) * 60;
    let days = local.div_euclid(86_400);
    let secs = local.rem_euclid(86_400);
    let (y, m, d) = civil(days);
    let sep = datsep.chars().next().filter(|c| *c != ' ').unwrap_or('/');
    let yy = y % 100;
    let date = match datfmt.trim() {
        "*YMD" => format!("{yy:02}{sep}{m:02}{sep}{d:02}"),
        "*DMY" => format!("{d:02}{sep}{m:02}{sep}{yy:02}"),
        "*JUL" => format!("{yy:02}{sep}{:03}", day_of_year(y, m, d)),
        _ => format!("{m:02}{sep}{d:02}{sep}{yy:02}"),
    };
    let time = format!("{:02}:{:02}:{:02}", secs / 3600, secs % 3600 / 60, secs % 60);
    (date, time)
}

/// Days since 1970 as year, month, day.
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

fn day_of_year(y: i64, m: i64, d: i64) -> i64 {
    const CUM: [i64; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let idx = usize::try_from(m - 1).unwrap_or(0).min(11);
    CUM[idx] + d + i64::from(leap && m > 2)
}

/// Tenths as the job writes decimals: `12,3` or `12.3`.
fn decimal(host: &Host, tenths: u64) -> String {
    let comma = matches!(host.attrs().decfmt.trim(), "J" | "I");
    let sep = if comma { ',' } else { '.' };
    format!("{}{sep}{}", tenths / 10, tenths % 10)
}

/// `number/user/name` of a job.
fn qual_job(number: u32, user: &str, name: &str) -> String {
    format!("{number:06}/{user}/{name}")
}

/// The first line of what a command said, for the message line.
fn first_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
}

/// Run a command for a panel; what it said goes on the message line.
fn run_command(job: &mut Job, host: &Host, d: &mut UimDialog, history: &mut Vec<String>, text: &str) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    history.push(text.to_string());
    let out = crate::command::run(job, host, text);
    if let Some(e) = &out.escape {
        let msg = first_line(&out.text).unwrap_or_else(|| format!("{} {}", e.msgid, e.text));
        d.message = Some((msg, true));
    } else if let Some(msg) = first_line(&out.text) {
        d.message = Some((msg, false));
    }
}

/// Options typed on list entries: each `CMD` action runs with the
/// entry's values; a `RETURN n` goes back to the driver.
fn run_options(
    job: &mut Job,
    host: &Host,
    d: &mut UimDialog,
    history: &mut Vec<String>,
    choices: &[Choice],
) -> Vec<(u32, Choice)> {
    let mut returned = Vec::new();
    for c in choices {
        match &c.action {
            Some(Action::Cmd(text)) => {
                let cmd = d.substitute(text, Some((&c.list, c.row)));
                run_command(job, host, d, history, &cmd);
            }
            Some(Action::Return(n)) => returned.push((*n, c.clone())),
            _ => {
                d.message = Some((format!("Option {} is not valid here.", c.option), true));
            }
        }
    }
    for c in choices {
        d.list_mut(&c.list).options.clear();
    }
    returned
}

/// What a list driver does after a key.
enum Flow {
    /// Show the panel again.
    Stay,
    /// Fetch the data again, then show it.
    Refresh,
    /// Leave the display.
    Leave,
}

/// The keys every list display shares: commands, options, retrieve,
/// leave; a `RETURN n` the driver does not know is not available yet.
fn common(
    job: &mut Job,
    host: &Host,
    d: &mut UimDialog,
    history: &mut Vec<String>,
    resp: Response,
) -> Flow {
    match resp {
        Response::Exit | Response::Cancel | Response::Gone => Flow::Leave,
        Response::Return(1 | 5) => Flow::Refresh,
        Response::Command(c) => {
            run_command(job, host, d, history, &c);
            Flow::Refresh
        }
        Response::Options(choices) => {
            for (n, c) in run_options(job, host, d, history, &choices) {
                d.message = Some((format!("Option {} ({n}) is not available yet.", c.option), true));
            }
            Flow::Refresh
        }
        Response::Retrieve => {
            if let Some(last) = history.last() {
                d.set(CMDLINE_VAR, last);
            }
            Flow::Stay
        }
        Response::Menu(m) => {
            run_command(job, host, d, history, &format!("GO {m}"));
            Flow::Refresh
        }
        Response::Return(n) => {
            d.message = Some((format!("F{n} is not available in this release."), true));
            Flow::Stay
        }
        Response::Panel(_) | Response::Print => {
            d.message = Some(("That function is not available in this release.".into(), true));
            Flow::Stay
        }
    }
}

/// Replace a list's entries, keeping the page where it was.
fn refill(d: &mut UimDialog, list: &str, rows: Vec<Vec<(String, String)>>) {
    let l = d.list_mut(list);
    let top = l.top;
    l.rows = rows;
    l.complete = true;
    l.options.clear();
    l.top = if top < l.rows.len() { top } else { 0 };
}

/// An entry from `(variable, value)` pairs.
fn entry(pairs: &[(&str, String)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.clone()))
        .collect()
}

/// Show a display panel (no list options) until it is left: F3
/// leaves the command, Enter, F12 and F16 go back; F5 runs `refresh`
/// and F9 prompts the dialog's `CHGCMD`.
fn show_display(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    d: &mut UimDialog,
    panel: &str,
    refresh: &mut dyn FnMut(&mut UimDialog),
) -> Result<job::Left, Fail> {
    let mut history = Vec::new();
    loop {
        match d.show(surface, panel).map_err(|e| render_fail(&e))? {
            Response::Return(5) => refresh(d),
            Response::Return(9) => {
                let cmd = d.get("CHGCMD");
                if !cmd.trim().is_empty() {
                    run_command(job, host, d, &mut history, &format!("?{cmd}"));
                    refresh(d);
                }
            }
            Response::Exit | Response::Gone => return Ok(job::Left::Exit),
            Response::Return(_) | Response::Cancel => return Ok(job::Left::Back),
            other => {
                if let Flow::Leave = common(job, host, d, &mut history, other) {
                    return Ok(job::Left::Exit);
                }
            }
        }
    }
}
