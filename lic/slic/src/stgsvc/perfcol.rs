// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Collection Services in the storage service (`docs/PERF.md`): the
//! collector the serve loop ticks. While a collection is active each
//! interval's data (the machine between two of the samples the
//! displays use) is appended to its `*MGTCOL` and, with
//! `CRTDBF(*YES)`, written to the `QAPM*` files at once. `QPFRADJ` 2
//! and 3 adjust the shared pools each minute from the same samples.

use std::time::{Duration, Instant};

use perf::config::Config;
use perf::mgtcol::{self, Collection, Header};
use perf::profile::{self, Category};
use perf::sample::Interval;
use qsname::{MemberSpec, Name};
use store::obj::space::Space;
use store::obj::{CreateParams, ObjectId, ops, types};
use store::txn::Txn;

use super::stats::{self, Sample};
use crate::ipl::system::{System, now_unix};

/// The data area that keeps `CFGPFRCOL`'s configuration.
const CONFIG: &str = "QYPSCOLCFG";
/// A management collection's attribute.
pub const ATTRIBUTE: &str = "*PFR";
/// `QPFRADJ` adjusts this often.
const ADJUST_EVERY: Duration = Duration::from_secs(60);

/// A collection in progress.
#[derive(Debug)]
pub struct Active {
    /// Its name, `Qdddhhmmss`.
    pub name: String,
    /// Its library.
    pub library: String,
    /// What it is.
    pub header: Header,
    /// Intervals recorded.
    pub intervals: u32,
    base: Sample,
    next: Instant,
    every: Duration,
    /// The local hour a cycling collection ends at, as hours since the
    /// epoch.
    cycle_at: Option<u64>,
}

/// The collector's state.
#[derive(Debug, Default)]
pub struct Collector {
    /// The collection in progress.
    pub active: Option<Active>,
    adjust: Option<(Instant, Sample)>,
    /// Each pool's defined size and level (`CHGSHRPOOL`, the IPL) and
    /// what `QPFRADJ` last set.
    defined: std::collections::BTreeMap<String, (u64, u32)>,
    adjusted: std::collections::BTreeMap<String, (u64, u32)>,
}

/// Local time now: UTC moved by `QTIMZON`'s offset.
pub fn local_now(system: &System) -> u64 {
    let unix = now_unix();
    let zone = system
        .sysvals
        .as_ref()
        .and_then(|v| v.get("QTIMZON").ok())
        .map(|v| v.text())
        .unwrap_or_default();
    let off = sysval::tz::find(zone.trim()).map_or(0, |z| i64::from(z.offset_at(unix)));
    u64::try_from(i64::try_from(unix).unwrap_or(0) + off).unwrap_or(unix)
}

fn sysval_text(system: &System, name: &str) -> String {
    system
        .sysvals
        .as_ref()
        .and_then(|v| v.get(name).ok())
        .map(|v| v.text().trim().to_string())
        .unwrap_or_default()
}

