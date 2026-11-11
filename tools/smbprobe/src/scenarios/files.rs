// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Share enumeration over srvsvc, byte-range locks across two jobs, and
//! the run directory's removal.

use std::sync::Arc;

use smb_server::ntstatus::{STATUS_FILE_LOCK_CONFLICT, STATUS_LOCK_NOT_GRANTED, STATUS_SUCCESS};

use super::Done;
use crate::conn::{self, SMB302, Target, expect};
use crate::report::status;
use crate::smbc::client::{
    Create, DELETE, FILE_DIRECTORY_FILE, FILE_GENERIC_READ, FILE_OPEN, LOCK_EXCLUSIVE,
    LOCK_FAIL_IMMEDIATELY, LOCK_UNLOCK, RW, SHARE_ALL,
};
use crate::smbc::rpc::{STYPE_DISKTREE, ack_results};

/// `STYPE_PRINTQ`.
const STYPE_PRINTQ: u32 = 1;

/// `NetrShareEnum` level 1 over `\pipe\srvsvc` on IPC$, bound as Windows
/// binds (NDR 2.0, NDR64 and the feature negotiation together): the file
/// share as a disk and the print share as a print queue.
pub async fn share_enum(t: Arc<Target>) -> Done {
    let mut c = conn::client_as(&t, t.machine(), SMB302, (&t.user, &t.password), "IPC$").await?;
    let (pipe, ack) = c.bound_pipe().await;
    let results = ack_results(&ack);
    if results.first().map(|r| r.0) != Some(0) {
        return Err(format!("NDR 2.0 not accepted: {results:?}"));
    }
    let reply = c.list_shares(pipe, 1).await;
    expect("NetrShareEnum", reply.status, STATUS_SUCCESS)?;
    let kind = |name: &str| {
        reply
            .entries
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(name))
            .and_then(|e| e.kind)
    };
    if kind(&t.share) != Some(STYPE_DISKTREE) {
        return Err(format!(
            "{} not a disk share in {:?}",
            t.share,
            reply.names()
        ));
    }
    if let Some(p) = &t.opt.print_share
        && kind(p) != Some(STYPE_PRINTQ)
    {
        return Err(format!("{p} not a print queue in {:?}", reply.names()));
    }
    Ok(vec![
        format!("bind results: {results:?}"),
        format!("shares: {}", reply.names().join(", ")),
    ])
}

/// An exclusive lock taken by one machine's job holds against another's:
/// its lock is not granted (its read is served: IFS locks are advisory
/// across jobs), the locking job's other open cannot read the range, and
/// once unlocked the other job takes it.
pub async fn lock_conflict(t: Arc<Target>) -> Done {
    let mut a = conn::client(&t, SMB302).await?;
    conn::new_file(&t, &mut a, "lock.txt", b"0123456789abcdef").await?;
    let path = t.file("lock.txt");
    let ida = a
        .open(&path, RW, SHARE_ALL)
        .await
        .map_err(|e| format!("open: {}", status(e)))?;
    expect(
        "lock 0-8",
        a.lock(ida, &[(0, 8, LOCK_EXCLUSIVE)]).await,
        STATUS_SUCCESS,
    )?;

    let mut b = conn::client(&t, SMB302).await?;
    let idb = b
        .open(&path, RW, SHARE_ALL)
        .await
        .map_err(|e| format!("second job's open: {}", status(e)))?;
    let flags = LOCK_EXCLUSIVE | LOCK_FAIL_IMMEDIATELY;
    expect(
        "second job's lock 4-8",
        b.lock(idb, &[(4, 4, flags)]).await,
        STATUS_LOCK_NOT_GRANTED,
    )?;
    // IFS locks are advisory across jobs (`docs/NETSERVER.md` section 6):
    // the other job's read is served; a second open in the locking job
    // meets the library's own lock.
    let across = b.read(idb, 0, 4).await;
    if across.as_deref() != Ok(b"0123".as_slice()) {
        return Err(format!("the other job's read of 0-4: {across:?}"));
    }
    let ida2 = a
        .open(&path, RW, SHARE_ALL)
        .await
        .map_err(|e| format!("second open in the job: {}", status(e)))?;
    let within = a.read(ida2, 0, 4).await;
    a.close(ida2).await;
    expect(
        "the same job's other open reading 0-4",
        within.err().unwrap_or(STATUS_SUCCESS),
        STATUS_FILE_LOCK_CONFLICT,
    )?;
    let outside = b
        .read(idb, 8, 8)
        .await
        .map_err(|e| format!("read outside the lock: {}", status(e)))?;
    if outside != b"89abcdef" {
        return Err(format!("read outside the lock: {outside:?}"));
    }
    expect(
        "unlock",
        a.lock(ida, &[(0, 8, LOCK_UNLOCK)]).await,
        STATUS_SUCCESS,
    )?;
    expect(
        "second job's lock after the unlock",
        b.lock(idb, &[(4, 4, flags)]).await,
        STATUS_SUCCESS,
    )?;
    expect(
        "first job's lock now",
        a.lock(ida, &[(0, 8, flags)]).await,
        STATUS_LOCK_NOT_GRANTED,
    )?;
    b.close(idb).await;
    a.close(ida).await;
    Ok(vec![
        "the other job: lock STATUS_LOCK_NOT_GRANTED, read served (advisory)".into(),
        "the same job's other open: read STATUS_FILE_LOCK_CONFLICT".into(),
        "unlocked: taken by the other job".into(),
    ])
}

/// Removes the run directory and what is in it.
pub async fn cleanup(t: Arc<Target>) -> Done {
    let mut c = conn::client(&t, SMB302).await?;
    let dir = t.dir();
    let id = match c
        .create(Create {
            name: &dir,
            access: FILE_GENERIC_READ,
            share: SHARE_ALL,
            disposition: FILE_OPEN,
            options: FILE_DIRECTORY_FILE,
            ..Create::default()
        })
        .await
    {
        Ok(r) => r.file_id,
        Err(_) => return Ok(vec!["nothing to remove".into()]),
    };
    let names = c.list(id, 0, "*").await.unwrap_or_default();
    c.close(id).await;
    let mut removed = 0;
    for n in names.iter().filter(|n| *n != "." && *n != "..") {
        let path = format!(r"{dir}\{n}");
        if let Ok(id) = c.open(&path, DELETE, SHARE_ALL).await {
            if c.set_delete(id, true).await == STATUS_SUCCESS {
                removed += 1;
            }
            let _ = c.close_with(id, 0).await;
        }
    }
    let gone = match c
        .create(Create {
            name: &dir,
            access: DELETE,
            share: SHARE_ALL,
            disposition: FILE_OPEN,
            options: FILE_DIRECTORY_FILE,
            ..Create::default()
        })
        .await
    {
        Ok(r) => {
            let st = c.set_delete(r.file_id, true).await;
            let _ = c.close_with(r.file_id, 0).await;
            st
        }
        Err(e) => e,
    };
    expect("removing the run directory", gone, STATUS_SUCCESS)?;
    Ok(vec![format!("{removed} files and {dir} removed")])
}
