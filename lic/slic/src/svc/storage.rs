// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Storage commands of the service console: `asp`, `rclstg`, `chkstg`,
//! `txn stats`.

use std::fmt::Write as _;
use std::path::Path;
use std::time::Instant;

use blockdev::{BlockDev, LinuxBlockDev};
use store::ReclaimMode;

use crate::ipl::storage;
use crate::ipl::system::{System, now_unix};

/// Open a device node for a new unit, with its serial from sysfs.
fn open_unit(path: &str) -> Result<(Box<dyn BlockDev>, String), String> {
    let p = Path::new(path);
    let dev = LinuxBlockDev::open(p).map_err(|e| format!("open {path}: {e}"))?;
    let (_, serial) = diskimg::locate::identity(p);
    Ok((Box::new(dev), serial))
}

fn unit_number(s: &str) -> Result<u16, String> {
    s.parse()
        .map_err(|_| format!("unit number expected, got '{s}'"))
}

/// `unit add DEV` and `unit rmv N`.
pub fn unit(system: &mut System, args: &[&str]) -> String {
    let result = match args {
        ["add", path] => {
            let Some(s) = system.storage.as_mut() else {
                return "object store not open\n".to_string();
            };
            open_unit(path).and_then(|(dev, serial)| {
                s.store
                    .add_unit(dev, storage::fresh_uuid(), &serial)
                    .map(|n| format!("unit {n} added from {path} (serial {serial})\n"))
                    .map_err(|e| e.to_string())
            })
        }
        ["rmv", n] => {
            let Some(s) = system.storage.as_mut() else {
                return "object store not open\n".to_string();
            };
            unit_number(n).and_then(|n| {
                s.store
                    .remove_unit(n)
                    .map(|d| format!("unit {n} removed ({} pages given back)\n", d.blocks()))
                    .map_err(|e| e.to_string())
            })
        }
        _ => Err("Usage: unit add DEV | unit rmv N".to_string()),
    };
    result.unwrap_or_else(|e| format!("{e}\n"))
}

/// `mirror start|stop|suspend|resume|replace|status`.
pub fn mirror(system: &mut System, args: &[&str]) -> String {
    if let ["status"] = args {
        return match storage::resume_progress(system) {
            Some(p) => format!(
                "unit {} resuming: {} of {} chunks, {}%\n",
                p.unit, p.chunks_done, p.chunks, p.percent
            ),
            None => "no resume running\n".to_string(),
        };
    }
    if let ["resume", n] = args {
        let started = unit_number(n)
            .and_then(|n| storage::start_resume(system, n).map_err(|e| e.to_string()));
        return match started {
            Ok(line) => line,
            Err(e) => format!("{e}\n"),
        };
    }
    let Some(s) = system.storage.as_mut() else {
        return "object store not open\n".to_string();
    };
    let result = match args {
        ["start", n, path] => unit_number(n).and_then(|n| {
            open_unit(path).and_then(|(dev, serial)| {
                s.store
                    .start_mirror(n, dev, storage::fresh_uuid(), &serial)
                    .map(|new| {
                        format!("unit {n} paired with new unit {new} from {path}; resume it to synchronize\n")
                    })
                    .map_err(|e| e.to_string())
            })
        }),
        ["stop", n] => unit_number(n).and_then(|n| {
            s.store
                .stop_mirror(n)
                .map(|_| format!("mirroring stopped; unit {n} kept\n"))
                .map_err(|e| e.to_string())
        }),
        ["suspend", n] => unit_number(n).and_then(|n| {
            s.store
                .suspend_unit(n)
                .map(|()| format!("unit {n} suspended\n"))
                .map_err(|e| e.to_string())
        }),
        ["replace", n, path] => unit_number(n).and_then(|n| {
            open_unit(path).and_then(|(dev, serial)| {
                s.store
                    .replace_unit(n, dev, storage::fresh_uuid(), &serial)
                    .map(|()| format!("unit {n} replaced from {path}; resume it to synchronize\n"))
                    .map_err(|e| e.to_string())
            })
        }),
        _ => Err(
            "Usage: mirror start N DEV | stop N | suspend N | resume N | replace N DEV | status"
                .to_string(),
        ),
    };
    result.unwrap_or_else(|e| format!("{e}\n"))
}

/// The hot spares, one line each.
fn spare_lines(st: &store::Store, out: &mut String) {
    for h in st.hot_spares() {
        let u = h.unit_uuid;
        let _ = writeln!(
            out,
            "  hot spare {:02x}{:02x}{:02x}{:02x}: {} pages{}",
            u[0],
            u[1],
            u[2],
            u[3],
            h.pages,
            if h.attached { "" } else { " (not attached)" }
        );
    }
}

