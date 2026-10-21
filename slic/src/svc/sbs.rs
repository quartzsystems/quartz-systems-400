// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Subsystem commands of the service console: descriptions and their
//! entries, `DSPSBSD`, `WRKSBS`, `WRKSBSJOB`, `ENDSBS`, and the shared
//! pools. `STRSBS` runs in the job, which starts the monitor.

use std::fmt::Write as _;

use store::obj::{JobHandle, ops, types};
use wm::cmd::Params;
use wm::obj::params::qualified;
use wm::obj::sbsd::Sbsd;
use wm::obj::sbsdcmd;

use crate::ipl::system::{System, now_unix};
use crate::stgsvc::sbs;

/// Help for these commands.
pub const HELP: &str = "\
crtsbsd LIB/SBS [POOLS((1 *BASE) (2 size act)) MAXJOBS() TEXT()] | chgsbsd SBS ... | dltsbsd SBS
addrtge|chgrtge SBS SEQNBR(n) [CMPVAL('v' start) PGM() CLS() MAXACT() POOLID()] | rmvrtge SBS SEQNBR(n)
addwse SBS WRKSTN()|WRKSTNTYPE() [JOBD() MAXACT() AT()] | rmvwse SBS WRKSTN()|WRKSTNTYPE()
addjobqe|chgjobqe SBS JOBQ() [SEQNBR() MAXACT() MAXPTY1()..] | rmvjobqe SBS JOBQ()
addaje SBS JOB() JOBD() | rmvaje SBS JOB() | addpje|chgpje SBS PGM() [...] | rmvpje SBS PGM()
dspsbsd SBS [OPTION(1-10)] | strsbs SBS | endsbs SBS|*ALL [OPTION(*CNTRLD|*IMMED) DELAY(s)]
wrksbs | wrksbsjob [SBS|*ALL] | wrkshrpool | chgshrpool POOL(*SHRPOOLn|*INTERACT|*SPOOL|*BASE) SIZE(kb) ACTLVL(n)
";

/// Dispatch; `None` when `cmd` is not one of these.
pub fn command(system: &mut System, cmd: &str, line: &str) -> Option<String> {
    let p = Params::parse(line);
    let edit =
        |f: fn(&mut Sbsd, &Params) -> wm::Result<()>| move |s: &mut Sbsd, p: &Params| f(s, p);
    let r = match cmd {
        "crtsbsd" => crtsbsd(system, &p),
        "chgsbsd" => with_sbsd(system, &p, |s, p| {
            *s = sbsdcmd::change(s.clone(), p)?;
            Ok(())
        }),
        "dltsbsd" => dltsbsd(system, &p),
        "addrtge" => with_sbsd(system, &p, |s, p| sbsdcmd::rtge(s, p, true)),
        "chgrtge" => with_sbsd(system, &p, |s, p| sbsdcmd::rtge(s, p, false)),
        "rmvrtge" => with_sbsd(system, &p, edit(sbsdcmd::rmvrtge)),
        "addwse" => with_sbsd(system, &p, edit(sbsdcmd::addwse)),
        "rmvwse" => with_sbsd(system, &p, edit(sbsdcmd::rmvwse)),
        "addjobqe" => with_sbsd(system, &p, |s, p| sbsdcmd::jobqe(s, p, true)),
        "chgjobqe" => with_sbsd(system, &p, |s, p| sbsdcmd::jobqe(s, p, false)),
        "rmvjobqe" => with_sbsd(system, &p, edit(sbsdcmd::rmvjobqe)),
        "addaje" => with_sbsd(system, &p, edit(sbsdcmd::addaje)),
        "rmvaje" => with_sbsd(system, &p, edit(sbsdcmd::rmvaje)),
        "addpje" => with_sbsd(system, &p, |s, p| sbsdcmd::pje(s, p, true)),
        "chgpje" => with_sbsd(system, &p, |s, p| sbsdcmd::pje(s, p, false)),
        "rmvpje" => with_sbsd(system, &p, edit(sbsdcmd::rmvpje)),
        "dspsbsd" => dspsbsd(system, &p),
        "wrksbs" => Ok(sbs::wrksbs(system)),
        "wrksbsjob" => Ok(sbs::wrksbsjob(
            system,
            &p.pos
                .first()
                .map_or("*ALL".into(), |s| s.to_ascii_uppercase()),
        )),
        "endsbs" => endsbs(system, &p),
        "wrkshrpool" => Ok(wrkshrpool(system)),
        "chgshrpool" => chgshrpool(system, &p),
        _ => return None,
    };
    Some(match r {
        Ok(s) => s,
        Err(e) => format!("{e}\n"),
    })
}

