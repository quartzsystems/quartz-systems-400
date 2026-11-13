// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a daemon's threads need of the job, any thread asking: a file
//! system client of the thread's own, the service's NFS requests, the
//! network's addresses, the hand-over of connections between
//! `QNFSNFSD` jobs, and host names. [`JobHost`] is the job's; the tests
//! have one over an IFS in memory.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::os::fd::OwnedFd;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::thread;

use ifs::client::IfsClient;
use ipc::chan::Chan;
use nfs::exports::Resolver;
use nfs::server::locate::{BinderLocator, Locator};
use sockets::relay;
use stg::nfs::{NfsDaemonJob, NfsOp, NfsReply};
use stg::remote::RemoteIfs;
use stg::{Client, Link, Shared};

use super::names::HostNames;

/// Where the daemons listen and find the binder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Net {
    /// The address listeners bind (every interface in the job).
    pub bind: IpAddr,
    /// The binder (`127.0.0.1:111` in the job).
    pub binder: SocketAddr,
    /// The binder's own port (111).
    pub rpc_port: u16,
    /// The server's port (2049).
    pub nfs_port: u16,
}

impl Net {
    /// The system's: every interface, the binder on 111, NFS on 2049.
    #[must_use]
    pub fn system() -> Self {
        Self {
            bind: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            binder: SocketAddr::from((Ipv4Addr::LOCALHOST, 111)),
            rpc_port: 111,
            nfs_port: 2049,
        }
    }
}

/// What the daemons' threads ask of the job.
pub trait Host: Send + Sync + 'static {
    /// A thread's file system client.
    type Fs: IfsClient + 'static;
    /// A client for the calling thread (a link of its own in the job).
    ///
    /// # Errors
    ///
    /// The link could not be made.
    fn fs(&self) -> Result<Self::Fs, String>;
    /// One NFS request to the service, from any thread.
    fn op(&self, op: NfsOp) -> NfsReply;
    /// The network.
    fn net(&self) -> Net;
    /// Hand connection `s` to server job `to`; the stream back when it
    /// could not be.
    ///
    /// # Errors
    ///
    /// The connection, not handed over.
    fn give(&self, to: &NfsDaemonJob, s: TcpStream) -> Result<(), TcpStream>;
    /// A connection handed to this job, waiting up to `ms` (at least 1).
    fn take(&self, ms: u32) -> Option<TcpStream>;
    /// Host names to addresses and back.
    fn names(&self) -> Box<dyn Resolver + Send + Sync>;
    /// Where other hosts' programs (and the local monitor) listen.
    fn locator(&self) -> Arc<dyn Locator>;
    /// This host's name, as the status monitor tells others.
    fn host_name(&self) -> String;
    /// The system name (`QSYSNAME`): the `NFSv4` server owner and scope.
    fn system_name(&self) -> String;
}

/// A daemon thread's file system: its own client under `IfsVfs`. A
/// thread that cannot have a link to the service ends (the daemon's
/// loop then reports it).
///
/// # Panics
///
/// When the thread gets no client.
pub fn thread_vfs<H: Host>(
    host: &H,
    instance: &std::sync::Arc<nfs::server::Instance>,
) -> nfs::server::IfsVfs<H::Fs> {
    let fs = host
        .fs()
        .unwrap_or_else(|e| panic!("an NFS daemon thread has no link to the storage service: {e}"));
    nfs::server::IfsVfs::new(
        fs,
        std::sync::Arc::clone(instance),
        nfs::server::ifsvfs::Config::default(),
    )
}

type Call = Box<dyn FnOnce(&Shared) + Send>;

/// The job's [`Host`]: a thread with a link of its own answers every
/// thread's NFS requests, and each daemon thread makes its own link.
pub struct JobHost {
    number: u32,
    pid: u32,
    me: sockets::JobId,
    tx: Mutex<mpsc::Sender<Call>>,
}

/// A thread's link of job `number` in process `pid` (`hello_thread`:
/// the same job, so the same IFS owner, as every link of the job).
///
/// # Errors
///
/// The service's.
pub fn thread_link(number: u32, pid: u32) -> Result<Shared, String> {
    let chan = Chan::connect(Path::new(stg::SOCKET)).map_err(|e| e.to_string())?;
    let mut link = Link::over(chan);
    link.with(|l| l.hello_thread(number, pid))?;
    Ok(std::rc::Rc::new(std::cell::RefCell::new(link)))
}

/// The job's identifier on the descriptor relay (as the host servers').
#[must_use]
pub fn relay_id(number: u32, name: &str) -> sockets::JobId {
    let start = wm::ctl::Start {
        number,
        name: name.to_string(),
        ..wm::ctl::Start::default()
    };
    crate::work::hostsvr::daemon::internal_id(&start)
}

