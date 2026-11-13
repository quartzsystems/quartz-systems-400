// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QNFSBIOD` (`NBRBIO` jobs): the NFS client's block I/O helpers
//! (docs/NFS.md section 7.6). Each takes the read-ahead and
//! write-behind blocks the mounts queue (`IfsClient::remote_take`),
//! makes the call to the block's server over a connection of its own
//! and posts the answer (`remote_post`). Without work it waits, longer
//! each time it finds none (50 ms doubling to a second). The mounts'
//! own threads take the same work, so a helper only adds speed.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use ifs::client::IfsClient;
use ifs::remote::{HelperResult, HelperWork};

use super::host::Host;
use super::running::{Fail, Running};
use super::{DaemonEnv, Serve, Started};

/// The shortest and the longest wait between looks for work.
const FIRST_WAIT: Duration = Duration::from_millis(50);
const LONGEST_WAIT: Duration = Duration::from_secs(1);

/// What carries out one block: the NFS call to the work's server.
pub type Io = Box<dyn FnMut(&HelperWork) -> HelperResult>;

/// The block I/O this job can do, or `None` when it can do none: then
/// it takes no work, so the mounts' own threads do it all.
#[must_use]
pub fn block_io() -> Option<Io> {
    // BLOCKED(rel): the NFS client in the block I/O daemon (NFS-7): an Io making each READ and WRITE over nfs::client to HelperWork::source.
    None
}

/// Take and carry out work until `stop`.
pub fn helper_loop(fs: &dyn IfsClient, io: Option<Io>, stop: &AtomicBool) {
    let mut wait = FIRST_WAIT;
    let Some(mut io) = io else {
        while !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(100));
        }
        return;
    };
    while !stop.load(Ordering::Relaxed) {
        match fs.remote_take() {
            Ok(Some(w)) => {
                let r = io(&w);
                if let Err(e) = fs.remote_post(w.fs, w.id, &r) {
                    eprintln!("qsjob: QNFSBIOD: posting block {}: {}", w.id, e.text());
                }
                wait = FIRST_WAIT;
                continue;
            }
            Ok(None) => {}
            Err(e) => {
                eprintln!("qsjob: QNFSBIOD: taking work: {}", e.text());
                wait = LONGEST_WAIT;
            }
        }
        let mut slept = Duration::ZERO;
        while slept < wait && !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(10));
            slept += Duration::from_millis(10);
        }
        wait = (wait * 2).min(LONGEST_WAIT);
    }
}

/// Serve: the helper loop on a thread with a link of its own.
pub fn serve<H: Host>(_env: &mut dyn DaemonEnv, _started: Started, host: &Arc<H>) -> Serve {
    let h = Arc::clone(host);
    let r = Running::spawn("nfs-biod", move |stop, ready| {
        let fs = h.fs().map_err(Fail::new)?;
        ready();
        helper_loop(&fs, block_io(), stop);
        Ok(())
    });
    match r {
        Ok(r) => Serve::Ready(r),
        Err(f) => Serve::Unavailable(f),
    }
}
