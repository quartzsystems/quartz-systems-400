// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A mount's worker threads: each takes work from the queue, makes the
//! blocking calls, puts what came back in the cache (or a request's
//! slot) and finishes the ticket. The watchdog reports a server that
//! stopped answering, and the first answer after it.

// Block, handle, cache, queue: the short names are the domain's.
#![allow(clippy::many_single_char_names)]

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use super::cache::{BState, Block, ListEnt, Listing};
use super::fs::access;
use super::mount::{Core, RemoteEvent};
use super::request;
use super::tickets;
use super::types::{Cred, RHandle, RKind, RResult, RemoteError, Stable};
use super::work::{Answer, Mutation, Work};
use crate::errno::Errno;

/// The most pages of one directory read.
const MAX_PAGES: usize = 100_000;

/// A worker: take, run, finish, until the mount ends.
pub(super) fn serve(core: &Arc<Core>, n: usize) {
    loop {
        let (t, w) = {
            let mut q = core.queue();
            loop {
                if let Some(x) = q.items.pop_front() {
                    q.running.insert(n, Instant::now());
                    break x;
                }
                if q.stop {
                    return;
                }
                q = core
                    .cv
                    .wait(q)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        };
        // A panic in a call must not leave its waiters for ever.
        let answered = if let Ok(a) =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(core, &w)))
        {
            a
        } else {
            {
                tracing::error!(?w, "remote work panicked");
                if let Work::Mutate {
                    token: Some((id, key)),
                    ..
                } = &w
                {
                    request::post(*id, key, Err(RemoteError::Errno(Errno::EIO)));
                }
                true
            }
        };
        {
            let mut q = core.queue();
            q.running.remove(&n);
            q.inflight.remove(&w);
        }
        answered_now(core, answered);
        tickets::finish(t);
    }
}

/// The watchdog: a call running past `slow_after` makes the mount not
/// responding (reported once).
pub(super) fn watch(core: &Arc<Core>) {
    loop {
        std::thread::sleep(Duration::from_millis(100));
        let (stop, oldest, lent) = {
            let mut q = core.queue();
            let now = Instant::now();
            // Work a helper took and did not post in time goes back.
            let late: Vec<u64> = q
                .lent
                .iter()
                .filter(|(_, (_, _, d))| *d <= now)
                .map(|(k, _)| *k)
                .collect();
            for k in &late {
                if let Some((t, w, _)) = q.lent.remove(k) {
                    q.items.push_front((t, w));
                }
            }
            (q.stop, q.running.values().min().copied(), !late.is_empty())
        };
        if lent {
            core.cv.notify_all();
        }
        if stop {
            return;
        }
        if let Some(t) = oldest
            && t.elapsed() >= core.info.slow_after
            && core.responding.swap(false, Ordering::Relaxed)
        {
            emit(core, false);
        }
    }
}

fn emit(core: &Core, up: bool) {
    let fs = core.id.load(Ordering::Relaxed);
    let source = core.info.source.clone();
    let ev = if up {
        RemoteEvent::Responding { fs, source }
    } else {
        RemoteEvent::NotResponding { fs, source }
    };
    tracing::info!(?ev, "remote mount");
    if let Some(f) = &core.events {
        f(&ev);
    }
}

/// A call came back: answered by the server, or not (`TimedOut`).
pub(super) fn answered_now(core: &Core, answered: bool) {
    if answered && !core.responding.swap(true, Ordering::Relaxed) {
        emit(core, true);
    }
}

fn counted<T>(core: &Core, r: RResult<T>, answered: &mut bool) -> RResult<T> {
    core.calls.fetch_add(1, Ordering::Relaxed);
    if matches!(r, Err(RemoteError::TimedOut)) {
        *answered = false;
    }
    r
}

