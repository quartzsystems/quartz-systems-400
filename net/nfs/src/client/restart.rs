// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! "A server this system mounts restarted": this system's status
//! monitor (`QNFSNSMD`) hears it from the server's (`SM_NOTIFY`) and
//! calls back this system's lock manager (`QNFSNLMD`, `NLM_SM_NOTIFY`
//! with [`super::nlm::PRIV_TAG`]); the lock manager's
//! `nlm::LockClient::restarted` hands it to the storage service, whose
//! client mounts are told through [`server_restarted`]: every version 3
//! mount of that server takes its locks again (`reclaim`), each on a
//! thread of its own.

use std::sync::{Arc, Mutex, PoisonError, Weak};

use super::nlm::Locker;

static MOUNTS: Mutex<Vec<Weak<Locker>>> = Mutex::new(Vec::new());

/// Remember a mount's lock client (forgotten when it is dropped).
pub(crate) fn register(l: &Arc<Locker>) {
    let mut m = MOUNTS.lock().unwrap_or_else(PoisonError::into_inner);
    m.retain(|w| w.strong_count() > 0);
    m.push(Arc::downgrade(l));
}

/// The server `name` (as its status monitor names itself, or its
/// address) restarted with `state`: the mounts of it reclaim their
/// locks in the background. How many mounts were told; `wait` makes
/// the reclaims finish first (tests).
pub fn server_restarted(name: &[u8], state: i32, wait: bool) -> usize {
    let found: Vec<Arc<Locker>> = MOUNTS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .filter_map(Weak::upgrade)
        .filter(|l| l.is_server(name))
        .collect();
    tracing::info!(
        server = %String::from_utf8_lossy(name),
        state,
        mounts = found.len(),
        "NFS server restarted"
    );
    let n = found.len();
    let threads: Vec<_> = found
        .into_iter()
        .filter_map(|l| {
            std::thread::Builder::new()
                .name("qs-nfs-reclaim".into())
                .spawn(move || l.reclaim())
                .ok()
        })
        .collect();
    if wait {
        for t in threads {
            let _ = t.join();
        }
    }
    n
}
