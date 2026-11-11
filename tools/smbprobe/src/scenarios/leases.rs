// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Leases across two machines' jobs, a lease broken by a writer outside
//! SMB, and a durable handle taken back on a new connection.

use std::io::Write as _;
use std::sync::Arc;
use std::time::{Duration, Instant};

use smb_server::ntstatus::{STATUS_LOCK_NOT_GRANTED, STATUS_SUCCESS};
use smb_server::wire::messages::{CreateResponse, FileId, LeaseBreakNotification};

use super::Done;
use crate::conn::{self, SMB302, Target, expect};
use crate::report::status;
use crate::smbc::client::{
    Client, Create, FILE_OPEN, LOCK_EXCLUSIVE, LOCK_FAIL_IMMEDIATELY, LOCK_UNLOCK, RW, SHARE_ALL,
};
use crate::smbc::durable::{contexts, dh2c, dh2q, durable_reply, lease_v2, reset};
use crate::smbc::lease::{LEASE_WRITE_CACHING, OPLOCK_LEVEL_LEASE, RWH, lease_reply};

/// Read and handle caching.
const RH: u32 = RWH & !LEASE_WRITE_CACHING;
/// How long a break is waited for.
const BREAK_WAIT: Duration = Duration::from_secs(20);
/// How long the harness's writer is waited for.
const OUTSIDE_WAIT: Duration = Duration::from_secs(90);

/// A lease key no other open of this run uses.
fn key(t: &Target) -> [u8; 16] {
    let mut k = t.machine();
    k[12] = 0x4C;
    k
}

/// Opens `path` asking for lease `state` under `key`, with `more`
/// contexts after it.
async fn leased(
    c: &mut Client,
    path: &str,
    key: [u8; 16],
    state: u32,
    more: Vec<smb_server::wire::messages::CreateContext>,
) -> Result<(CreateResponse, u32), String> {
    let mut list = vec![lease_v2(key, state)];
    list.extend(more);
    let r = c
        .create(Create {
            name: path,
            oplock: OPLOCK_LEVEL_LEASE,
            disposition: FILE_OPEN,
            contexts: contexts(&list),
            ..Create::default()
        })
        .await
        .map_err(|e| format!("leased open of {path}: {}", status(e)))?;
    let granted = lease_reply(&r).map_or(0, |l| l.state);
    Ok((r, granted))
}

/// The next break, or why there was none.
async fn next_break(
    c: &mut Client,
    within: Duration,
    what: &str,
) -> Result<LeaseBreakNotification, String> {
    c.next_break(within)
        .await
        .ok_or_else(|| format!("no lease break for {what} in {}s", within.as_secs()))
}

/// Acknowledges `n` when it asks for it.
async fn ack(c: &mut Client, key: [u8; 16], n: &LeaseBreakNotification) -> Result<bool, String> {
    if n.flags & LeaseBreakNotification::FLAG_ACK_REQUIRED == 0 {
        return Ok(false);
    }
    expect(
        "lease break acknowledgement",
        c.ack(key, n.new_lease_state).await,
        STATUS_SUCCESS,
    )?;
    Ok(true)
}

fn state(s: u32) -> String {
    let mut out = String::new();
    for (bit, c) in [(1, 'R'), (4, 'W'), (2, 'H')] {
        if s & bit != 0 {
            out.push(c);
        }
    }
    if out.is_empty() {
        out.push_str("none");
    }
    out
}

/// A read of `n` bytes at 0.
async fn read(c: &mut Client, id: FileId, n: u32, what: &str) -> Result<Vec<u8>, String> {
    c.read(id, 0, n)
        .await
        .map_err(|e| format!("{what}: {}", status(e)))
}

