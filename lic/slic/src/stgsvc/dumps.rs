// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Dumps (`docs/SERVICE.md`): the system dump the main storage dump
//! manager takes (the storage service's state, the job table, the LIC
//! log and the IPL history, kept as user space `QSYS/QSDnnnnnnn`), and
//! the formatted object dumps `DMPOBJ` and `DMPSYSOBJ` print to
//! `QPSRVDMP`. Every dump taken logs a problem.



use prb::PrbType;
use store::Pager;
use store::obj::{ObjectId, ops, types, usrspc};
use store::txn::Txn;
use svctools::DstEnv;

use crate::ipl::system::{System, now_unix};

/// What a dump list shows of one dump.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DumpInfo {
    /// `QSDnnnnnnn`.
    pub name: String,
    /// Unix seconds taken.
    pub taken: u64,
    /// Bytes of text.
    pub size: u64,
    /// Why it was taken.
    pub text: String,
}

fn civil(t: u64) -> String {
    let (y, mo, d, hh, mm, ss) = crate::platform::rtc::unix_to_civil(t);
    format!("{y:04}-{mo:02}-{d:02} {hh:02}:{mm:02}:{ss:02}")
}

pub(crate) fn with_store<T>(
    system: &mut System,
    f: impl FnOnce(&mut Txn<'_>, ObjectId) -> Result<T, String>,
) -> Result<T, String> {
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    crate::svc::db::with_txn(&mut s.store, |txn| f(txn, qsys))
}

fn is_dump_name(n: &str) -> bool {
    n.len() == 10 && n.starts_with("QSD") && n[3..].bytes().all(|b| b.is_ascii_digit())
}

/// The system dumps, oldest first.
pub fn list(system: &mut System) -> Result<Vec<DumpInfo>, String> {
    with_store(system, |txn, qsys| {
        let mut out = Vec::new();
        for (key, id) in ops::list(txn, qsys, Some(types::USRSPC)).map_err(|e| e.to_string())? {
            let name = key.name.to_string().trim().to_string();
            if !is_dump_name(&name) {
                continue;
            }
            let h = ops::describe(txn, id).map_err(|e| e.to_string())?;
            let len = usrspc::read(txn, id, 0, 8).map_err(|e| e.to_string())?;
            out.push(DumpInfo {
                name,
                taken: h.created,
                size: u64::from_be_bytes(len.try_into().expect("8")),
                text: ccsid::from_ebcdic(&h.text).trim_end().to_string(),
            });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    })
}

/// The lines of a system dump.
pub fn show(system: &mut System, name: &str) -> Result<Vec<String>, String> {
    let name = name.trim().to_ascii_uppercase();
    if !is_dump_name(&name) {
        return Err(format!("CPF39D2 System dump {name} not found."));
    }
    let text = super::prblog::load_text(system, "QSYS", &name)?
        .ok_or_else(|| format!("CPF39D2 System dump {name} not found."))?;
    Ok(text.lines().map(str::to_string).collect())
}

/// Delete a system dump.
pub fn delete(system: &mut System, name: &str) -> Result<(), String> {
    let name = name.trim().to_ascii_uppercase();
    if !is_dump_name(&name) {
        return Err(format!("CPF39D2 System dump {name} not found."));
    }
    let job = system.current_job;
    with_store(system, |txn, qsys| {
        let n = qsname::Name::parse(&name).map_err(|e| e.to_string())?;
        let l = store::obj::library::Library::load(txn, qsys).map_err(|e| e.to_string())?;
        let id = l
            .lookup(txn, types::USRSPC, n)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("CPF39D2 System dump {name} not found."))?;
        sec::guard::as_system(|| ops::delete(txn, store::obj::JobHandle(u64::from(job)), id))
            .map_err(|e| e.to_string())
    })?;
    super::liclog::write(0x0500, 0x0002, job, &name);
    Ok(())
}

/// Take a system dump; its name. Logs a problem.
pub fn take(system: &mut System, why: &str) -> Result<String, String> {
    let next = list(system)?
        .last()
        .and_then(|d| d.name[3..].parse::<u32>().ok())
        .map_or(1, |n| n + 1);
    let name = format!("QSD{next:07}");
    let lines = contents(system, &name, why);
    let text = lines.join("\n") + "\n";
    let why50: String = why.chars().take(50).collect();
    sec::guard::as_system(|| super::prblog::store_text(system, "QSYS", &name, &text, &why50))?;
    super::liclog::write(0x0500, 0x0001, system.current_job, &format!("{name}: {why}"));
    super::trace::internal("DMP", system.current_job, &name);
    let _ = super::prblog::log(system, PrbType::Machine, &prb::analyze::from_dump(&name, why));
    Ok(name)
}

/// What a system dump holds.
fn contents(system: &mut System, name: &str, why: &str) -> Vec<String> {
    let now = now_unix();
    let mut out = vec![
        format!("System dump {name}"),
        format!("System . . . . . . : {}", system.system_name()),
        format!("Taken  . . . . . . : {}", civil(now)),
        format!("Requested by job . : {}", job_name(system, system.current_job)),
        format!("Reason . . . . . . : {why}"),
        format!("Release  . . . . . : {}", release::banner()),
        String::new(),
        "Storage service: disk units".into(),
    ];
    {
        let mut e = crate::ipl::dst::Env::new(system);
        for u in e.disk_units() {
            out.push(format!(
                "  ASP {} unit {:>3} {:<8} {:>8} MiB {:>3}% used  {}",
                u.asp, u.unit, u.resource, u.size_mib, u.percent_used, u.status
            ));
        }
    }
    out.push(String::new());
    out.push("Job table".into());
    out.push("  Number Name       User       Type Status Function".into());
    for j in system.jobs.all() {
        out.push(format!(
            "  {:06} {:<10} {:<10} {:<4} {:>6} {}",
            j.number, j.name, j.user, j.kind, j.status, j.function
        ));
    }
    out.push(String::new());
    out.push("IPL history".into());
    if let Some((_, record)) = system.lic.as_ref() {
        for h in &record.history {
            out.push(format!(
                "  {}  {:?} {:?} slot {:?} {} {:?}",
                civil(h.start_time),
                h.ipl_type,
                h.mode,
                h.slot,
                h.final_src_str(),
                h.outcome
            ));
        }
    }
    out.push(String::new());
    out.push("Licensed Internal Code log".into());
    for e in super::liclog::entries(0) {
        out.push(format!("  {}", super::liclog::line(&e)));
    }
    out.push(String::new());
    out.push("Traces".into());
    for (n, k, active, records) in super::trace::list() {
        out.push(format!("  {n:<10} {k:?} {} {records} records", if active { "active" } else { "ended" }));
    }
    out
}

/// A job's qualified name, `number/user/name`.
pub fn job_name(system: &System, number: u32) -> String {
    if number == 0 {
        return "*SYSTEM".into();
    }
    system.jobs.get(number).map_or_else(
        || format!("{number:06}"),
        |j| format!("{:06}/{}/{}", j.number, j.user, j.name),
    )
}

/// Hex lines: offset, sixteen bytes, the EBCDIC text.
pub fn hex(bytes: &[u8], base: usize) -> Vec<String> {
    bytes
        .chunks(16)
        .enumerate()
        .map(|(i, c)| {
            let h: Vec<String> = c.iter().map(|b| format!("{b:02X}")).collect();
            let t: String = ccsid::from_ebcdic(c)
                .chars()
                .map(|ch| if ch.is_control() { '.' } else { ch })
                .collect();
            format!("{:08X}  {:<47}  *{t}*", base + i * 16, h.join(" "))
        })
        .collect()
}

/// The pages a formatted dump shows at most.
const DUMP_PAGES: usize = 8;

/// A formatted dump of object `id`: header fields, the header page and
/// the first payload pages in hex.
pub fn object_lines(txn: &mut Txn<'_>, id: ObjectId, title: &str) -> Result<Vec<String>, String> {
    let h = ops::describe(txn, id).map_err(|e| e.to_string())?;
    let ty = types::type_by_code(h.id.type_code).map_or("*UNKNOWN", |t| t.name);
    let txt = |b: &[u8]| ccsid::from_ebcdic(b).trim_end().to_string();
    let mut out = vec![
        title.to_string(),
        String::new(),
        format!("Object . . . . . . : {}", h.name.to_string().trim()),
        format!("Type . . . . . . . : {ty}  (X'{:04X}')", h.id.type_code),
        format!("Object identifier  : {}", h.id),
        format!("Library identifier : {}", h.library),
        format!("Attribute  . . . . : {}", txt(&h.attribute)),
        format!("Text . . . . . . . : {}", txt(&h.text)),
        format!("Owner  . . . . . . : {}", txt(&h.owner)),
        format!("Created  . . . . . : {}", civil(h.created)),
        format!("Changed  . . . . . : {}", civil(h.changed)),
        String::new(),
    ];
    let header = store::obj::dir::header_page(txn, id).map_err(|e| e.to_string())?;
    let page = txn.read(header).map_err(|e| e.to_string())?;
    out.push(format!("Header page {header}"));
    out.extend(hex(page.bytes(), 0));
    let runs = ops::all_extents(txn, &h).map_err(|e| e.to_string())?;
    let mut shown = 0;
    'runs: for r in runs {
        for p in r.start..r.start + r.len {
            if shown == DUMP_PAGES {
                out.push(format!("(more pages follow; {DUMP_PAGES} shown)"));
                break 'runs;
            }
            let page = txn.read(p).map_err(|e| e.to_string())?;
            out.push(String::new());
            out.push(format!("Payload page {p}"));
            out.extend(hex(page.body(), 0));
            shown += 1;
        }
    }
    Ok(out)
}

