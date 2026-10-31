// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The problem log's commands: `WRKPRB`'s text form, `DSPPRB`, `CHGPRB`,
//! `DLTPRB`.

use std::fmt::Write as _;

use cmd::block::Parms;
use prb::{Filter, PrbStatus, Problem};
use stg::proto::CmdReply;

use super::direct::failed;
use crate::ipl::system::{System, now_unix};
use crate::stgsvc::prblog;

/// Run `name`, or `None` when it is not one of these.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    let r = match name {
        "WRKPRB" | "DSPPRB" => list(system, parms, name == "DSPPRB"),
        "CHGPRB" => change(system, parms),
        "DLTPRB" => delete(system, parms),
        _ => return None,
    };
    Some(match r {
        Ok(text) => CmdReply {
            text,
            ..CmdReply::default()
        },
        Err(e) => failed(name, &e),
    })
}

/// The statuses a `STATUS` list names; empty for `*ALL`.
fn statuses(parms: &Parms) -> Vec<PrbStatus> {
    parms
        .list("STATUS")
        .iter()
        .filter_map(|v| PrbStatus::parse(&v.source()))
        .collect()
}

fn id_of(parms: &Parms) -> Option<String> {
    let id = parms.text("PRBID");
    (!id.is_empty() && !id.starts_with('*')).then_some(id)
}

/// The rows `WRKPRB` shows: identifier, status, text.
pub fn rows(system: &mut System, status: &[PrbStatus]) -> Result<Vec<Vec<String>>, String> {
    let log = prblog::load(system)?;
    let f = Filter {
        status: status.to_vec(),
        ..Filter::default()
    };
    Ok(log
        .list(&f)
        .into_iter()
        .map(|p| vec![p.id.clone(), p.status.display().to_string(), p.text.clone()])
        .collect())
}

fn detail(out: &mut String, p: &Problem) {
    let _ = writeln!(out, "Problem ID {}  status {}  type {}  severity {}", p.id, p.status.display(), p.kind.number(), p.severity);
    let _ = writeln!(out, "  {}", p.text);
    let _ = writeln!(out, "  symptoms {}", p.symptoms);
    let _ = writeln!(out, "  system {}  opened {}  changed {}", p.system, p.opened, p.changed);
    if !p.assigned_to.is_empty() {
        let _ = writeln!(out, "  assigned to {}", p.assigned_to);
    }
    for n in &p.notes {
        let _ = writeln!(out, "  note: {n}");
    }
}

fn list(system: &mut System, parms: &Parms, details: bool) -> Result<String, String> {
    let log = prblog::load(system)?;
    let f = Filter {
        status: statuses(parms),
        ..Filter::default()
    };
    let id = id_of(parms);
    let mut out = String::new();
    for p in log.list(&f).into_iter().filter(|p| id.as_ref().is_none_or(|i| *i == p.id)) {
        if details {
            detail(&mut out, p);
        } else {
            let _ = writeln!(out, "{}  {:<10} {}", p.id, p.status.display(), p.text);
        }
    }
    if out.is_empty() {
        out.push_str("No problems.\n");
    }
    Ok(out)
}

fn change(system: &mut System, parms: &Parms) -> Result<String, String> {
    let id = id_of(parms).ok_or("CPD0071 PRBID is required")?;
    let mut log = prblog::load(system)?;
    let p = log
        .get_mut(&id)
        .ok_or_else(|| format!("CPF3925 problem {id} not found"))?;
    if let Some(sev) = parms.int("SEV").and_then(|s| u8::try_from(s).ok()).filter(|s| (1..=4).contains(s)) {
        p.severity = sev;
    }
    let to = parms.text("ASSIGNTO");
    if !to.is_empty() && to != "*SAME" {
        p.assigned_to = to;
    }
    let note = parms.text("TEXT");
    if !note.is_empty() && note != "*SAME" {
        p.notes.push(note);
    }
    if let Some(s) = PrbStatus::parse(&parms.text("STATUS")) {
        p.status = s;
    }
    p.changed = now_unix();
    prblog::store(system, &log)?;
    Ok(format!("Problem {id} changed.\n"))
}

fn delete(system: &mut System, parms: &Parms) -> Result<String, String> {
    let mut log = prblog::load(system)?;
    let n = if let Some(id) = id_of(parms) {
        usize::from(log.problems.remove(&id).is_some())
    } else {
        let days = parms.int("DAYS").and_then(|d| u64::try_from(d).ok()).unwrap_or(30);
        let f = Filter {
            status: statuses(parms),
            before: Some(now_unix().saturating_sub(days * 86_400)),
            ..Filter::default()
        };
        log.delete(&f)
    };
    prblog::store(system, &log)?;
    Ok(format!("{n} problems deleted.\n"))
}