/// `asp threshold PERCENT`: what DST and SST's Work with ASP threshold
/// sets.
pub fn asp_threshold(system: &mut System, percent: &str) -> String {
    use svctools::DstEnv as _;
    let Ok(p) = percent.parse::<u8>() else {
        return format!("{}\n", svctools::dst::disk::reasons::THRESHOLD_RANGE);
    };
    match crate::ipl::dst::Env::new(system).set_asp_threshold(p) {
        Ok(()) => format!("ASP 1 threshold set to {p}%\n"),
        Err(e) => format!("{e}\n"),
    }
}

/// `asp`: units, free space, WAL fill, checkpoint generation, release.
pub fn asp(system: &mut System) -> String {
    let mut out = String::new();
    if let Some(s) = system.storage.as_ref() {
        let st = &s.store;
        let a = st.asp();
        let sb = a.superblock();
        let _ = writeln!(
            out,
            "ASP 1: {} unit(s), {} pages ({} MiB), {} free ({} available), {} metadata; created by {}, generation {}, next serial {}, installed {}",
            a.layout().unit_count(),
            a.layout().total_pages(),
            (a.layout().total_pages() * 4096) >> 20,
            a.total_free(),
            a.available(),
            a.metadata_pages(),
            sb.release,
            sb.generation,
            sb.next_serial,
            sb.install_time
        );
        let _ = writeln!(
            out,
            "  protection: {}; recovery replayed {} transaction(s) at IPL; threshold {}%",
            protection(sb.protection),
            s.recovery.replayed,
            sb.threshold
        );
        for u in st.units() {
            let e = &u.entry;
            let partner = if e.mirrored() {
                format!(" partner {}", e.mirror_partner)
            } else {
                String::new()
            };
            let _ = writeln!(
                out,
                "  unit {} {:<15} {} pages ({} usable), {} used, serial {}{}{}",
                u.unit,
                e.state,
                e.pages,
                e.usable_pages,
                u.used_pages,
                e.serial_text(),
                partner,
                if u.attached { "" } else { " (not attached)" }
            );
        }
        parity_lines(st, &mut out);
        spare_lines(st, &mut out);
        for u in a.unit_stats() {
            let _ = writeln!(
                out,
                "  span {}: {} pages, {} free, {} pending",
                u.unit, u.pages, u.free, u.pending
            );
        }
        for (i, d) in s.devices.iter().enumerate() {
            let _ = writeln!(out, "  device {}: {}", i, d.display());
        }
        let w = st.wal();
        let _ = writeln!(
            out,
            "  WAL: {} of {} pages used (tail {} head {}), {} transaction(s) since the checkpoint, checkpoint every {} or {}s",
            w.used(),
            w.pages,
            w.tail,
            w.head,
            st.txns_since_checkpoint(),
            st.config().checkpoint_txns,
            st.config().checkpoint_secs
        );
        let _ = writeln!(
            out,
            "  flags: needs_recovery {} rclstg_required {}; last RCLSTG {}",
            sb.flag(store::asp::superblock::FLAG_NEEDS_RECOVERY),
            sb.flag(store::asp::superblock::FLAG_RCLSTG_REQUIRED),
            sb.last_rclstg_time
        );
    } else {
        let _ = writeln!(out, "ASP 1: not open");
    }
    match system.temp.as_ref() {
        Some(t) => {
            let a = t.store.asp();
            let _ = writeln!(
                out,
                "ASP 0 (QTEMP): {} pages, {} free, {}",
                a.layout().total_pages(),
                a.total_free(),
                t.qtemp
            );
        }
        None => {
            let _ = writeln!(out, "ASP 0 (QTEMP): not available");
        }
    }
    out
}

/// `rclstg [*CHECK|*ALL|*SYSBAS]`.
pub fn rclstg(system: &mut System, args: &[&str]) -> String {
    let mode = match args {
        [] | ["*ALL" | "*all"] => ReclaimMode::All,
        ["*SYSBAS" | "*sysbas"] => ReclaimMode::SysBas,
        ["*CHECK" | "*check"] => ReclaimMode::Check,
        _ => return "Usage: rclstg [*ALL|*SYSBAS|*CHECK]\n".to_string(),
    };
    let Some(s) = system.storage.as_mut() else {
        return "object store not open\n".to_string();
    };
    let start = Instant::now();
    let mut out = match s.store.reclaim(mode, now_unix()) {
        Ok(r) => format!(
            "RCLSTG {mode:?} in {:.2}s\n{r}\n",
            start.elapsed().as_secs_f64()
        ),
        Err(e) => return format!("RCLSTG failed: {e}\n"),
    };
    out.push_str(&crate::ipl::ifs::reclaim(system));
    let Some(s) = system.storage.as_mut() else {
        return out;
    };
    // The profile indexes: verified by *CHECK, rebuilt otherwise.
    if let Some(qsys) = s.qsys {
        let r = (|| -> sec::Result<String> {
            let mut txn = s.store.begin()?;
            let line = if matches!(mode, ReclaimMode::Check) {
                let problems = sec::profile::index::verify(&mut txn, qsys)?;
                if problems.is_empty() {
                    "profile indexes: no problems".to_string()
                } else {
                    format!(
                        "profile indexes: {} problem(s)\n{}",
                        problems.len(),
                        problems.join("\n")
                    )
                }
            } else {
                let n = sec::profile::index::rebuild(&mut txn, qsys)?;
                format!("profile indexes rebuilt: {n} entries")
            };
            txn.commit()?;
            Ok(line)
        })();
        match r {
            Ok(l) => out.push_str(&l),
            Err(e) => {
                let _ = write!(out, "profile indexes: {e}");
            }
        }
        out.push('\n');
    }
    out
}