/// Run one piece of work; false when the server did not answer.
pub(super) fn run(core: &Core, w: &Work) -> bool {
    let mut ok = true;
    let ok_ref = &mut ok;
    match w {
        Work::Getattr { h, cred } => getattr(core, h, cred, ok_ref),
        Work::Access { h, cred } => fetch_access(core, h, cred, ok_ref),
        Work::Walk { dir, names, cred } => walk(core, dir, names, cred, ok_ref),
        Work::Readlink { h, cred } => {
            let a = counted(core, core.fs.getattr(cred, h), ok_ref);
            let r = counted(core, core.fs.readlink(cred, h), ok_ref);
            let mut c = core.cache();
            match (a, r) {
                (Ok(a), Ok(t)) => {
                    c.put_attr(h, a);
                    c.inode(h).link = Some((t, a.change));
                }
                (Err(e), _) | (_, Err(e)) => c.put_err(h, "link", e),
            }
        }
        Work::Readdir { dir, cred } => readdir(core, dir, cred, ok_ref),
        Work::Read { h, cred, block } => read_block(core, h, cred, *block, ok_ref),
        Work::WriteBlock { h, cred, block } => {
            write_block(core, h, cred, *block, ok_ref);
        }
        Work::Statfs { cred } => {
            let r = counted(core, core.fs.statfs(cred, &core.root), ok_ref);
            *core
                .statfs
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some((r, Instant::now()));
        }
        Work::Mutate { m, cred, token } => {
            let r = mutate(core, m, cred, ok_ref);
            if let Some((id, key)) = token {
                request::post(*id, key, r);
            }
        }
    }
    ok
}

fn getattr(core: &Core, h: &RHandle, cred: &Cred, ok: &mut bool) {
    let r = counted(core, core.fs.getattr(cred, h), ok);
    let mut c = core.cache();
    match r {
        Ok(a) => c.put_attr(h, a),
        Err(e) => c.put_err(h, "attr", e),
    }
}

fn fetch_access(core: &Core, h: &RHandle, cred: &Cred, ok: &mut bool) {
    let all = access::READ
        | access::LOOKUP
        | access::MODIFY
        | access::EXTEND
        | access::DELETE
        | access::EXECUTE;
    let r = counted(core, core.fs.access(cred, h, all), ok);
    let mut c = core.cache();
    match r {
        Ok(bits) => {
            c.inode(h)
                .access
                .insert(cred.clone(), (bits, Instant::now()));
        }
        Err(e) => c.put_err(h, &format!("access:{}", cred.uid), e),
    }
}

fn walk(core: &Core, dir: &RHandle, names: &[Vec<u8>], cred: &Cred, ok: &mut bool) {
    let mut dir = dir.clone();
    if core.cache().access(&dir, cred, Instant::now()).is_none() {
        fetch_access(core, &dir, cred, ok);
    }
    for (i, name) in names.iter().enumerate() {
        let r = counted(core, core.fs.lookup(cred, &dir, name), ok);
        match r {
            Ok((h, a)) => {
                let mut c = core.cache();
                c.put_attr(&h, a);
                c.put_name(&dir, name, Some(h.clone()));
                drop(c);
                if a.kind != RKind::Dir || i + 1 == names.len() {
                    return;
                }
                fetch_access(core, &h, cred, ok);
                dir = h;
            }
            Err(RemoteError::Errno(Errno::ENOENT)) => {
                core.cache().put_name(&dir, name, None);
                return;
            }
            Err(RemoteError::Stale) => {
                core.cache().put_err(&dir, "attr", RemoteError::Stale);
                return;
            }
            Err(e) => {
                core.cache().name_err(&dir, name, e);
                return;
            }
        }
    }
}

