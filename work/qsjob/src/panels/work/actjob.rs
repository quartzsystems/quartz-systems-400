// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `WRKACTJOB` (`QWCWRKAJ`): the active jobs under their subsystems in
//! three views (one wide view at a 27x132 display), measured from the
//! display's start or the last F10. Option 9 leaves a job out until F14
//! (Include); F14 and F20 (Subset) choose the jobs shown; F16 sorts by
//! the column under the cursor.

use std::collections::BTreeSet;

use cmd::Resolved;
use mi::Job;
use stg::Client;
use stg::proto::{ActJob, ActJobs};
use uim::model::Action;
use wm::job::wrkactjob_type;

use super::{Flow, common, decimal, entry, parm, refill, render_fail, stamp};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{Choice, Response, UimDialog};

const LIST: &str = "JOBS";

/// Which jobs show and in what order.
#[derive(Default)]
struct Select {
    /// `SBS()` of the command.
    sbs: Vec<String>,
    /// Left out with option 9, by number.
    excluded: BTreeSet<u32>,
    /// The column F16 sorted by.
    sort: Option<&'static str>,
}

/// `WRKACTJOB [RESET(*YES)] [SBS(...)]`.
pub fn wrkactjob(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let mut d = crate::panels::dialog(host, "QWCWRKAJ")?;
    let mut sel = Select {
        sbs: parm(r, "SBS")
            .filter(|s| s != "*ALL")
            .map(|s| s.split_whitespace().map(str::to_string).collect())
            .unwrap_or_default(),
        ..Select::default()
    };
    let wide = surface.wide();
    let mut panel = if wide { "WRKACTJW" } else { "WRKACTJ1" };
    let mut history = Vec::new();
    let mut advance = true;
    loop {
        let data = host.link.borrow_mut().with(|l| l.actjob_data(advance));
        advance = false;
        fill(host, &mut d, &data, &sel);
        loop {
            let resp = d.show(surface, panel).map_err(|e| render_fail(&e))?;
            let flow = match resp {
                Response::Return(10) => {
                    advance = true;
                    Flow::Refresh
                }
                Response::Return(11) if !wide => {
                    d.view = (d.view + 1) % 3;
                    d.set("VIEW", &(d.view + 1).to_string());
                    Flow::Stay
                }
                Response::Return(23) if !wide => {
                    panel = if panel == "WRKACTJ1" { "WRKACTJ2" } else { "WRKACTJ1" };
                    Flow::Stay
                }
                Response::Return(n @ (14 | 20)) => {
                    let which = if n == 14 { "WRKACTJI" } else { "WRKACTJS" };
                    if select(surface, &mut d, which)? {
                        if n == 14 {
                            sel.excluded.clear();
                        }
                        Flow::Refresh
                    } else {
                        Flow::Stay
                    }
                }
                Response::Return(16) => {
                    match column_at(d.view, wide, d.cursor) {
                        Some(c) => {
                            sel.sort = Some(c);
                            d.list_mut(LIST).top = 0;
                        }
                        None => {
                            d.message = Some(("Put the cursor on a column of the list, then F16.".into(), true));
                        }
                    }
                    Flow::Refresh
                }
                Response::Return(19) => {
                    d.message = Some(("Automatic refresh is not available in this release.".into(), true));
                    Flow::Stay
                }
                Response::Return(21) => {
                    d.message = Some(("The instructions and keys always show in this release.".into(), true));
                    Flow::Stay
                }
                Response::Options(choices) => {
                    let (left_out, chosen): (Vec<Choice>, Vec<Choice>) = choices
                        .into_iter()
                        .partition(|c| c.action == Some(Action::Return(9)));
                    for c in &left_out {
                        let number = d
                            .row(&c.list, c.row)
                            .into_iter()
                            .find(|(k, _)| k == "NUMBER")
                            .and_then(|(_, v)| v.parse().ok());
                        if let Some(n) = number {
                            sel.excluded.insert(n);
                        }
                    }
                    d.list_mut(LIST).options.clear();
                    if chosen.is_empty() {
                        Flow::Refresh
                    } else {
                        common(job, host, &mut d, &mut history, Response::Options(chosen))
                    }
                }
                other => common(job, host, &mut d, &mut history, other),
            };
            match flow {
                Flow::Stay => {}
                Flow::Refresh => break,
                Flow::Leave => return Ok(String::new()),
            }
        }
    }
}