impl JobHost {
    /// The host of job `number` named `name`.
    ///
    /// # Errors
    ///
    /// The link thread could not be started.
    pub fn start(number: u32, name: &str) -> Result<Self, String> {
        let pid = std::process::id();
        let (tx, rx) = mpsc::channel::<Call>();
        let (ok_tx, ok_rx) = mpsc::channel();
        thread::Builder::new()
            .name("nfs-svc".into())
            .spawn(move || {
                let link = match thread_link(number, pid) {
                    Ok(l) => {
                        let _ = ok_tx.send(Ok(()));
                        l
                    }
                    Err(e) => {
                        let _ = ok_tx.send(Err(e));
                        return;
                    }
                };
                while let Ok(call) = rx.recv() {
                    call(&link);
                }
            })
            .map_err(|e| e.to_string())?;
        ok_rx.recv().map_err(|e| e.to_string())??;
        Ok(Self {
            number,
            pid,
            me: relay_id(number, name),
            tx: Mutex::new(tx),
        })
    }

    /// Run `f` on the link thread and wait for it.
    fn call<T: Send + 'static>(&self, f: impl FnOnce(&Shared) -> T + Send + 'static) -> Option<T> {
        let (tx, rx) = mpsc::channel();
        let call: Call = Box::new(move |l| {
            let _ = tx.send(f(l));
        });
        self.tx
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .send(call)
            .ok()?;
        rx.recv().ok()
    }
}

/// The job number in a qualified job name (`000123/USER/NAME`).
fn number_of(job: &str) -> Option<u32> {
    job.split('/').next()?.parse().ok()
}

impl Host for JobHost {
    type Fs = RemoteIfs;

    fn fs(&self) -> Result<RemoteIfs, String> {
        thread_link(self.number, self.pid).map(RemoteIfs::new)
    }

    fn op(&self, op: NfsOp) -> NfsReply {
        self.call(move |l| l.borrow_mut().with(|c| c.nfs(op)))
            .unwrap_or(NfsReply::Refused {
                msgid: "CPFA1B4".into(),
                data: Vec::new(),
                text: "the job's link to the storage service is gone".into(),
            })
    }

    fn net(&self) -> Net {
        Net::system()
    }

    fn give(&self, to: &NfsDaemonJob, s: TcpStream) -> Result<(), TcpStream> {
        let Some(n) = number_of(&to.job) else {
            return Err(s);
        };
        let target = relay_id(n, to.daemon.job_name());
        let Ok(dup) = s.try_clone() else {
            return Err(s);
        };
        let fd = sockets::Fd::from(OwnedFd::from(dup));
        match relay::give(relay::SOCKET, &fd, &target, &self.me, b"") {
            Ok(()) => Ok(()),
            Err(e) => {
                eprintln!("qsjob: QNFSNFSD: handing a connection to {}: {e:?}", to.job);
                Err(s)
            }
        }
    }

    fn take(&self, ms: u32) -> Option<TcpStream> {
        match relay::take(relay::SOCKET, &self.me, None, ms.max(1)) {
            Ok(Some(t)) => Some(TcpStream::from(t.fd.into_owned())),
            _ => None,
        }
    }

    fn names(&self) -> Box<dyn Resolver + Send + Sync> {
        Box::new(HostNames::new(self.host_config()))
    }

    fn locator(&self) -> Arc<dyn Locator> {
        Arc::new(BinderLocator::default())
    }

    fn host_name(&self) -> String {
        let c = self.host_config();
        let name = match (c.host_name.trim(), c.dmn_name.trim()) {
            ("", _) => "localhost".to_string(),
            (h, "") => h.to_string(),
            (h, d) => format!("{h}.{d}"),
        };
        name.to_ascii_lowercase()
    }

    fn system_name(&self) -> String {
        self.sysname()
    }
}

impl JobHost {
    /// `QSYSNAME`.
    fn sysname(&self) -> String {
        self.call(crate::work::ddm::system_name)
            .unwrap_or_else(|| "QS400".into())
    }

    /// The resolver's configuration (`CFGTCP`).
    fn host_config(&self) -> tcpcfg::resolver::HostConfig {
        self.call(
            |l| match l.borrow_mut().with(|c| c.tcp(stg::tcp::TcpOp::HostConfig)) {
                stg::tcp::TcpReply::HostConfig { cfg } => cfg,
                _ => tcpcfg::resolver::HostConfig::default(),
            },
        )
        .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn job_numbers_from_qualified_names() {
        assert_eq!(super::number_of("000123/QSECOFR/QNFSNFSD"), Some(123));
        assert_eq!(super::number_of("x"), None);
    }
}
