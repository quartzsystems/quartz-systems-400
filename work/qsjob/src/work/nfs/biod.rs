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

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use ifs::Errno;
use ifs::client::IfsClient;
use ifs::remote::{
    Cred, HelperOp, HelperResult, HelperWork, MountCcsid, RHandle, RemoteError, RemoteFs, Stable,
};
use nfs::client::{ClientEnv, Mount};
use nfs::exports::MountOptions;

use super::host::Host;
use super::running::{Fail, Running};
use super::{DaemonEnv, Serve, Started};

/// The shortest and the longest wait between looks for work.
const FIRST_WAIT: Duration = Duration::from_millis(50);
const LONGEST_WAIT: Duration = Duration::from_secs(1);

/// What carries out one block: the NFS call to the work's server.
pub type Io = Box<dyn FnMut(&HelperWork) -> HelperResult>;

/// Host names for the client: the system's (`names::HostNames`).
struct Names(Box<dyn nfs::exports::Resolver + Send + Sync>);

impl nfs::client::Resolver for Names {
    fn resolve(&self, host: &str) -> Vec<std::net::IpAddr> {
        self.0.addresses(host)
    }
}

/// A helper's connections, one per mount it served, kept and reused.
pub struct Helper {
    /// A mount's options as the IFS reports them (`IfsClient::mounts`).
    options: Options,
    env: ClientEnv,
    mounts: HashMap<(u32, String), Mount>,
}

/// A mount's options by its ID.
pub type Options = Box<dyn Fn(u32) -> Result<String, Errno>>;

impl Helper {
    /// A helper finding a mount's options with `options`, connecting
    /// with `env`.
    #[must_use]
    pub fn new(options: Options, env: ClientEnv) -> Self {
        Self {
            options,
            env,
            mounts: HashMap::new(),
        }
    }

    /// The options of mount `id`, soft: a helper that cannot reach the
    /// server gives the block back failed rather than holding it.
    fn options(&self, id: u32) -> Result<MountOptions, Errno> {
        let text = (self.options)(id)?;
        let mut o = nfs::exports::mount::parse(&text).map_err(|_| Errno::EINVAL)?;
        o.hard = false;
        o.retry = 1;
        Ok(o)
    }

    /// The connection to `w`'s server, made when there is none.
    fn mount(&mut self, w: &HelperWork, cred: &Cred) -> Result<&Mount, Errno> {
        let key = (w.fs, w.source.clone());
        if !self.mounts.contains_key(&key) {
            let o = self.options(w.fs)?;
            let m = nfs::client::connect(&w.source, &o, MountCcsid::default(), cred, &self.env)
                .map_err(RemoteError::errno)?;
            self.mounts.insert(key.clone(), m);
        }
        self.mounts.get(&key).ok_or(Errno::EIO)
    }

    /// Carry out one block.
    pub fn run(&mut self, w: &HelperWork) -> HelperResult {
        let cred = Cred {
            uid: w.uid,
            gid: w.gid,
            gids: w.gids.clone(),
        };
        let h = RHandle(w.handle.clone());
        let r = self.mount(w, &cred).and_then(|m| match w.op {
            HelperOp::Read => m
                .read(&cred, &h, w.off, w.len)
                .map(|(data, eof)| HelperResult {
                    data,
                    eof,
                    ..HelperResult::default()
                })
                .map_err(RemoteError::errno),
            HelperOp::Write => {
                let how = if w.sync {
                    Stable::FileSync
                } else {
                    Stable::Unstable
                };
                m.write(&cred, &h, w.off, &w.data, how)
                    .map(|r| HelperResult {
                        count: r.count,
                        stable: !matches!(r.committed, Stable::Unstable),
                        verifier: r.verifier,
                        ..HelperResult::default()
                    })
                    .map_err(RemoteError::errno)
            }
        });
        r.unwrap_or_else(|e| {
            // A connection that failed is made again for the next block.
            self.mounts.remove(&(w.fs, w.source.clone()));
            HelperResult {
                err: Some(e),
                ..HelperResult::default()
            }
        })
    }
}

/// The block I/O of a helper job: the NFS client over `fs` and `host`'s
/// names.
#[must_use]
pub fn block_io<H: Host>(host: &H) -> Option<Io> {
    let fs = host.fs().ok()?;
    let env = ClientEnv {
        resolver: Arc::new(Names(host.names())),
        host_name: host.host_name(),
        local: None,
        connections: 2,
        ..ClientEnv::default()
    };
    let options = move |id: u32| {
        fs.mounts()?
            .into_iter()
            .find(|m| m.id == id)
            .map(|m| m.options)
            .ok_or(Errno::ESTALE)
    };
    let mut h = Helper::new(Box::new(options), env);
    Some(Box::new(move |w: &HelperWork| h.run(w)))
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
        helper_loop(&fs, block_io(&*h), stop);
        Ok(())
    });
    match r {
        Ok(r) => Serve::Ready(r),
        Err(f) => Serve::Unavailable(f),
    }
}