/// An RWH lease from machine A's job; machine B's open breaks it to RH
/// and waits while A writes back what it cached and acknowledges; while B
/// has the file open a new lease from A caches no writes; B's write breaks
/// the rest.
pub async fn lease_break(t: Arc<Target>) -> Done {
    let mut a = conn::client(&t, SMB302).await?;
    conn::new_file(&t, &mut a, "lease.txt", b"").await?;
    let path = t.file("lease.txt");
    let ka = key(&t);
    let (r, granted) = leased(&mut a, &path, ka, RWH, Vec::new()).await?;
    if granted != RWH {
        return Err(format!("granted {} where RWH was asked", state(granted)));
    }
    let mut b = conn::client(&t, SMB302).await?;
    let p = path.clone();
    let opening = tokio::spawn(async move {
        let id = b.open(&p, RW, SHARE_ALL).await;
        (b, id)
    });
    let n = next_break(&mut a, BREAK_WAIT, "the other job's open").await?;
    if (n.current_lease_state, n.new_lease_state) != (RWH, RH) {
        return Err(format!(
            "the open broke {} to {}, not RWH to RH",
            state(n.current_lease_state),
            state(n.new_lease_state)
        ));
    }
    if n.flags & LeaseBreakNotification::FLAG_ACK_REQUIRED == 0 {
        return Err("a break of write caching without an acknowledgement asked".into());
    }
    // What the client cached goes back before the acknowledgement.
    conn::write_all(&mut a, r.file_id, b"cached").await?;
    ack(&mut a, ka, &n).await?;
    let (mut b, idb) = opening.await.map_err(|e| format!("the other open: {e}"))?;
    let idb = idb.map_err(|e| format!("the other job's open: {}", status(e)))?;
    let seen = read(&mut b, idb, 6, "the other job's read").await?;
    if seen != b"cached" {
        return Err(format!(
            "the other job read {seen:?}, not the written-back data"
        ));
    }

    // While B has it open, a new lease caches no writes.
    let (r2, second) = leased(&mut a, &path, key(&t), RWH, Vec::new()).await?;
    if second & LEASE_WRITE_CACHING != 0 {
        return Err(format!(
            "granted {} while another job has the file open",
            state(second)
        ));
    }
    a.close(r2.file_id).await;

    // B's write breaks read caching.
    let writing = tokio::spawn(async move {
        let w = b.write(idb, 0, b"C").await;
        (b, w)
    });
    let n2 = next_break(&mut a, BREAK_WAIT, "the other job's write").await?;
    if n2.current_lease_state != RH || n2.new_lease_state & 1 != 0 {
        return Err(format!(
            "the write broke {} to {}",
            state(n2.current_lease_state),
            state(n2.new_lease_state)
        ));
    }
    let acked = ack(&mut a, ka, &n2).await?;
    let (mut b, w) = writing.await.map_err(|e| format!("the other write: {e}"))?;
    w.map_err(|e| format!("the other job's write: {}", status(e)))?;
    let now = read(&mut a, r.file_id, 6, "read after the break").await?;
    if now != b"Cached" {
        return Err(format!("read {now:?} after the other job's write"));
    }
    b.close(idb).await;
    a.close(r.file_id).await;
    Ok(vec![
        "granted RWH; the other job's open: RWH to RH, written back, acknowledged".into(),
        format!(
            "a new lease while the other job has it open: {}",
            state(second)
        ),
        format!(
            "the other job's write: RH to {} (acknowledged: {})",
            state(n2.new_lease_state),
            if acked { "yes" } else { "not asked" }
        ),
    ])
}

