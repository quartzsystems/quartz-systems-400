// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DSPSYSVAL` and `WRKSYSVAL` over the `QUISYSVAL` panel group.

use cmd::Resolved;
use mi::Job;
use stg::Client;

use super::{Next, display, parm, work_answer};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;

/// The label of a value's row, for the values that have their own.
fn label(name: &str) -> &'static str {
    match name {
        "QMODEL" => "Model number",
        "QSRLNBR" => "Serial number",
        "QSECURITY" => "Security level",
        "QDATE" => "Date",
        "QTIME" => "Time",
        "QCCSID" => "Coded character set ID",
        "QSYSLIBL" | "QUSRLIBL" => "Libraries",
        "QCTLSBSD" => "Controlling subsystem",
        "QSTRUPPGM" => "Startup program",
        _ => "Value",
    }
}

/// Words packed into rows of at most `width`, `gap` blanks apart.
fn pack(words: &[String], width: usize, gap: usize) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();
    let mut row = String::new();
    for w in words {
        let w = w.trim_end();
        if row.is_empty() {
            row = w.to_string();
        } else if row.chars().count() + gap + w.chars().count() <= width {
            row.push_str(&" ".repeat(gap));
            row.push_str(w);
        } else {
            rows.push(std::mem::take(&mut row));
            row = w.to_string();
        }
    }
    if !row.is_empty() {
        rows.push(row);
    }
    rows
}

/// `DSPSYSVAL SYSVAL()`.
pub fn dspsysval(host: &Host, surface: &mut Surface, r: &Resolved) -> Option<Result<String, Fail>> {
    let name = parm(r, "SYSVAL");
    let (text, _, elems) = host
        .link
        .borrow_mut()
        .with(|l| l.sysval_panel(name.clone()))?;
    let mut d = match crate::panels::dialog(host, "QUISYSVAL") {
        Ok(d) => d,
        Err(e) => return Some(Err(e)),
    };
    d.set("SVNAME", &name);
    let words: Vec<String> = text.split_whitespace().map(str::to_string).collect();
    let desc = pack(&words, 51, 1);
    d.set("SVTEXT", desc.first().map_or("", String::as_str));
    d.set("SVTEXT2", desc.get(1).map_or("", String::as_str));
    d.set("SVLABEL", label(&name));
    let rows = pack(&elems, 51, 2);
    for i in 0..4 {
        d.set(&format!("SVVAL{}", i + 1), rows.get(i).map_or("", String::as_str));
    }
    Some(display(surface, &mut d, "DSPSYSVAL"))
}

/// `WRKSYSVAL SYSVAL()`: every value, a type (`*SEC`) or a generic name.
pub fn wrksysval(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
) -> Option<Result<String, Fail>> {
    let which = parm(r, "SYSVAL");
    let mut d = match crate::panels::dialog(host, "QUISYSVAL") {
        Ok(d) => d,
        Err(e) => return Some(Err(e)),
    };
    let subset = if which.is_empty() || which == "*ALL" || !which.starts_with('*') {
        "*ALL".to_string()
    } else {
        which.clone()
    };
    d.set("SUBSET", &subset);
    loop {
        let all = host.link.borrow_mut().with(Client::sysval_panel_list);
        let top = d.list_mut("SYSVALS").top;
        d.clear_list("SYSVALS");
        for (name, group, text) in all {
            let keep = if which.is_empty() || which == "*ALL" {
                true
            } else if let Some(prefix) = which.strip_suffix('*') {
                name.starts_with(prefix)
            } else if which.starts_with('*') {
                group == which
            } else {
                name == which
            };
            if keep {
                d.add_row(
                    "SYSVALS",
                    vec![
                        ("OPT".into(), String::new()),
                        ("SVNAME".into(), name),
                        ("SVGROUP".into(), group),
                        ("SVDESC".into(), text),
                    ],
                );
            }
        }
        let l = d.list_mut("SYSVALS");
        l.complete = true;
        l.top = top.min(l.rows.len().saturating_sub(1));
        loop {
            let answer = match d.show(surface, "WRKSYSVAL") {
                Ok(a) => a,
                Err(e) => return Some(Err(crate::panels::failed(e.to_string()))),
            };
            // Option 2 changes the value on its change panel.
            let answer = crate::panels::system2::change_options(job, host, surface, &d, answer);
            match work_answer(job, host, &mut d, "SYSVALS", answer) {
                Next::Stay => {}
                Next::Refresh => break,
                Next::Leave => return Some(Ok(String::new())),
            }
        }
    }
}
