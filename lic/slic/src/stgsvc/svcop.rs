// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `service_op` (request 1400): the service actions a job's panels ask
//! for. `prb-*` work on the problem log (`ANZPRB`, `WRKPRB` options 8
//! and 12), `trace-point` records a job's own trace point, and `sst-*`
//! are SST's newer functions, each needing the signed-on service tools
//! user's privilege as the older SST requests do.

use prb::analyze::{Answers, Area};
use prb::{PrbStatus, PrbType, Reported};
use stg::JobInfo;
use svctools::{DstEnv, Privilege};

use super::{dumps, liclog, prblog, trace};
use crate::ipl::system::{System, now_unix};

fn arg(args: &[String], i: usize) -> &str {
    args.get(i).map_or("", |s| s.trim())
}

/// Run `op`.
pub fn run(system: &mut System, me: &JobInfo, op: &str, args: &[String]) -> Result<Vec<String>, String> {
    let job = me.number;
    if let Some(sst) = op.strip_prefix("sst-") {
        return sst_op(system, job, sst, args);
    }
    match op {
        "prb-analyze" => analyze(system, args),
        "prb-ready" => change(system, arg(args, 0), |l, id, now| {
            l.analyzed(id, now);
            Ok(format!("Problem {id} is ready to report."))
        }),
        "prb-close" => change(system, arg(args, 0), |l, id, now| {
            let p = l.get_mut(id).expect("there");
            p.status = PrbStatus::Closed;
            p.changed = now;
            Ok(format!("Problem {id} closed."))
        }),
        "prb-note" => {
            let text = arg(args, 1).to_string();
            change(system, arg(args, 0), move |l, id, now| {
                if text.is_empty() {
                    return Err("Type the text of the note.".into());
                }
                let p = l.get_mut(id).expect("there");
                p.notes.push(text);
                p.changed = now;
                Ok(format!("Note added to problem {id}."))
            })
        }
        "prb-report" => {
            let id = arg(args, 0).to_string();
            Ok(vec![match prblog::report(system, &id)? {
                Reported::Sent(r) => format!("Problem {id} sent to the service provider: {r}."),
                Reported::Kept(why) => {
                    format!("Problem {id} prepared; the report is kept: {why}.")
                }
            }])
        }
        "prb-dump" => {
            let d = prb::analyze::from_dump(arg(args, 0), arg(args, 1));
            liclog::write(0x0500, 0x0003, job, arg(args, 0));
            Ok(vec![prblog::log(system, PrbType::Machine, &d)?])
        }
        "trace-point" => {
            let category = trace::CATEGORIES
                .iter()
                .find(|c| c.0 == arg(args, 0))
                .map_or("REQ", |c| c.0);
            trace::job(job, category, arg(args, 1));
            Ok(Vec::new())
        }
        other => Err(format!("CPF9899 no service action called {other}")),
    }
}

/// `ANZPRB`'s answers: area number, message ID, object, stops work
/// (`Y`), description. The problem is logged, analyzed and ready.
fn analyze(system: &mut System, args: &[String]) -> Result<Vec<String>, String> {
    let area = arg(args, 0)
        .parse()
        .ok()
        .and_then(Area::from_choice)
        .ok_or("Select one of the areas, 1 to 5.")?;
    let a = Answers {
        area,
        msgid: arg(args, 1).to_ascii_uppercase(),
        object: arg(args, 2).to_ascii_uppercase(),
        stops_work: arg(args, 3).eq_ignore_ascii_case("Y"),
        description: arg(args, 4).to_string(),
    };
    if a.description.is_empty() {
        return Err("Describe the problem in a few words.".into());
    }
    let d = prb::analyze::guided(&a);
    let id = prblog::log(system, PrbType::User, &d)?;
    let mut l = prblog::load(system)?;
    l.analyzed(&id, now_unix());
    prblog::store(system, &l)?;
    Ok(vec![id, d.symptoms])
}

fn change(
    system: &mut System,
    id: &str,
    f: impl FnOnce(&mut prb::ProblemLog, &str, u64) -> Result<String, String>,
) -> Result<Vec<String>, String> {
    let mut l = prblog::load(system)?;
    if l.get(id).is_none() {
        return Err(format!("CPF3925 Problem {id} not found in the problem log."));
    }
    let msg = f(&mut l, id, now_unix())?;
    prblog::store(system, &l)?;
    Ok(vec![msg])
}

