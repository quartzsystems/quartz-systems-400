// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `rla`: the scripted record-level driver of the `sql` crate, run from
//! an xfer buffer or from lines typed until `end`.

use super::{Mode, db};
use crate::ipl::system::System;

/// `rla [NAME]`.
pub fn command(system: &mut System, args: &[&str]) -> String {
    match args {
        [] => {
            system.svc.mode = Mode::Rla { lines: Vec::new() };
            "RLA script: one operation per line, 'end' runs it.\n".to_string()
        }
        [name] => match system.svc.xfer.get(*name).cloned() {
            Some(text) => run(
                system,
                &text.lines().map(str::to_string).collect::<Vec<_>>(),
            ),
            None => format!("{name}: no such buffer; xfer put it first\n"),
        },
        _ => "Usage: rla [NAME]\n".to_string(),
    }
}

/// Run script lines in one transaction; the report.
pub fn run(system: &mut System, lines: &[String]) -> String {
    let (store, ctx) = match db(system) {
        Ok(x) => x,
        Err(e) => return format!("{e}\n"),
    };
    let mut txn = match store.begin() {
        Ok(t) => t,
        Err(e) => return format!("{e}\n"),
    };
    let report = sql::rla::run_script(&mut txn, ctx, lines);
    match txn.commit() {
        Ok(()) => report.text,
        Err(e) => format!("{}commit failed: {e}\n", report.text),
    }
}