fn readdir(core: &Core, dir: &RHandle, cred: &Cred, ok: &mut bool) {
    let mut ents = Vec::new();
    let (mut cookie, mut verf) = (0u64, 0u64);
    let mut failed = None;
    for _ in 0..MAX_PAGES {
        match counted(core, core.fs.readdir(cred, dir, cookie, verf, 4096), ok) {
            Ok(p) => {
                let eof = p.eof || p.entries.is_empty();
                verf = p.verifier;
                for e in p.entries {
                    cookie = e.cookie;
                    if e.name == b"." || e.name == b".." {
                        continue;
                    }
                    let mut kind = e.attr.map(|a| a.kind);
                    let mut c = core.cache();
                    if let (Some(h), Some(a)) = (&e.handle, e.attr) {
                        c.put_attr(h, a);
                        c.put_name(dir, &e.name, Some(h.clone()));
                    } else if kind.is_none() {
                        drop(c);
                        // No attributes with the page: ask for the entry.
                        if let Ok((h, a)) = counted(core, core.fs.lookup(cred, dir, &e.name), ok) {
                            kind = Some(a.kind);
                            let mut c = core.cache();
                            c.put_attr(&h, a);
                            c.put_name(dir, &e.name, Some(h));
                        }
                    }
                    ents.push(ListEnt {
                        name: e.name,
                        fileid: e.fileid,
                        kind,
                    });
                }
                if eof {
                    break;
                }
            }
            Err(e) => {
                failed = Some(e);
                break;
            }
        }
    }
    let a = counted(core, core.fs.getattr(cred, dir), ok);
    let mut c = core.cache();
    match (failed, a) {
        (None, Ok(a)) => {
            c.put_attr(dir, a);
            c.inode(dir).listing = Some(Listing {
                ents,
                at: Instant::now(),
                dir_change: a.change,
            });
        }
        (Some(e), _) | (None, Err(e)) => c.put_err(dir, "list", e),
    }
}

fn read_block(core: &Core, h: &RHandle, cred: &Cred, k: u64, ok: &mut bool) {
    // Queued again just as another read brought it: nothing to do.
    let have = core
        .cache()
        .inodes
        .get(h)
        .and_then(|i| i.blocks.get(&k))
        .is_some_and(|b| b.base);
    if have {
        return;
    }
    let bs = core.bs();
    let len = u32::try_from(bs).unwrap_or(u32::MAX);
    let r = counted(core, core.fs.read(cred, h, k * bs, len), ok);
    apply_read(core, h, k, r);
}

/// Put a block the server sent in the cache.
pub(super) fn apply_read(core: &Core, h: &RHandle, k: u64, r: RResult<(Vec<u8>, bool)>) {
    let mut c = core.cache();
    match r {
        Ok((data, eof)) => {
            let change = c
                .inodes
                .get(h)
                .and_then(|i| i.attr)
                .map_or(0, |a| a.a.change);
            let n = data.len();
            let i = c.inode(h);
            let mut added = n;
            match i.blocks.get_mut(&k) {
                Some(b) => {
                    // Changes made here meanwhile stay on top.
                    let mut merged = data;
                    if let Some((a, z)) = b.dirty {
                        if merged.len() < z {
                            merged.resize(z, 0);
                        }
                        let top = z.min(b.data.len());
                        if a < top {
                            merged[a..top].copy_from_slice(&b.data[a..top]);
                        }
                    }
                    added = merged.len().saturating_sub(b.data.len());
                    b.data = merged;
                    b.base = true;
                    b.eof = eof;
                    b.at = Instant::now();
                    if b.state == BState::Clean {
                        b.change = change;
                    }
                }
                None => {
                    i.blocks.insert(
                        k,
                        Block {
                            data,
                            base: true,
                            at: Instant::now(),
                            change,
                            state: BState::Clean,
                            dirty: None,
                            epoch: 0,
                            eof,
                        },
                    );
                }
            }
            i.errs.remove(&format!("read:{k}"));
            c.bytes += added;
            c.trim();
        }
        Err(e) => c.put_err(h, &format!("read:{k}"), e),
    }
}

/// What one block has to send: its offset, bytes and generation.
pub(super) fn snapshot(core: &Core, h: &RHandle, k: u64) -> Option<(u64, Vec<u8>, u64)> {
    let bs = core.bs();
    let c = core.cache();
    let b = c.inodes.get(h)?.blocks.get(&k)?;
    if b.state != BState::Dirty {
        return None;
    }
    let (a, z) = b.dirty?;
    let z = z.min(b.data.len());
    Some((k * bs + a as u64, b.data[a..z].to_vec(), b.epoch))
}

