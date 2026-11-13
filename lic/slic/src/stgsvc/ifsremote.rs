// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Remote mounts (`MOUNT TYPE(*NFS)`, `ifs::remote`) in the service. A
//! remote file system's calls run on its mount's threads; a request
//! that needs one is parked by the loop (`super::turns`) as a lease
//! wait is, and run again with its token when the work is done. The
//! mounts' threads wake the loop through a socket pair whose reading
//! end it polls ([`wake_fd`]). A server that stops answering, and
//! answers again, is reported to `QSYSOPR` (`CPDA1B9`, reason 5 and 7)
//! from the loop ([`tick`]). The NFS client registers itself with
//! [`set_factory`] (`super::nfs::client`); without it `MOUNT TYPE(*NFS)` is
//! `CPFA0AD`.

use std::io::{Read as _, Write as _};
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use ifs::remote::{RemoteConfig, RemoteEvent, RemoteFactory, tickets};

use crate::ipl::system::System;

/// `QSYSOPR`'s message about a remote mount's server (IBM's NFS error
/// message, with its reason code).
pub const SERVER_MSG: &str = "CPDA1B9";
/// Reason: the server is not responding, still trying.
const NOT_RESPONDING: u32 = 5;
/// Reason: the server responded.
const RESPONDED: u32 = 7;

/// Worker threads per mount.
pub(crate) const THREADS: usize = 4;

static WAKE: OnceLock<(UnixStream, UnixStream)> = OnceLock::new();
static EVENTS: Mutex<Vec<RemoteEvent>> = Mutex::new(Vec::new());

/// Set up the waker and the events, once the IFS is built.
pub fn start() {
    let Some(ifs) = super::ifs::get() else {
        return;
    };
    let Ok((r, w)) = UnixStream::pair() else {
        eprintln!("qsstg: no waker for remote mounts");
        return;
    };
    let _ = r.set_nonblocking(true);
    let _ = w.set_nonblocking(true);
    if WAKE.set((r, w)).is_err() {
        return;
    }
    tickets::set_waker(Some(Arc::new(|| {
        if let Some((_, w)) = WAKE.get() {
            // Full: the loop is woken already.
            let _ = (&*w).write(&[1]);
        }
    })));
    ifs.set_remote_events(Some(Arc::new(|e: &RemoteEvent| {
        EVENTS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(e.clone());
        tickets::wake();
    })));
}

/// The hook the NFS client fills: what mounts `TYPE(*NFS)`.
pub fn set_factory(f: Option<Arc<dyn RemoteFactory>>) {
    if let Some(ifs) = super::ifs::get() {
        ifs.set_remote_factory(
            f,
            RemoteConfig {
                threads: THREADS,
                ..RemoteConfig::default()
            },
        );
    }
}

/// What the loop polls to hear of finished remote work.
pub fn wake_fd() -> Option<BorrowedFd<'static>> {
    WAKE.get().map(|(r, _)| r.as_fd())
}

/// Empty the waker after the loop woke.
pub fn drain() {
    if let Some((r, _)) = WAKE.get() {
        let mut buf = [0u8; 64];
        while matches!((&*r).read(&mut buf), Ok(n) if n > 0) {}
    }
}

/// Report what the mounts said of their servers to `QSYSOPR`.
pub fn tick(system: &mut System) {
    let evs = std::mem::take(&mut *EVENTS.lock().unwrap_or_else(PoisonError::into_inner));
    for e in evs {
        let (reason, source) = match &e {
            RemoteEvent::NotResponding { source, .. } => (NOT_RESPONDING, source),
            RemoteEvent::Responding { source, .. } => (RESPONDED, source),
        };
        let text = super::msgs::text(system, SERVER_MSG, &[&reason.to_string()]);
        eprintln!("qsstg: remote mount of {source}: {SERVER_MSG} reason {reason}");
        let m = wm::obj::msgq::QMsg {
            msgid: SERVER_MSG.into(),
            msgf: ("QCPFMSG".into(), "QSYS".into()),
            severity: 20,
            data: reason.to_be_bytes().to_vec(),
            text,
            job: ("SCPF".into(), "QSYS".into(), 0),
            from_pgm: "QSYSARB".into(),
            ..wm::obj::msgq::QMsg::default()
        };
        let _ = super::msgs::send(system, "QSYS", "QSYSOPR", m);
    }
}

/// Job `number` is ending (`ENDJOB`): its requests waiting on an `intr`
/// mount answer `EINTR`.
pub fn interrupt_job(number: u32) {
    if let Some(ifs) = super::ifs::get() {
        let h = u64::from(number);
        ifs.interrupt_owner(super::ifs::owner_of(h));
        ifs.interrupt_owner(super::ifs::console_owner(h));
    }
}
