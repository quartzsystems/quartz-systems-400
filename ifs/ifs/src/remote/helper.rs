// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Block I/O helpers (`QNFSBIOD`): a helper job takes read-ahead and
//! write-behind work from a mount's queue ([`RemoteMount::take_helper`]),
//! makes the call over its own connection to the server, and posts what
//! came back ([`RemoteMount::post_helper`]). Work a helper does not post
//! within [`LEND`] goes back to the queue. Without helpers the mount's
//! own threads do all of it; with them they share it.

// Block, handle, cache, queue: the short names are the domain's.
#![allow(clippy::many_single_char_names)]

use std::time::{Duration, Instant};

use super::mount::RemoteMount;
use super::tickets;
use super::types::{Cred, RemoteError, Stable};
use super::work::Work;
use super::worker;
use crate::errno::{Errno, Result};

/// How long a helper may keep work.
pub const LEND: Duration = Duration::from_secs(30);

/// What a helper is asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelperOp {
    /// Read `len` bytes at `off`.
    Read,
    /// Write `data` at `off`, unstable (or file-sync on a mount that
    /// writes synchronously).
    Write,
}

/// One piece of work lent to a helper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelperWork {
    /// What to post it back as.
    pub id: u64,
    /// The mount.
    pub fs: u32,
    /// What is mounted (`host:/path`): where to send the call.
    pub source: String,
    /// Read or write.
    pub op: HelperOp,
    /// The file's handle.
    pub handle: Vec<u8>,
    /// Where.
    pub off: u64,
    /// How much to read.
    pub len: u32,
    /// What to write.
    pub data: Vec<u8>,
    /// Write file-sync.
    pub sync: bool,
    /// As whom.
    pub uid: u32,
    /// Primary group.
    pub gid: u32,
    /// Every group.
    pub gids: Vec<u32>,
}

/// What a helper posts back.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HelperResult {
    /// The bytes read.
    pub data: Vec<u8>,
    /// The read reached the end.
    pub eof: bool,
    /// Bytes written.
    pub count: u32,
    /// The write reached stable storage.
    pub stable: bool,
    /// The server's write verifier.
    pub verifier: u64,
    /// The call failed so.
    pub err: Option<Errno>,
}

fn cred_of(c: &Cred) -> (u32, u32, Vec<u32>) {
    (c.uid, c.gid, c.gids.clone())
}

impl RemoteMount {
    /// Lend the first read-ahead or write-behind work to a helper.
    pub fn take_helper(&self) -> Option<HelperWork> {
        let core = &self.core;
        loop {
            let mut q = core.queue();
            let i = q.items.iter().position(|(_, w)| w.for_helper())?;
            let (t, w) = q.items.remove(i)?;
            q.next_lent += 1;
            let id = q.next_lent;
            let (op, h, cred, block) = match &w {
                Work::Read { h, cred, block } => (HelperOp::Read, h.clone(), cred.clone(), *block),
                Work::WriteBlock { h, cred, block } => {
                    (HelperOp::Write, h.clone(), cred.clone(), *block)
                }
                _ => unreachable!("for_helper"),
            };
            let bs = core.bs();
            let (off, data) = if op == HelperOp::Write {
                drop(q);
                let Some((off, data, epoch)) = worker::snapshot(core, &h, block) else {
                    // Written meanwhile: nothing to lend.
                    let mut q = core.queue();
                    q.inflight.remove(&w);
                    drop(q);
                    tickets::finish(t);
                    continue;
                };
                q = core.queue();
                q.lent.insert(id, (t, w.clone(), Instant::now() + LEND));
                q.gens.insert(id, epoch);
                (off, data)
            } else {
                q.lent.insert(id, (t, w.clone(), Instant::now() + LEND));
                (block * bs, Vec::new())
            };
            let (uid, gid, gids) = cred_of(&cred);
            return Some(HelperWork {
                id,
                fs: self.id(),
                source: core.info.source.clone(),
                op,
                handle: h.0,
                off,
                len: u32::try_from(bs).unwrap_or(u32::MAX),
                data,
                sync: core.info.sync_writes,
                uid,
                gid,
                gids,
            });
        }
    }

    /// A helper's answer to work it took.
    pub fn post_helper(&self, id: u64, r: &HelperResult) -> Result<()> {
        let core = &self.core;
        let (t, w, epoch) = {
            let mut q = core.queue();
            let (t, w, _) = q.lent.remove(&id).ok_or(Errno::ENOENT)?;
            let epoch = q.gens.remove(&id).unwrap_or(0);
            (t, w, epoch)
        };
        let failed = r.err.map(RemoteError::from);
        match &w {
            Work::Read { h, block, .. } => {
                let got = failed.map_or_else(|| Ok((r.data.clone(), r.eof)), Err);
                worker::apply_read(core, h, *block, got);
            }
            Work::WriteBlock { h, block, .. } => {
                let reached = if r.stable {
                    Stable::FileSync
                } else {
                    Stable::Unstable
                };
                let got = failed.map_or(Ok((r.verifier, reached)), Err);
                worker::sent(core, h, *block, epoch, got);
            }
            _ => {}
        }
        core.queue().inflight.remove(&w);
        worker::answered_now(core, failed != Some(RemoteError::TimedOut));
        tickets::finish(t);
        Ok(())
    }
}