/// An RWH lease broken by a writer outside SMB: the probe prints `ready
/// lease-break-ifs PATH` (the file relative to the share) and waits; the
/// harness writes `--ifs-text` into it from the system; each break is
/// acknowledged, and the file then reads as written.
pub async fn lease_break_ifs(t: Arc<Target>) -> Done {
    let want = format!("{}\n", t.opt.ifs_text.as_deref().unwrap_or_default());
    let mut a = conn::client(&t, SMB302).await?;
    conn::new_file(&t, &mut a, "ifs.txt", b"before").await?;
    let path = t.file("ifs.txt");
    let ka = key(&t);
    let (r, granted) = leased(&mut a, &path, ka, RWH, Vec::new()).await?;
    if granted != RWH {
        return Err(format!("granted {} where RWH was asked", state(granted)));
    }
    println!("ready lease-break-ifs {}", path.replace('\\', "/"));
    let _ = std::io::stdout().flush();
    let mut steps = Vec::new();
    let mut within = OUTSIDE_WAIT;
    loop {
        let n = next_break(&mut a, within, "the outside writer").await?;
        let acked = ack(&mut a, ka, &n).await?;
        steps.push(format!(
            "{} to {}{}",
            state(n.current_lease_state),
            state(n.new_lease_state),
            if acked { " (acknowledged)" } else { "" }
        ));
        if n.new_lease_state & 1 == 0 {
            break;
        }
        within = BREAK_WAIT;
    }
    let end = Instant::now() + BREAK_WAIT;
    let seen = loop {
        let seen = read(&mut a, r.file_id, 256, "read after the breaks").await?;
        if seen == want.as_bytes() || Instant::now() > end {
            break seen;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    };
    a.close(r.file_id).await;
    if seen != want.as_bytes() {
        return Err(format!(
            "after the breaks the file reads {:?}, not {want:?}",
            String::from_utf8_lossy(&seen)
        ));
    }
    Ok(vec![
        format!("breaks: {}", steps.join(", ")),
        format!("reads the outside write: {:?}", want.trim_end()),
    ])
}

/// A durable handle (version 2, with an RWH lease) and a lock; the TCP
/// connection is reset; a new connection from the same `ClientGuid` signs
/// on naming the old session and takes the handle back by its `CreateGuid`.
/// It reads the file, its lock still refuses another job, and it unlocks
/// it, so the open, its lock and the job are the ones from before.
pub async fn durable_reconnect(t: Arc<Target>) -> Done {
    let guid = t.machine();
    let create_guid = {
        let mut g = t.machine();
        g[12] = 0x43;
        g
    };
    let user = (t.user.as_str(), t.password.as_str());
    let mut a = conn::client_as(&t, guid, SMB302, user, &t.share).await?;
    conn::new_file(&t, &mut a, "durable.txt", b"durable").await?;
    let path = t.file("durable.txt");
    let k = key(&t);
    let (r, granted) = leased(&mut a, &path, k, RWH, vec![dh2q(60_000, 0, create_guid)]).await?;
    if durable_reply(&r).is_none() {
        return Err(format!(
            "no durable handle granted (lease {})",
            state(granted)
        ));
    }
    expect(
        "lock 0-4",
        a.lock(r.file_id, &[(0, 4, LOCK_EXCLUSIVE)]).await,
        STATUS_SUCCESS,
    )?;
    let old_session = a.session_id;
    reset(a);

    // The server sees the reset at its next read; retry for a while.
    let end = Instant::now() + Duration::from_secs(20);
    let mut tries = 0;
    let (mut c, back) = loop {
        tries += 1;
        let (mut s, l) = conn::session(&t, guid, SMB302, user.0, user.1, old_session).await?;
        let (st, tree_id, _) = conn::tree(&t, &mut s, l.session_id, &t.share).await;
        expect("tree connect after the reset", st, STATUS_SUCCESS)?;
        let mut c = Client::from_parts(s, l.session_id, tree_id);
        let got = c
            .create(Create {
                name: &path,
                oplock: OPLOCK_LEVEL_LEASE,
                disposition: FILE_OPEN,
                contexts: contexts(&[lease_v2(k, RWH), dh2c(r.file_id, create_guid, 0)]),
                ..Create::default()
            })
            .await;
        match got {
            Ok(back) => break (c, back),
            Err(e) if Instant::now() > end => {
                return Err(format!("reconnect ({tries} tries): {}", status(e)));
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(500)).await,
        }
    };
    let id = back.file_id;
    let same = id == r.file_id;
    let data = read(&mut c, id, 7, "read on the handle taken back").await?;
    if data != b"durable" {
        return Err(format!("the handle taken back reads {data:?}"));
    }
    // Another job's open breaks the handle's write caching: answered here.
    let mut other = conn::client(&t, SMB302).await?;
    let p = path.clone();
    let opening = tokio::spawn(async move {
        let id = other.open(&p, RW, SHARE_ALL).await;
        (other, id)
    });
    if let Some(n) = c.next_break(BREAK_WAIT).await {
        ack(&mut c, k, &n).await?;
    }
    let (mut other, oid) = opening.await.map_err(|e| format!("another open: {e}"))?;
    let oid = oid.map_err(|e| format!("another job's open: {}", status(e)))?;
    let flags = LOCK_EXCLUSIVE | LOCK_FAIL_IMMEDIATELY;
    expect(
        "another job's lock while the handle holds it",
        other.lock(oid, &[(0, 4, flags)]).await,
        STATUS_LOCK_NOT_GRANTED,
    )?;
    expect(
        "unlock on the handle taken back",
        c.lock(id, &[(0, 4, LOCK_UNLOCK)]).await,
        STATUS_SUCCESS,
    )?;
    expect(
        "another job's lock after the unlock",
        other.lock(oid, &[(0, 4, flags)]).await,
        STATUS_SUCCESS,
    )?;
    other.close(oid).await;
    c.close(id).await;
    Ok(vec![
        format!(
            "taken back after {tries} tries; same FileId: {same}; lease {}",
            state(granted)
        ),
        "its lock held against another job, then unlocked by the handle".into(),
    ])
}
