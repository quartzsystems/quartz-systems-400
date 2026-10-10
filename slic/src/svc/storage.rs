// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Storage commands of the service console: `asp`, `rclstg`, `chkstg`,
//! `txn stats`.

use std::fmt::Write as _;
use std::time::Instant;

use store::ReclaimMode;

use crate::ipl::system::{System, now_unix};

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
        for u in a.unit_stats() {
            let _ = writeln!(
                out,
                "  unit {} {}: {} pages, {} free, {} pending; recovery replayed {} transaction(s) at IPL",
                u.unit,
                s.device.display(),
                u.pages,
                u.free,
                u.pending,
                s.recovery.replayed
            );
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
    match s.store.reclaim(mode, now_unix()) {
        Ok(r) => format!(
            "RCLSTG {mode:?} in {:.2}s\n{r}\n",
            start.elapsed().as_secs_f64()
        ),
        Err(e) => format!("RCLSTG failed: {e}\n"),
    }
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
            out
        }
        Err(e) => format!("chkstg failed: {e}\n"),
    }
}

/// `txn stats`.
pub fn txn_stats(system: &mut System) -> String {
    let Some(s) = system.storage.as_ref() else {
        return "object store not open\n".to_string();
    };
    let st = s.store.stats();
    format!(
        "commits {} aborts {} checkpoints {}\nWAL pages {} images {} patches {}\ncache hits {} misses {} evictions {}\n",
        st.commits,
        st.aborts,
        st.checkpoints,
        st.wal_pages,
        st.images,
        st.patches,
        st.cache.hits,
        st.cache.misses,
        st.cache.evictions
    )
}