/// `chkstg`: verify every allocated page's checksum.
pub fn chkstg(system: &mut System) -> String {
    let Some(s) = system.storage.as_mut() else {
        return "object store not open\n".to_string();
    };
    let start = Instant::now();
    match s.store.check_pages() {
        Ok(r) => {
            let mut out = format!(
                "checked {} allocated page(s) of {} in {:.2}s: {} bad\n",
                r.pages_allocated,
                r.pages_swept,
                start.elapsed().as_secs_f64(),
                r.bad_pages.len()
            );
            for b in r.bad_pages.iter().take(50) {
                let _ = writeln!(out, "  page {} owner {}: {}", b.page, b.owner, b.what);
            }
            out.push_str(&check_paths(system));
            out
        }
        Err(e) => format!("chkstg failed: {e}\n"),
    }
}

/// Every keyed member's access path against its records.
fn check_paths(system: &mut System) -> String {
    let Some(s) = system.storage.as_mut() else {
        return String::new();
    };
    let Some(qsys) = s.qsys else {
        return String::new();
    };
    let mut txn = match s.store.begin() {
        Ok(t) => t,
        Err(e) => return format!("access paths not checked: {e}\n"),
    };
    let r = db::util::verify_all(&mut txn, qsys);
    txn.abort();
    match r {
        Ok(report) => report.render(),
        Err(e) => format!("access paths not checked: {e}\n"),
    }
}

/// `txn stats`.
pub fn txn_stats(system: &mut System) -> String {
    let Some(s) = system.storage.as_ref() else {
        return "object store not open\n".to_string();
    };
    let st = s.store.stats();
    // The statements in progress by job number, `W` when one waits for
    // its job.
    let load = crate::stgsvc::turns::load();
    let mut jobs = String::new();
    for (n, waits) in &load.fibers {
        jobs.push(' ');
        jobs.push_str(&n.to_string());
        if *waits {
            jobs.push('W');
        }
    }
    format!(
        "commits {} aborts {} checkpoints {}\nWAL pages {} images {} patches {}\ncache hits {} misses {} evictions {} held {} of {} pages\nstatements {} of {} queued {}:{}\n",
        st.commits,
        st.aborts,
        st.checkpoints,
        st.wal_pages,
        st.images,
        st.patches,
        st.cache.hits,
        st.cache.misses,
        st.cache.evictions,
        st.cache.held,
        st.cache.capacity,
        load.fibers.len(),
        crate::stgsvc::turns::MOST,
        load.queued,
        jobs
    )
}

/// `asp`'s device parity lines: each set's members and its work.
fn parity_lines(st: &store::Store, out: &mut String) {
    for set in st.parity_sets() {
        let members: Vec<String> = set
            .members
            .iter()
            .map(|m| match m.rebuild_percent {
                Some(p) => format!("unit {} {} {p}%", m.unit, m.status),
                None => format!("unit {} {}", m.unit, m.status),
            })
            .collect();
        let sync = set
            .sync_percent
            .map_or_else(String::new, |p| format!("; synchronizing {p}%"));
        let _ = writeln!(
            out,
            "  parity set {} ({}): {}{sync}",
            set.set,
            set.level,
            members.join(", ")
        );
    }
}

/// The ASP's protection as `asp` shows it.
fn protection(bits: u8) -> &'static str {
    use store::asp::superblock::{PROTECTION_MIRRORED, PROTECTION_PARITY};
    match (
        bits & PROTECTION_MIRRORED != 0,
        bits & PROTECTION_PARITY != 0,
    ) {
        (true, true) => "mirrored, device parity",
        (true, false) => "mirrored",
        (false, true) => "device parity",
        (false, false) => "none",
    }
}