fn err(e: &wm::Error) -> String {
    format!("{} {e}", e.msgid().unwrap_or("CPF1691"))
}

/// The description named first on the line: `LIB/NAME`, else `QSYS`.
fn target(p: &Params) -> Result<(String, String), String> {
    let (lib, name) = qualified(&p.pos_or(0, "SBSD")?);
    Ok((if lib == "*LIBL" { "QSYS".into() } else { lib }, name))
}

/// Change a description in place.
fn with_sbsd(
    system: &mut System,
    p: &Params,
    f: impl FnOnce(&mut Sbsd, &Params) -> wm::Result<()>,
) -> Result<String, String> {
    let (lib, name) = target(p)?;
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    super::db::with_txn(&mut s.store, |txn| {
        let names = super::ile::name_context(txn, qsys);
        let id = wm::obj::find(txn, qsys, &names, &lib, &name, types::SBSD).map_err(|e| err(&e))?;
        let mut d =
            Sbsd::decode(&wm::obj::read(txn, id).map_err(|e| err(&e))?).map_err(|e| err(&e))?;
        f(&mut d, p).map_err(|e| err(&e))?;
        wm::obj::write(txn, id, &d.encode()).map_err(|e| err(&e))?;
        Ok(())
    })?;
    Ok(format!("Subsystem description {name} changed.\n"))
}

/// `CRTSBSD`.
#[allow(clippy::many_single_char_names)] // params, description, store, library, name
fn crtsbsd(system: &mut System, p: &Params) -> Result<String, String> {
    let (lib, name) = target(p)?;
    let d = sbsdcmd::change(Sbsd::default(), p).map_err(|e| err(&e))?;
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    super::db::with_txn(&mut s.store, |txn| {
        let l = wm::obj::library(txn, qsys, &lib).map_err(|e| err(&e))?;
        let n = qsname::Name::parse(&name).map_err(|e| format!("CPD0078 {name}: {e}"))?;
        let stamp = wm::obj::Stamp {
            owner: "QSECOFR".into(),
            system: "QS400".into(),
            now: now_unix(),
        };
        wm::obj::create(txn, l, n, types::SBSD, &d.text, &stamp, &d.encode())
            .map_err(|e| err(&e))?;
        Ok(())
    })?;
    Ok(format!(
        "Subsystem description {name} created in library {lib}.\n"
    ))
}

/// `DLTSBSD`: not while active.
fn dltsbsd(system: &mut System, p: &Params) -> Result<String, String> {
    let (lib, name) = target(p)?;
    if system.sbs.contains_key(&name) {
        return Err(format!("CPF1010 subsystem {name} is active"));
    }
    let job = JobHandle(u64::from(system.current_job));
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    super::db::with_txn(&mut s.store, |txn| {
        let names = super::ile::name_context(txn, qsys);
        let id = wm::obj::find(txn, qsys, &names, &lib, &name, types::SBSD).map_err(|e| err(&e))?;
        ops::delete(txn, job, id).map_err(|e| e.to_string())
    })?;
    Ok(format!("Subsystem description {name} deleted.\n"))
}