fn hex_bytes(s: &str) -> Result<Vec<u8>, String> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if s.is_empty() || !s.len().is_multiple_of(2) {
        return Err("Type the new bytes as pairs of hex digits".into());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| format!("{} is not hex", &s[i..i + 2])))
        .collect()
}

/// SST's newer requests.
#[allow(clippy::too_many_lines)] // one arm a request
fn sst_op(system: &mut System, job: u32, op: &str, args: &[String]) -> Result<Vec<String>, String> {
    match op {
        "capacity" => {
            super::sst::need(system, job, Privilege::NONE)?;
            Ok(capacity(system))
        }
        "dump-take" => {
            super::sst::need(system, job, Privilege::DUMPS_AND_TRACES)?;
            let why = if arg(args, 0).is_empty() { "Taken from the service tools" } else { arg(args, 0) };
            Ok(vec![dumps::take(system, why)?])
        }
        "dump-list" => {
            super::sst::need(system, job, Privilege::DUMPS_AND_TRACES)?;
            Ok(dumps::list(system)?
                .into_iter()
                .map(|d| format!("{}\t{}\t{}\t{}", d.name, d.taken, d.size, d.text))
                .collect())
        }
        "dump-show" => {
            super::sst::need(system, job, Privilege::DUMPS_AND_TRACES)?;
            dumps::show(system, arg(args, 0))
        }
        "dump-delete" => {
            super::sst::need(system, job, Privilege::DUMPS_AND_TRACES)?;
            dumps::delete(system, arg(args, 0))?;
            Ok(vec![format!("System dump {} deleted.", arg(args, 0))])
        }
        "dump-copy" => {
            super::sst::need(system, job, Privilege::DUMPS_AND_TRACES)?;
            Err("Copying a dump to media is not available in this release".into())
        }
        "trace-start" => {
            super::sst::need(system, job, Privilege::DUMPS_AND_TRACES)?;
            let name = arg(args, 0).to_ascii_uppercase();
            if name.is_empty() || name.len() > 10 {
                return Err("Type a trace table name of 1 to 10 characters".into());
            }
            let jobs = match arg(args, 2).trim_start_matches('0') {
                "" => Vec::new(),
                n => vec![n.parse().map_err(|_| format!("{n} is not a job number"))?],
            };
            let size = arg(args, 3).parse().unwrap_or(1024);
            let mut t = trace::Trace::new(&name, trace::Kind::Internal, jobs, trace::Detail::All, size, arg(args, 4) != "N");
            let cats = arg(args, 1).to_ascii_uppercase();
            if !cats.is_empty() && cats != "*ALL" {
                for c in cats.split_whitespace() {
                    if !trace::CATEGORIES.iter().any(|k| k.0 == c) {
                        return Err(format!("{c} is not a trace category"));
                    }
                    t.categories.push(c.to_string());
                }
            }
            trace::start(t).map_err(strip_id)?;
            Ok(vec![format!("Trace {name} started.")])
        }
        "trace-stop" => {
            super::sst::need(system, job, Privilege::DUMPS_AND_TRACES)?;
            let n = trace::end(arg(args, 0)).map_err(strip_id)?;
            Ok(vec![format!("Trace {} stopped with {n} records.", arg(args, 0))])
        }
        "trace-list" => {
            super::sst::need(system, job, Privilege::DUMPS_AND_TRACES)?;
            Ok(trace::list()
                .into_iter()
                .map(|(n, k, active, records)| {
                    format!(
                        "{n}\t{}\t{}\t{records}",
                        if k == trace::Kind::Job { "Job" } else { "LIC" },
                        if active { "Active" } else { "Stopped" }
                    )
                })
                .collect())
        }
        "trace-show" => {
            super::sst::need(system, job, Privilege::DUMPS_AND_TRACES)?;
            let t = trace::get(arg(args, 0)).ok_or_else(|| format!("Trace {} not found", arg(args, 0)))?;
            Ok(trace::report(&t, &|n| dumps::job_name(system, n)))
        }
        "trace-delete" => {
            super::sst::need(system, job, Privilege::DUMPS_AND_TRACES)?;
            trace::delete(arg(args, 0)).ok_or_else(|| format!("Trace {} not found", arg(args, 0)))?;
            Ok(vec![format!("Trace {} deleted.", arg(args, 0))])
        }
        "liclog" => {
            super::sst::need(system, job, Privilege::LOGS)?;
            let major = u16::from_str_radix(arg(args, 0), 16)
                .ok()
                .filter(|m| liclog::MAJORS.iter().any(|k| k.0 == *m))
                .unwrap_or(0);
            Ok(liclog::entries(major).iter().map(liclog::line).collect())
        }
        "dad-print" => {
            super::sst::need(system, job, Privilege::DISPLAY_ALTER_DUMP)?;
            let (title, bytes) = storage(system, args)?;
            let mut lines = vec![title];
            lines.extend(dumps::hex(&bytes, 0));
            let r = crate::svc::security::print(system, "QPCSMPRT", &lines)?;
            Ok(vec![r.trim_end().to_string()])
        }
        "dad-alter" => {
            let by = super::sst::need(system, job, Privilege::DISPLAY_ALTER_DUMP)?;
            super::security::require(sec::profile::SpecialAuth::SERVICE, "alter storage")?;
            let id: store::obj::ObjectId = arg(args, 0).parse()?;
            let page: u64 = arg(args, 1).parse().map_err(|_| "Type the page number".to_string())?;
            let offset = usize::from_str_radix(arg(args, 2), 16).map_err(|_| "Type the offset in hex".to_string())?;
            let bytes = hex_bytes(arg(args, 3))?;
            dumps::alter(system, id, page, offset, &bytes)?;
            eprintln!("qsstg: SST storage altered by {by}: {id} page {page} offset {offset:X}");
            Ok(vec![format!("{} bytes altered.", bytes.len())])
        }
        "hsm-print" => {
            super::sst::need(system, job, Privilege::HARDWARE)?;
            let rows = crate::ipl::dst::Env::new(system).resources();
            let mut lines = vec![
                "Hardware Service Manager: system configuration".to_string(),
                String::new(),
                "Resource  Type      Status        Location          Description".into(),
            ];
            for r in rows {
                lines.push(format!(
                    "{:<9} {:<9} {:<13} {:<17} {}",
                    r.name, r.kind, r.status, r.location, r.description
                ));
            }
            let r = crate::svc::security::print(system, "QPCSMPRT", &lines)?;
            Ok(vec![r.trim_end().to_string()])
        }
        "devices" => {
            super::sst::need(system, job, Privilege::USER_IDS)?;
            Ok(vec!["QCONSOLE\tEnabled\tThe system console".into()])
        }
        other => Err(format!("CPF9899 no service tools action called {other}")),
    }
}