/// Show the Include or Subset panel; whether Enter took its values.
fn select(surface: &mut Surface, d: &mut UimDialog, panel: &str) -> Result<bool, Fail> {
    let saved: Vec<String> = SELECTS.iter().map(|v| d.get(v)).collect();
    loop {
        match d.show(surface, panel).map_err(|e| render_fail(&e))? {
            Response::Return(1) => {
                for v in SELECTS {
                    let t = d.get(v).trim().to_ascii_uppercase();
                    d.set(v, if t.is_empty() { "*ALL" } else { &t });
                }
                return Ok(true);
            }
            Response::Return(5) => {
                for v in SELECTS {
                    d.set(v, "*ALL");
                }
            }
            _ => {
                for (v, s) in SELECTS.iter().zip(saved) {
                    d.set(v, &s);
                }
                return Ok(false);
            }
        }
    }
}

/// The selection variables of the Include and Subset panels.
const SELECTS: [&str; 5] = ["SELSBS", "SELJOB", "SELUSER", "SELTYPE", "SELSTS"];

/// A name against a selection: `*ALL`, a name, or `ABC*`.
fn matches(pattern: &str, value: &str) -> bool {
    let p = pattern.trim();
    let v = value.trim();
    if p.is_empty() || p == "*ALL" {
        return true;
    }
    match p.strip_suffix('*') {
        Some(prefix) => v.starts_with(prefix),
        None => p == v,
    }
}

/// The list column at the cursor in a view, for F16.
fn column_at(view: usize, wide: bool, (row, col): (usize, usize)) -> Option<&'static str> {
    // The entries start on row 10 and the list ends above the command line.
    if !(10..=21 + if wide { 3 } else { 0 }).contains(&row) {
        return None;
    }
    let cols: &[(usize, &str)] = match (wide, view) {
        (true, _) => &[
            (7, "JOB"),
            (22, "CURUSER"),
            (34, "NUMBER"),
            (42, "TYPE"),
            (48, "CPU"),
            (55, "FUNCTION"),
            (71, "STATUS"),
            (78, "POOL"),
            (84, "PTY"),
            (89, "CPUSEC"),
            (96, "AUXIO"),
            (103, "THREADS"),
            (112, "TMPSTG"),
        ],
        (false, 0) => &[(7, "JOB"), (22, "CURUSER"), (34, "TYPE"), (40, "CPU"), (47, "FUNCTION"), (63, "STATUS")],
        (false, 1) => &[
            (7, "JOB"),
            (22, "TYPE"),
            (28, "POOL"),
            (34, "PTY"),
            (39, "CPUSEC"),
            (46, "INT"),
            (52, "RSP"),
            (58, "AUXIO"),
            (65, "ECPU"),
        ],
        _ => &[(7, "JOB"), (22, "USER"), (34, "NUMBER"), (42, "TYPE"), (48, "CPU"), (55, "THREADS"), (65, "TMPSTG")],
    };
    cols.iter().rev().find(|(start, _)| col >= *start).map(|(_, v)| *v)
}

/// A job's sort key for a column: numbers by size, the rest by text.
fn key(a: &ActJob, column: &str) -> (u64, String) {
    let j = &a.job;
    match column {
        "CPU" | "ECPU" => (u64::from(a.cpu), String::new()),
        "CPUSEC" => (a.cpu_secs, String::new()),
        "POOL" => (u64::from(a.pool), String::new()),
        "PTY" => (u64::from(a.runpty), String::new()),
        "AUXIO" => (a.aux_io, String::new()),
        "THREADS" => (u64::from(a.threads), String::new()),
        "TMPSTG" => (a.tmp_mb, String::new()),
        "NUMBER" => (u64::from(j.number), String::new()),
        "USER" | "CURUSER" => (0, j.user.clone()),
        "FUNCTION" => (0, j.function.clone()),
        "STATUS" => (0, j.job_status().code().to_string()),
        "TYPE" => (0, j.job_type().map_or(String::new(), |t| wrkactjob_type(t, j.sub_type()).to_string())),
        _ => (0, j.name.clone()),
    }
}