/// `DSPSBSD [OPTION(n)]`.
fn dspsbsd(system: &mut System, p: &Params) -> Result<String, String> {
    let (lib, name) = target(p)?;
    let b = sbs::wm_object(system, &format!("{lib}/{name}"), "*SBSD")?;
    let d = Sbsd::decode(&b).map_err(|e| err(&e))?;
    let option = match p.kw("OPTION") {
        Some(o) => Some(
            o.trim()
                .parse::<u8>()
                .ok()
                .filter(|n| (1..=10).contains(n))
                .ok_or_else(|| format!("CPD0084 OPTION({o}) is 1 through 10"))?,
        ),
        None => None,
    };
    Ok(d.display(&lib, &name, option))
}

/// `ENDSBS SBS|*ALL [OPTION(*CNTRLD|*IMMED) DELAY(seconds|*NOLIMIT)]`.
fn endsbs(system: &mut System, p: &Params) -> Result<String, String> {
    let name = p.pos_or(0, "SBS")?.to_ascii_uppercase();
    let immediate = match p.upper("OPTION").as_deref() {
        None | Some("*CNTRLD") => false,
        Some("*IMMED") => true,
        Some(o) => return Err(format!("CPD0084 OPTION({o})")),
    };
    let delay = match p.upper("DELAY").as_deref() {
        None | Some("*NOLIMIT") => 0,
        Some(d) => d.parse().map_err(|_| format!("CPD0084 DELAY({d})"))?,
    };
    sbs::end(system, &name, immediate, delay)
}

/// `WRKSHRPOOL`.
fn wrkshrpool(system: &System) -> String {
    let mut out = String::from(
        "Pool          System Pool  Defined Size (K)  Max Active  Allocated (K)  Active  Ineligible\n",
    );
    for p in system.pools.info() {
        if !p.name.starts_with('*') {
            continue;
        }
        let _ = writeln!(
            out,
            "{:<12}  {:>11}  {:>16}  {:>10}  {:>13}  {:>6}  {:>10}",
            p.name, p.number, p.size_kb, p.actlvl, p.size_kb, p.active, p.ineligible
        );
    }
    out
}

/// `CHGSHRPOOL POOL() SIZE(kb) ACTLVL(n)`.
fn chgshrpool(system: &mut System, p: &Params) -> Result<String, String> {
    let pool = p
        .upper("POOL")
        .or_else(|| p.pos.first().map(|s| s.to_ascii_uppercase()))
        .ok_or("CPD0071 POOL is needed")?;
    if pool == "*MACHINE" {
        return Err("CPF1165 *MACHINE is sized by QMCHPOOL".into());
    }
    let cur = system
        .pools
        .get(&pool)
        .cloned()
        .ok_or_else(|| format!("CPD0084 POOL({pool})"))?;
    let size: u64 = match p.kw("SIZE") {
        Some(s) => s.trim().parse().map_err(|_| format!("CPD0084 SIZE({s})"))?,
        None => cur.size_kb,
    };
    let actlvl: u32 = match p.kw("ACTLVL") {
        Some(a) => a
            .trim()
            .parse()
            .map_err(|_| format!("CPD0084 ACTLVL({a})"))?,
        None => cur.actlvl,
    };
    let total: u64 = system
        .pools
        .info()
        .iter()
        .filter(|i| i.name != pool)
        .map(|i| i.size_kb)
        .sum::<u64>()
        + size;
    let main = main_storage_kb();
    if main > 0 && total > main {
        return Err(format!(
            "CPF1165 pools would total {total} K, more than main storage ({main} K)"
        ));
    }
    system.pools.define(&pool, size, actlvl);
    Ok(format!(
        "Shared pool {pool} changed: {size} K, activity level {actlvl}.\n"
    ))
}

/// Main storage in kilobytes, from `/proc/meminfo`.
pub fn main_storage_kb() -> u64 {
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|m| {
            m.lines()
                .find(|l| l.starts_with("MemTotal:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|n| n.parse().ok())
        })
        .unwrap_or(0)
}