/// A message without its leading identifier.
fn strip_id(e: String) -> String {
    match crate::cpp::leading_msgid(&e) {
        Some(_) => e[7..].trim().to_string(),
        None => e,
    }
}

/// Display/Alter/Dump's unit, page and object: the title and bytes.
fn storage(system: &mut System, args: &[String]) -> Result<(String, Vec<u8>), String> {
    let mut e = crate::ipl::dst::Env::new(system);
    let id = arg(args, 2);
    if id.is_empty() {
        let u: u16 = arg(args, 0).parse().map_err(|_| "Type a unit number".to_string())?;
        let p: u64 = arg(args, 1).parse().map_err(|_| "Type a page number".to_string())?;
        let u = u.checked_sub(1).ok_or("Unit numbers start at 1")?;
        Ok((format!("Unit {} page {p}", u + 1), e.read_page(u, p)?))
    } else {
        Ok((format!("Object header {id}"), e.object_header(id)?))
    }
}

/// `Work with System Capacity`'s figures: label and value.
fn capacity(system: &mut System) -> Vec<String> {
    let (vars, _) = super::stats::syssts_panel(system, false);
    let v = |k: &str| vars.iter().find(|(n, _)| n == k).map_or_else(String::new, |(_, x)| x.clone());
    let cpus = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let units = crate::ipl::dst::Env::new(system).disk_units();
    let mib: u64 = units.iter().map(|u| u.size_mib).sum();
    let threshold = crate::ipl::dst::Env::new(system).asp_threshold();
    let pools = system.pools.info().len();
    vec![
        format!("Installed processors\t{cpus}"),
        format!("Active processors\t{cpus}"),
        format!("Processor use, %\t{}", v("CPUPCT")),
        format!("Main storage, MB\t{}", v("MAINSTG")),
        format!("Storage pools\t{pools}"),
        format!("Disk units configured\t{}", units.len()),
        format!("System ASP capacity\t{} G", mib / 1024),
        format!("System ASP used, %\t{}", v("ASPPCT")),
        format!("ASP threshold, %\t{threshold}"),
        format!("Active jobs\t{}", v("JOBS")),
    ]
}
