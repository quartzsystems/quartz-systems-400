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

/// `mm/dd/yy` and `hh:mm:ss` of a Unix time.
fn date_time(t: u64) -> (String, String) {
    let (y, mo, d, hh, mm, ss) = crate::platform::rtc::unix_to_civil(t);
    (
        format!("{mo:02}/{d:02}/{:02}", y.rem_euclid(100)),
        format!("{hh:02}:{mm:02}:{ss:02}"),
    )
}

/// The reference code and resource of a problem, as the report shows.
fn refcode(p: &Problem) -> (String, String) {
    match &p.origin {
        prb::Origin::Hardware { resource, src } => (format!("SRC{src}"), resource.clone()),
        prb::Origin::Message(m) => (m.clone(), String::new()),
        prb::Origin::Dump(_) => ("DUMP".into(), String::new()),
        prb::Origin::Analysis => ("ANZPRB".into(), String::new()),
    }
}

/// `DSPPRB OUTPUT(*PRINT)`: the problem log report.
fn report(system: &mut System, probs: &[&Problem]) -> Result<String, String> {
    let mut lines = vec![
        "                            System                           Resource    Service  Problem".to_string(),
        " Problem ID  Status         Ref Code     Date      Time      Name        Number   Description".into(),
    ];
    for p in probs {
        let (d, t) = date_time(p.opened);
        let (code, rsrc) = refcode(p);
        let mut text = p.text.clone();
        text.truncate(text.char_indices().nth(36).map_or(text.len(), |(i, _)| i));
        lines.push(format!(
            " {:<11} {:<14} {:<12} {d}  {t}  {rsrc:<11} {:<8} {text}",
            p.id,
            p.status.display(),
            code,
            ""
        ));
    }
    crate::svc::security::print(system, "QPSXPRT", &lines)
}

/// The variables `DSPPRB`'s panel shows of one problem, and its notes.
pub fn detail_data(system: &mut System, id: &str) -> Result<stg::proto::PanelData, String> {
    let log = prblog::load(system)?;
    let p = log
        .get(id)
        .ok_or_else(|| format!("CPF3925 Problem {id} not found in the problem log."))?;
    let (od, ot) = date_time(p.opened);
    let (cd, ct) = date_time(p.changed);
    let (code, rsrc) = refcode(p);
    let pair = |k: &str, v: String| (k.to_string(), v);
    let vars = vec![
        pair("PRBID", p.id.clone()),
        pair("PRBSTS", p.status.display().to_string()),
        pair("PRBTYPE", p.kind.number().to_string()),
        pair("PRBSEV", p.severity.to_string()),
        pair("PRBTEXT", p.text.clone()),
        pair("PRBREF", code),
        pair("PRBRSRC", if rsrc.is_empty() { "*NONE".into() } else { rsrc }),
        pair("PRBSYS", p.system.clone()),
        pair("PRBODATE", od),
        pair("PRBOTIME", ot),
        pair("PRBCDATE", cd),
        pair("PRBCTIME", ct),
        pair("PRBASGN", if p.assigned_to.is_empty() { "*NONE".into() } else { p.assigned_to.clone() }),
        pair("PRBSYMP", p.symptoms.clone()),
        pair("PRBRPT", if log.reports.contains_key(id) { "*YES".into() } else { "*NO".into() }),
    ];
    let rows = p
        .notes
        .iter()
        .map(|n| vec![("NOTE".to_string(), n.clone())])
        .collect();
    Ok((vars, rows))
}

fn list(system: &mut System, parms: &Parms, details: bool) -> Result<String, String> {
    let log = prblog::load(system)?;
    let f = Filter {
        status: statuses(parms),
        ..Filter::default()
    };
    let id = id_of(parms);
    if details && parms.name("OUTPUT") == "*PRINT" {
        let probs: Vec<&Problem> = log
            .list(&f)
            .into_iter()
            .filter(|p| id.as_ref().is_none_or(|i| *i == p.id))
            .collect();
        return report(system, &probs);
    }
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