/// Send `data` at `off` in pieces of `wsize`; the verifier and how far
/// it reached.
fn send(
    core: &Core,
    h: &RHandle,
    cred: &Cred,
    off: u64,
    data: &[u8],
    stable: Stable,
    ok: &mut bool,
) -> RResult<(u64, Stable)> {
    let ws = core.info.wsize.max(512) as usize;
    let mut done = 0;
    let (mut verf, mut reached) = (0, Stable::FileSync);
    while done < data.len() {
        let piece = &data[done..data.len().min(done + ws)];
        let r = counted(
            core,
            core.fs.write(cred, h, off + done as u64, piece, stable),
            ok,
        )?;
        if r.count == 0 {
            return Err(RemoteError::Errno(Errno::EIO));
        }
        verf = r.verifier;
        if r.committed == Stable::Unstable {
            reached = Stable::Unstable;
        }
        done += r.count as usize;
    }
    if data.is_empty() {
        reached = stable;
    }
    Ok((verf, reached))
}

/// Mark block `k` sent (or clean), unless it changed while it went.
pub(super) fn sent(core: &Core, h: &RHandle, k: u64, epoch: u64, r: RResult<(u64, Stable)>) {
    let mut c = core.cache();
    let i = c.inode(h);
    let mut gone = 0;
    match r {
        Ok((verf, reached)) => {
            if let Some(b) = i.blocks.get_mut(&k)
                && b.epoch == epoch
            {
                b.dirty = if reached == Stable::Unstable {
                    b.dirty
                } else {
                    None
                };
                b.state = if reached == Stable::Unstable {
                    BState::Sent(verf)
                } else {
                    BState::Clean
                };
                if b.state == BState::Clean && !b.base {
                    // Only part of it was ever here: no use for reads.
                    gone = i.blocks.remove(&k).map_or(0, |b| b.data.len());
                }
            }
        }
        Err(e) => {
            // The data cannot be written: dropped, the error kept for
            // `fsync` and `close`.
            gone = i.blocks.remove(&k).map_or(0, |b| b.data.len());
            i.write_err.get_or_insert(e);
        }
    }
    c.bytes = c.bytes.saturating_sub(gone);
    // Written data is clean data: past the limit, the oldest goes.
    c.trim();
}

fn write_block(core: &Core, h: &RHandle, cred: &Cred, k: u64, ok: &mut bool) {
    let Some((off, data, epoch)) = snapshot(core, h, k) else {
        return;
    };
    let stable = if core.info.sync_writes {
        Stable::FileSync
    } else {
        Stable::Unstable
    };
    let r = send(core, h, cred, off, &data, stable, ok);
    sent(core, h, k, epoch, r);
}