/// Alter `bytes` at `offset` in the body of page `page`, which must
/// belong to object `id` (its header page or one of its extents).
pub fn alter(system: &mut System, id: ObjectId, page: u64, offset: usize, bytes: &[u8]) -> Result<(), String> {
    let job = system.current_job;
    with_store(system, |txn, _| {
        let h = ops::describe(txn, id).map_err(|e| e.to_string())?;
        let header = store::obj::dir::header_page(txn, id).map_err(|e| e.to_string())?;
        let runs = ops::all_extents(txn, &h).map_err(|e| e.to_string())?;
        let owned = page == header || runs.iter().any(|r| (r.start..r.start + r.len).contains(&page));
        if !owned {
            return Err(format!("Page {page} does not belong to object {id}"));
        }
        let mut p = txn.read(page).map_err(|e| e.to_string())?;
        let tr = p.trailer();
        let end = offset.checked_add(bytes.len()).filter(|e| *e <= store::BODY_SIZE);
        let Some(end) = end else {
            return Err(format!("Offset {offset} and length {} are past the page body", bytes.len()));
        };
        p.body_mut()[offset..end].copy_from_slice(bytes);
        let kind = store::PageType::from_u16(tr.page_type).ok_or("page type not known")?;
        txn.write(page, &mut p, kind, tr.owner).map_err(|e| e.to_string())
    })?;
    super::liclog::write(0x0700, 0x0002, job, &format!("object {id} page {page} offset {offset} {} bytes", bytes.len()));
    Ok(())
}