fn in_store<T>(system: &mut System, f: impl FnOnce(&mut Txn<'_>, ObjectId) -> Result<T, String>) -> Result<T, String> {
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    crate::svc::db::with_txn(&mut s.store, |txn| f(txn, qsys))
}

fn lookup(txn: &mut Txn<'_>, lib: ObjectId, ty: u16, name: &str) -> Result<Option<ObjectId>, String> {
    let n = Name::parse(name).map_err(|e| format!("CPD0074 {e}"))?;
    store::obj::library::Library::load(txn, lib)
        .and_then(|l| l.lookup(txn, ty, n))
        .map_err(|e| e.to_string())
}

/// The configuration `CFGPFRCOL` left, or the shipped one.
pub fn config(system: &mut System) -> Config {
    in_store(system, |txn, qsys| {
        let Some(id) = lookup(txn, qsys, types::DTAARA, CONFIG)? else {
            return Ok(Config::default());
        };
        Ok(match store::obj::dtaara::read(txn, id).map_err(|e| e.to_string())? {
            store::obj::dtaara::DataAreaValue::Char(t) => Config::decode(&t),
            _ => Config::default(),
        })
    })
    .unwrap_or_default()
}

/// Keep `c` as the configuration.
pub fn save_config(system: &mut System, c: &Config) -> Result<(), String> {
    use store::obj::dtaara::{self, DataAreaType, DataAreaValue};
    let v = DataAreaValue::Char(c.encode());
    in_store(system, |txn, qsys| {
        if let Some(id) = lookup(txn, qsys, types::DTAARA, CONFIG)? {
            return dtaara::write(txn, id, &v).map_err(|e| e.to_string());
        }
        let params = CreateParams {
            library: qsys,
            name: Name::parse(CONFIG).expect("valid"),
            type_code: types::DTAARA,
            attribute: String::new(),
            text: "Collection Services configuration".into(),
            owner: "QSYS".into(),
            system: "QS400".into(),
            now: now_unix(),
            by_system: true,
        };
        dtaara::create(txn, &params, DataAreaType::Char { len: 1000 }, &v)
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
}

/// Library `name`, created when it is missing.
fn ensure_library(txn: &mut Txn<'_>, qsys: ObjectId, name: &str, system: &str) -> Result<ObjectId, String> {
    if let Some(id) = lookup(txn, qsys, types::LIB, name)? {
        return Ok(id);
    }
    ops::create(
        txn,
        &CreateParams {
            library: qsys,
            name: Name::parse(name).map_err(|e| format!("CPD0074 {e}"))?,
            type_code: types::LIB,
            attribute: "PROD".into(),
            text: "Performance data".into(),
            owner: "QSYS".into(),
            system: system.into(),
            now: now_unix(),
            by_system: true,
        },
    )
    .map_err(|e| e.to_string())
}

/// `STRPFRCOL`: start a collection; its name.
pub fn start(system: &mut System, colprf: &str, cycle: bool) -> Result<String, String> {
    if system.stats.perf.active.is_some() {
        return Err("CPFB993 Collection Services is already active.".into());
    }
    let cfg = config(system);
    let cats = cfg
        .categories(colprf)
        .ok_or_else(|| format!("CPD0084 COLPRF({colprf})"))?;
    let profile = if colprf.eq_ignore_ascii_case("*CFG") {
        cfg.profile.clone()
    } else {
        colprf.to_ascii_uppercase()
    };
    let started = local_now(system);
    let name = mgtcol::name_of(started);
    let sysname = system.system_name();
    let header = Header {
        started,
        interval: cfg.interval,
        profile,
        categories: profile::mask(&cats),
        system: sysname.clone(),
        release: {
            let r = release::RELEASE;
            format!("V{}R{}M{}", r.v, r.r, r.m)
        },
        dbf_library: if cfg.create_files { cfg.library.clone() } else { String::new() },
        cycle,
    };
    let payload = mgtcol::new_payload(&header);
    let lib = cfg.library.clone();
    let retention = cfg.retention_secs();
    let now = now_unix();
    in_store(system, |txn, qsys| {
        let l = ensure_library(txn, qsys, &lib, &sysname)?;
        expire(txn, l, retention, now)?;
        let n = Name::parse(&name).map_err(|e| e.to_string())?;
        if lookup(txn, l, types::MGTCOL, &name)?.is_some() {
            return Err(format!("CPFB993 Management collection {name} exists in {lib}."));
        }
        wm::obj::create_attr(
            txn,
            l,
            (n, types::MGTCOL, ATTRIBUTE),
            "",
            &wm::obj::Stamp {
                owner: "QSYS".into(),
                system: sysname.clone(),
                now,
            },
            &payload,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })?;
    let every = Duration::from_secs(u64::from(cfg.interval.max(1)));
    let cycle_at = cycle.then(|| {
        let hh: u64 = cfg.cycle_time.get(..2).and_then(|h| h.parse().ok()).unwrap_or(0);
        let hours = started / 3600;
        let mut at = hours - hours % 24 + hh;
        while at <= hours {
            at += u64::from(cfg.cycle_hours.clamp(1, 24));
        }
        at
    });
    let base = stats::sample(system);
    system.stats.perf.active = Some(Active {
        name: name.clone(),
        library: lib,
        header,
        intervals: 0,
        base,
        next: Instant::now() + every,
        every,
        cycle_at,
    });
    Ok(name)
}

/// Collections in `lib` older than `retention` seconds, deleted.
fn expire(txn: &mut Txn<'_>, lib: ObjectId, retention: Option<u64>, now: u64) -> Result<(), String> {
    let Some(keep) = retention else {
        return Ok(());
    };
    for (_, id) in ops::list(txn, lib, Some(types::MGTCOL)).map_err(|e| e.to_string())? {
        let h = ops::describe(txn, id).map_err(|e| e.to_string())?;
        if h.created + keep < now {
            ops::delete(txn, store::obj::JobHandle(0), id).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Append `bytes` to the payload of object `id` and patch its head:
/// the interval count and, when ending, the end time.
fn append(txn: &mut Txn<'_>, id: ObjectId, bytes: &[u8], count: u32, ended: Option<u64>) -> Result<(), String> {
    let (space, _) = Space::load(txn, id).map_err(|e| e.to_string())?;
    let head = space.read(txn, 0, 4).map_err(|e| e.to_string())?;
    let len = u64::from(u32::from_le_bytes(head[..4].try_into().expect("four bytes")));
    let new_len = len + bytes.len() as u64;
    let space = Space::ensure(txn, id, 4 + new_len, 0).map_err(|e| e.to_string())?;
    if !bytes.is_empty() {
        space.write(txn, 4 + len, bytes).map_err(|e| e.to_string())?;
    }
    let total = u32::try_from(new_len).map_err(|_| "management collection over 4 GiB".to_string())?;
    space.write(txn, 0, &total.to_le_bytes()).map_err(|e| e.to_string())?;
    space
        .write(txn, 4 + mgtcol::OFF_COUNT as u64, &count.to_be_bytes())
        .map_err(|e| e.to_string())?;
    if let Some(t) = ended {
        space
            .write(txn, 4 + mgtcol::OFF_ENDED as u64, &t.to_be_bytes())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Record one interval of the active collection, ending now.
fn record(system: &mut System, ended: Option<u64>) -> Result<(), String> {
    let Some(base) = system.stats.perf.active.as_ref().map(|a| a.base.clone()) else {
        return Ok(());
    };
    let now = stats::sample_after(system, Some(&base));
    let end = local_now(system);
    let Some(a) = system.stats.perf.active.as_ref() else {
        return Ok(());
    };
    let (number, mask) = (a.intervals + 1, a.header.categories);
    let interval = stats::interval_data(system, &now, &base, number, end).keep(mask);
    let a = system.stats.perf.active.as_mut().expect("active");
    a.intervals = number;
    a.base = now;
    a.next += a.every;
    if a.next < Instant::now() {
        a.next = Instant::now() + a.every;
    }
    let (name, lib) = (a.name.clone(), a.library.clone());
    let dbf = a.header.dbf_library.clone();
    let header = a.header.clone();
    let frame = mgtcol::frame(&interval);
    in_store(system, |txn, qsys| {
        let l = lookup(txn, qsys, types::LIB, &lib)?.ok_or_else(|| format!("CPF2110 {lib}"))?;
        let id = lookup(txn, l, types::MGTCOL, &name)?.ok_or_else(|| format!("CPF8225 {name}"))?;
        append(txn, id, &frame, number, ended)
    })?;
    if !dbf.is_empty() {
        let part = Collection {
            header,
            ended: ended.unwrap_or(0),
            intervals: vec![interval],
        };
        write_files(system, &part, &dbf, &name, None, number == 1)?;
    }
    Ok(())
}

/// `ENDPFRCOL`: end the active collection, after a last interval.
pub fn end(system: &mut System, force: bool) -> Result<String, String> {
    let Some(a) = system.stats.perf.active.as_ref() else {
        return Err("CPFB994 Collection Services is not active.".into());
    };
    let name = a.name.clone();
    let ended = local_now(system);
    let r = record(system, Some(ended));
    system.stats.perf.active = None;
    match r {
        Err(e) if !force => Err(e),
        _ => Ok(name),
    }
}

/// The serve loop's turn: an interval due, a collection to cycle, the
/// pools to adjust.
pub fn tick(system: &mut System) {
    let due = system
        .stats
        .perf
        .active
        .as_ref()
        .is_some_and(|a| Instant::now() >= a.next);
    if due {
        if let Err(e) = record(system, None) {
            eprintln!("qsstg: Collection Services: {e}");
        }
        let cycle = system.stats.perf.active.as_ref().and_then(|a| a.cycle_at).is_some_and(|h| local_now(system) / 3600 >= h);
        if cycle {
            let prf = system.stats.perf.active.as_ref().map(|a| a.header.profile.clone()).unwrap_or_default();
            let _ = end(system, true);
            if let Err(e) = start(system, &prf, true) {
                eprintln!("qsstg: Collection Services: {e}");
            }
        }
    }
    adjust_tick(system);
}

/// `QPFRADJ` 2 and 3: every minute. (1 and 2 adjust at IPL on IBM's
/// systems; here the pools start at their defined sizes, which are the
/// cgroups' limits, so there is nothing to adjust before data arrives.)
fn adjust_tick(system: &mut System) {
    let mode = sysval_text(system, "QPFRADJ");
    if !matches!(mode.as_str(), "2" | "3") {
        system.stats.perf.adjust = None;
        return;
    }
    let due = system.stats.perf.adjust.as_ref().is_none_or(|(at, _)| at.elapsed() >= ADJUST_EVERY);
    if !due {
        return;
    }
    let base = system.stats.perf.adjust.take().map(|(_, b)| b);
    let now = stats::sample_after(system, base.as_ref());
    if let Some(base) = base {
        let i = stats::interval_data(system, &now, &base, 0, 0);
        apply(system, &i);
    }
    system.stats.perf.adjust = Some((Instant::now(), now));
}

/// Main storage, kilobytes.
fn main_kb() -> u64 {
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|t| wm::stats::mem_total_kb(&t))
        .unwrap_or(0)
}

/// The changes `perf::adjust` makes for interval `i`, applied.
fn apply(system: &mut System, i: &Interval) {
    let main = main_kb();
    if main == 0 {
        return;
    }
    // A pool changed since QPFRADJ last set it was defined anew.
    for p in &i.pools {
        let now = (p.size_kb, p.actlvl);
        if system.stats.perf.adjusted.get(&p.name) != Some(&now) {
            system.stats.perf.defined.insert(p.name.clone(), now);
        }
    }
    let defined = system.stats.perf.defined.clone();
    #[allow(clippy::cast_precision_loss)] // counts far below 2^52
    let pools: Vec<perf::adjust::Pool> = i
        .pools
        .iter()
        .filter(|p| p.name.starts_with('*'))
        .map(|p| {
            let (priority, min_pct, max_pct, fault_min, fault_thread, fault_max) = perf::adjust::defaults(&p.name);
            let threads: u32 = i.jobs.iter().filter(|j| j.pool == p.number).map(|j| j.threads).sum();
            perf::adjust::Pool {
                name: p.name.clone(),
                size_kb: p.size_kb,
                actlvl: p.actlvl,
                defined_actlvl: defined.get(&p.name).map_or(p.actlvl, |d| d.1),
                faults: (p.db_faults + p.ndb_faults) as f64 / f64::from(i.secs.max(1)),
                threads,
                ineligible: p.wi + p.ai,
                active: p.active,
                priority,
                min_pct,
                max_pct,
                fault_min,
                fault_thread,
                fault_max,
            }
        })
        .collect();
    let changes = perf::adjust::adjust(&pools, main);
    if changes.is_empty() {
        return;
    }
    // The pools' current sizes and levels change; their defined ones
    // (QWCSHRPOOL) stay as CHGSHRPOOL left them.
    for c in &changes {
        system.pools.define(&c.name, c.size_kb, c.actlvl);
        system.stats.perf.adjusted.insert(c.name.clone(), (c.size_kb, c.actlvl));
        let dir = stats::pool_dir(&c.name);
        let _ = std::fs::write(format!("{dir}/memory.high"), (c.size_kb * 1024).to_string());
    }
}

/// `CHKPFRCOL`: the active collection, if any.
pub fn check(system: &System) -> Option<(String, String, u32)> {
    system
        .stats
        .perf
        .active
        .as_ref()
        .map(|a| (a.name.clone(), a.library.clone(), a.intervals))
}

/// Collection `lib/name` read back.
pub fn read(system: &mut System, lib: &str, name: &str) -> Result<Collection, String> {
    in_store(system, |txn, qsys| {
        let l = lookup(txn, qsys, types::LIB, lib)?.ok_or_else(|| format!("CPF2110 Library {lib} not found."))?;
        let id = lookup(txn, l, types::MGTCOL, name)?
            .ok_or_else(|| format!("CPFB995 Management collection {name} not found in {lib}."))?;
        let b = wm::obj::read(txn, id).map_err(|e| e.to_string())?;
        mgtcol::parse(&b).map_err(|e| format!("CPF8225 {name}: {e}"))
    })
}

/// The files of `c`'s categories in `cats` (all of the collection's
/// when `None`), in `lib`, member `member`: created when missing; the
/// member replaced when `replace`, else the records added.
pub fn write_files(
    system: &mut System,
    c: &Collection,
    lib: &str,
    member: &str,
    cats: Option<&[Category]>,
    replace: bool,
) -> Result<usize, String> {
    let have = profile::of_mask(c.header.categories);
    let cats: Vec<Category> = cats.map_or(have.clone(), |w| w.iter().copied().filter(|x| have.contains(x)).collect());
    let mut files: Vec<&str> = vec!["QAPMCONF"];
    for cat in &cats {
        files.extend(cat.files());
    }
    let sysname = system.system_name();
    let now = now_unix();
    let mut written = 0;
    for file in files {
        let rows = perf::qapm::rows(file, c);
        // The configuration is written once, with the first interval.
        if file == "QAPMCONF" && !replace {
            continue;
        }
        written += in_store(system, |txn, qsys| {
            let l = ensure_library(txn, qsys, lib, &sysname)?;
            let fid = match lookup(txn, l, types::FILE, file)? {
                Some(id) => id,
                None => perf::qapm::create(txn, l, file, now)?,
            };
            let mname = Name::parse(member).map_err(|e| format!("CPD0074 {e}"))?;
            let spec = MemberSpec::Name(mname);
            let mut m = if let Ok(m) = db::Member::open_by(txn, fid, spec) {
                m
            } else {
                db::file::add_member(txn, fid, mname, "", None, now).map_err(|e| e.to_string())?;
                db::Member::open_by(txn, fid, spec).map_err(|e| e.to_string())?
            };
            if replace {
                m.clear(txn).map_err(|e| e.to_string())?;
            }
            let fmt = m.format().format.clone();
            let nulls = vec![0u8; m.core.null_map_len()];
            for row in &rows {
                let rec = perf::qapm::encode(&fmt, row)?;
                m.write_new(txn, &rec, &nulls).map_err(|e| e.to_string())?;
            }
            Ok(rows.len())
        })?;
    }
    Ok(written)
}