/// Write every dirty block of `h` and commit: blocks the server lost
/// (its write verifier changed) are sent again, stable. The first
/// write error comes back (and is cleared).
fn flush(core: &Core, h: &RHandle, cred: &Cred, ok: &mut bool) -> RResult<()> {
    let stable = if core.info.sync_writes {
        Stable::FileSync
    } else {
        Stable::Unstable
    };
    for _ in 0..8 {
        let ks: Vec<u64> = core
            .cache()
            .inodes
            .get(h)
            .map(|i| {
                i.blocks
                    .iter()
                    .filter(|(_, b)| b.state == BState::Dirty)
                    .map(|(k, _)| *k)
                    .collect()
            })
            .unwrap_or_default();
        if ks.is_empty() {
            break;
        }
        for k in ks {
            if let Some((off, data, epoch)) = snapshot(core, h, k) {
                let r = send(core, h, cred, off, &data, stable, ok);
                sent(core, h, k, epoch, r);
            }
        }
    }
    let unsent: Vec<(u64, u64)> = core
        .cache()
        .inodes
        .get(h)
        .map(|i| {
            i.blocks
                .iter()
                .filter_map(|(k, b)| match b.state {
                    BState::Sent(v) => Some((*k, v)),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    if !unsent.is_empty() {
        match counted(core, core.fs.commit(cred, h, 0, 0), ok) {
            Ok(v) => {
                for (k, vb) in unsent {
                    if vb == v {
                        let mut c = core.cache();
                        let i = c.inode(h);
                        let mut gone = 0;
                        if let Some(b) = i.blocks.get_mut(&k)
                            && b.state == BState::Sent(vb)
                        {
                            b.state = BState::Clean;
                            b.dirty = None;
                            if !b.base {
                                gone = i.blocks.remove(&k).map_or(0, |b| b.data.len());
                            }
                        }
                        c.bytes = c.bytes.saturating_sub(gone);
                        continue;
                    }
                    // The server restarted: send it again, stable.
                    let snap = {
                        let c = core.cache();
                        c.inodes
                            .get(h)
                            .and_then(|i| i.blocks.get(&k))
                            .and_then(|b| {
                                let (a, z) = b.dirty?;
                                let z = z.min(b.data.len());
                                Some((k * core.bs() + a as u64, b.data[a..z].to_vec(), b.epoch))
                            })
                    };
                    if let Some((off, data, epoch)) = snap {
                        let r = send(core, h, cred, off, &data, Stable::FileSync, ok);
                        sent(core, h, k, epoch, r);
                    }
                }
            }
            Err(e) => {
                core.cache().inode(h).write_err.get_or_insert(e);
            }
        }
    }
    let a = counted(core, core.fs.getattr(cred, h), ok);
    let mut c = core.cache();
    if let Ok(a) = a {
        c.put_attr(h, a);
    }
    c.trim();
    let i = c.inode(h);
    if !i.has_dirty() {
        i.size_local = None;
        i.mtime_local = None;
    }
    match i.write_err.take() {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// Look a directory up again after a change made here.
fn refresh_dir(core: &Core, dir: &RHandle, cred: &Cred, ok: &mut bool) {
    let a = counted(core, core.fs.getattr(cred, dir), ok).ok();
    core.cache().dir_changed(dir, a);
}

fn made(
    core: &Core,
    dir: &RHandle,
    name: &[u8],
    cred: &Cred,
    r: RResult<(RHandle, super::types::RAttr)>,
    ok: &mut bool,
) -> RResult<Answer> {
    let (h, a) = r?;
    refresh_dir(core, dir, cred, ok);
    let mut c = core.cache();
    c.put_attr(&h, a);
    c.put_name(dir, name, Some(h.clone()));
    Ok(Answer::Node(h, a))
}

#[allow(clippy::too_many_lines)]
fn mutate(core: &Core, m: &Mutation, cred: &Cred, ok: &mut bool) -> RResult<Answer> {
    let fs = &*core.fs;
    match m {
        Mutation::Setattr { h, set } => {
            if let Some(size) = set.size {
                // Data past the new end goes; what is below stays.
                let bs = core.bs();
                let mut c = core.cache();
                let i = c.inode(h);
                i.blocks.retain(|k, _| k * bs < size);
                if let Some(b) = i.blocks.get_mut(&(size / bs)) {
                    let keep = usize::try_from(size % bs).unwrap_or(0);
                    b.data.truncate(keep);
                    if let Some((a, z)) = b.dirty {
                        b.dirty = (a < keep).then_some((a, z.min(keep)));
                        if b.dirty.is_none() && b.state == BState::Dirty {
                            b.state = BState::Clean;
                        }
                    }
                }
                i.size_local = None;
                drop(c);
                // Writes below the new end go first.
                flush(core, h, cred, ok)?;
            }
            let a = counted(core, fs.setattr(cred, h, set, None), ok)?;
            let mut c = core.cache();
            c.put_attr(h, a);
            if set.size.is_some() {
                let i = c.inode(h);
                i.size_local = None;
            }
            Ok(Answer::Attr(a))
        }
        Mutation::Create { dir, name, how } => {
            let r = counted(core, fs.create(cred, dir, name, *how), ok);
            made(core, dir, name, cred, r, ok)
        }
        Mutation::Mkdir { dir, name, set } => {
            let r = counted(core, fs.mkdir(cred, dir, name, set), ok);
            made(core, dir, name, cred, r, ok)
        }
        Mutation::Symlink {
            dir,
            name,
            target,
            set,
        } => {
            let r = counted(core, fs.symlink(cred, dir, name, target, set), ok);
            made(core, dir, name, cred, r, ok)
        }
        Mutation::Mknod {
            dir,
            name,
            kind,
            set,
        } => {
            let r = counted(core, fs.mknod(cred, dir, name, *kind, set), ok);
            made(core, dir, name, cred, r, ok)
        }
        Mutation::Remove { dir, name, h } | Mutation::Rmdir { dir, name, h } => {
            let r = if matches!(m, Mutation::Remove { .. }) {
                counted(core, fs.remove(cred, dir, name), ok)
            } else {
                counted(core, fs.rmdir(cred, dir, name), ok)
            };
            r?;
            refresh_dir(core, dir, cred, ok);
            let mut c = core.cache();
            c.put_name(dir, name, None);
            if let Some(h) = h {
                if matches!(m, Mutation::Rmdir { .. }) {
                    c.forget(h);
                } else if let Some(i) = c.inodes.get_mut(h) {
                    // Other links may remain: ask again next time.
                    i.attr = None;
                }
            }
            Ok(Answer::Unit)
        }
        Mutation::Rename {
            from_dir,
            from,
            to_dir,
            to,
        } => {
            counted(core, fs.rename(cred, from_dir, from, to_dir, to), ok)?;
            let moved = {
                let c = core.cache();
                c.inodes
                    .get(from_dir)
                    .and_then(|d| d.names.get(from.as_slice()))
                    .and_then(|n| n.to.clone())
            };
            refresh_dir(core, from_dir, cred, ok);
            if to_dir != from_dir {
                refresh_dir(core, to_dir, cred, ok);
            }
            let mut c = core.cache();
            c.put_name(from_dir, from, None);
            match moved {
                Some(h) => {
                    c.put_name(to_dir, to, Some(h.clone()));
                    if let Some(i) = c.inodes.get_mut(&h) {
                        i.attr = None;
                    }
                }
                None => {
                    c.inode(to_dir).names.remove(to.as_slice());
                }
            }
            Ok(Answer::Unit)
        }
        Mutation::Link { h, dir, name } => {
            counted(core, fs.link(cred, h, dir, name), ok)?;
            refresh_dir(core, dir, cred, ok);
            let a = counted(core, fs.getattr(cred, h), ok);
            let mut c = core.cache();
            c.put_name(dir, name, Some(h.clone()));
            if let Ok(a) = a {
                c.put_attr(h, a);
            }
            Ok(Answer::Unit)
        }
        Mutation::Open { h, share, owner } => {
            counted(core, fs.open(cred, h, *share, *owner), ok).map(Answer::Token)
        }
        Mutation::Close { h, token } => {
            counted(core, fs.close(cred, h, token), ok).map(|()| Answer::Unit)
        }
        Mutation::Lock {
            h,
            owner,
            kind,
            off,
            len,
            token,
        } => counted(core, fs.lock(cred, h, *owner, *kind, *off, *len, token), ok)
            .map(|()| Answer::Unit),
        Mutation::Unlock { h, owner, off, len } => {
            counted(core, fs.unlock(cred, h, *owner, *off, *len), ok).map(|()| Answer::Unit)
        }
        Mutation::Test {
            h,
            owner,
            kind,
            off,
            len,
        } => counted(core, fs.test(cred, h, *owner, *kind, *off, *len), ok).map(Answer::Conflict),
        Mutation::Flush { h } => flush(core, h, cred, ok).map(|()| Answer::Unit),
        Mutation::FlushAll => {
            let hs: Vec<RHandle> = core
                .cache()
                .inodes
                .iter()
                .filter(|(_, i)| i.has_dirty())
                .map(|(h, _)| h.clone())
                .collect();
            let mut first = Ok(Answer::Unit);
            for h in hs {
                if let Err(e) = flush(core, &h, cred, ok)
                    && first.is_ok()
                {
                    first = Err(e);
                }
            }
            first
        }
        Mutation::Unmount => counted(core, fs.unmount(), ok).map(|()| Answer::Unit),
    }
}