/// The header and the entries.
fn fill(host: &Host, d: &mut UimDialog, data: &ActJobs, sel: &Select) {
    d.set("*DATETIME", &stamp(host));
    d.set("SYSCPU", &decimal(host, u64::from(data.cpu)));
    d.set("ELAPSED", &wm::stats::hms(data.elapsed));
    let want: Vec<String> = SELECTS.iter().map(|v| d.get(v)).collect();
    let [sbs_sel, job_sel, user_sel, type_sel, status_sel] = &want[..] else {
        return;
    };
    let mut shown: Vec<&ActJob> = data
        .jobs
        .iter()
        .filter(|a| {
            let j = &a.job;
            let in_sbs = |s: &str| *s == j.subsystem || (j.kind == 'M' && *s == j.name);
            let kind = j.job_type().map_or("", |t| wrkactjob_type(t, j.sub_type())).trim().to_string();
            (sel.sbs.is_empty() || sel.sbs.iter().any(|s| in_sbs(s)))
                && !sel.excluded.contains(&j.number)
                && (matches(sbs_sel, &j.subsystem) || (j.kind == 'M' && matches(sbs_sel, &j.name)))
                && matches(job_sel, &j.name)
                && matches(user_sel, &j.user)
                && matches(type_sel, &kind)
                && matches(status_sel, j.job_status().code())
        })
        .collect();
    if let Some(col) = sel.sort {
        // Numbers largest first, names in order.
        shown.sort_by(|a, b| {
            let (first, second) = (key(a, col), key(b, col));
            second.0.cmp(&first.0).then(first.1.cmp(&second.1))
        });
    }
    d.set("ACTJOBS", &shown.len().to_string());
    let rows = shown
        .into_iter()
        .map(|a| {
            let j = &a.job;
            let kind = j
                .job_type()
                .map_or("   ", |t| wrkactjob_type(t, j.sub_type()));
            let name = if a.under && sel.sort.is_none() {
                format!("  {}", j.name)
            } else {
                j.name.clone()
            };
            entry(&[
                ("JOB", name),
                ("JOBNM", j.name.clone()),
                ("USER", j.user.clone()),
                ("CURUSER", j.user.clone()),
                ("NUMBER", format!("{:06}", j.number)),
                ("TYPE", kind.to_string()),
                ("CPU", decimal(host, u64::from(a.cpu))),
                ("FUNCTION", j.function.clone()),
                // The status sits one column into its heading.
                ("STATUS", format!(" {}", j.job_status().code())),
                ("POOL", a.pool.to_string()),
                ("PTY", a.runpty.to_string()),
                ("CPUSEC", decimal(host, a.cpu_secs)),
                ("INT", String::new()),
                ("RSP", String::new()),
                ("AUXIO", a.aux_io.to_string()),
                ("ECPU", decimal(host, u64::from(a.cpu))),
                ("THREADS", a.threads.to_string()),
                ("TMPSTG", a.tmp_mb.to_string()),
            ])
        })
        .collect();
    refill(d, LIST, rows);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selections_and_columns() {
        assert!(matches("*ALL", "QBATCH"));
        assert!(matches("QBAT*", "QBATCH"));
        assert!(!matches("QBAT*", "QINTER"));
        assert!(matches("QINTER", "QINTER"));
        assert_eq!(column_at(0, false, (12, 40)), Some("CPU"));
        assert_eq!(column_at(0, false, (12, 3)), None);
        assert_eq!(column_at(2, false, (12, 56)), Some("THREADS"));
        assert_eq!(column_at(0, false, (3, 40)), None);
    }
}
