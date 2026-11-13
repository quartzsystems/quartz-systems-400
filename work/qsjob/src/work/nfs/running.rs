// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A daemon serving: its sockets bound by the job, its programs
//! registered with the binder as the oracle's `/etc/RPCBTAB` shows them
//! (the wildcard address `::.p1.p2`, owner `4294967295`), its loop on a
//! thread of its own until the job asks it to stop; then the loop is
//! given a bound to finish the calls in flight and the registrations
//! go.

use std::io;
use std::net::{SocketAddr, TcpListener, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use nfs::rpc::{ClientConfig, Proto};
use nfs::rpcb::BinderClient;
use nfs::rpcb::binder::UNKNOWN_OWNER;
use nfs::server::run::Listeners;
use nfs::types::rpcb::rpcb;
use stg::nfs::NfsPort;

use super::DaemonEnv;
use super::host::Net;

/// How long the loop has to finish the calls in flight at the end.
pub const END_BOUND: Duration = Duration::from_secs(10);
/// How long a loop has to say it serves.
pub const START_BOUND: Duration = Duration::from_secs(30);

/// Why a daemon could not serve, or stopped: an error number (Linux's)
/// and what failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fail {
    /// The error number, 0 when none.
    pub errno: i32,
    /// What failed.
    pub what: String,
}

impl Fail {
    /// From an I/O error.
    #[must_use]
    pub fn io(what: &str, e: &io::Error) -> Self {
        Self {
            errno: e.raw_os_error().unwrap_or(0),
            what: format!("{what}: {e}"),
        }
    }

    /// Without an error number.
    #[must_use]
    pub fn new(what: impl Into<String>) -> Self {
        Self {
            errno: 0,
            what: what.into(),
        }
    }
}

/// A program and versions to register on the listeners.
#[derive(Debug, Clone, Copy)]
pub struct Prog {
    /// The program number.
    pub number: u32,
    /// Its versions.
    pub versions: &'static [u32],
}

/// Bind TCP and UDP on `port` (0: any) of the job's address.
///
/// # Errors
///
/// The socket's.
pub fn bind(net: Net, port: u16) -> Result<Listeners, Fail> {
    let at = SocketAddr::new(net.bind, port);
    let tcp = TcpListener::bind(at).map_err(|e| Fail::io(&format!("TCP port {port}"), &e))?;
    // UDP on the port TCP got, when it was chosen freely and is free.
    let uport = if port == 0 {
        tcp.local_addr().map_or(0, |a| a.port())
    } else {
        port
    };
    let udp = UdpSocket::bind(SocketAddr::new(net.bind, uport))
        .or_else(|e| {
            if port == 0 {
                UdpSocket::bind(SocketAddr::new(net.bind, 0))
            } else {
                Err(e)
            }
        })
        .map_err(|e| Fail::io(&format!("UDP port {uport}"), &e))?;
    Ok(Listeners {
        tcp: Some(tcp),
        udp: Some(udp),
    })
}

/// The registrations of `p` on `l`, as the oracle writes them.
#[must_use]
pub fn entries(l: &Listeners, p: Prog) -> Vec<rpcb> {
    let mut socks: Vec<(u16, &[u8])> = Vec::new();
    if let Some(Ok(a)) = l.tcp.as_ref().map(TcpListener::local_addr) {
        socks.push((a.port(), b"tcp"));
    }
    if let Some(Ok(a)) = l.udp.as_ref().map(UdpSocket::local_addr) {
        socks.push((a.port(), b"udp"));
    }
    let mut out = Vec::new();
    for v in p.versions {
        for (port, netid) in &socks {
            out.push(rpcb {
                r_prog: p.number,
                r_vers: *v,
                r_netid: netid.to_vec(),
                r_addr: format!("::.{}.{}", port >> 8, port & 0xff).into_bytes(),
                r_owner: UNKNOWN_OWNER.to_vec(),
            });
        }
    }
    out
}

/// What the registry shows of `list`.
#[must_use]
pub fn ports(list: &[rpcb]) -> Vec<NfsPort> {
    list.iter()
        .map(|r| NfsPort {
            program: r.r_prog,
            version: r.r_vers,
            proto: String::from_utf8_lossy(&r.r_netid).into_owned(),
            port: nfs::rpcb::uaddr::port_of(&String::from_utf8_lossy(&r.r_addr)).unwrap_or(0),
        })
        .collect()
}

fn binder(net: Net) -> BinderClient {
    BinderClient::at(
        net.binder,
        Proto::Udp,
        ClientConfig {
            timeout: Duration::from_secs(2),
            retransmits: 2,
            reserved_port: false,
            ..ClientConfig::default()
        },
    )
}

/// Register `list` with the binder, each after unsetting what an
/// earlier start left.
///
/// # Errors
///
/// A registration refused or not answered.
pub fn register(net: Net, list: &[rpcb]) -> Result<(), Fail> {
    let mut b = binder(net);
    for r in list {
        let _ = b.unset(r);
        match b.set(r) {
            Ok(true) => {}
            Ok(false) => {
                return Err(Fail::new(format!(
                    "the binder refused program {} version {}",
                    r.r_prog, r.r_vers
                )));
            }
            Err(e) => return Err(Fail::new(format!("the binder did not answer: {e}"))),
        }
    }
    Ok(())
}

/// Take `list` back from the binder.
pub fn unregister(net: Net, list: &[rpcb]) {
    let mut b = binder(net);
    for r in list {
        let _ = b.unset(r);
    }
}

type Tick = Box<dyn FnMut(&mut dyn DaemonEnv)>;

/// A daemon serving.
pub struct Running {
    /// What it registered.
    pub ports: Vec<NfsPort>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<(), Fail>>>,
    tick: Option<Tick>,
    finish: Vec<Box<dyn FnOnce()>>,
}

impl std::fmt::Debug for Running {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Running")
            .field("ports", &self.ports)
            .finish_non_exhaustive()
    }
}

impl Running {
    /// Run `body` on a thread of its own until `stop`; `body` calls its
    /// second argument once it serves. Waits until it does (or fails, or
    /// [`START_BOUND`] passes).
    ///
    /// # Errors
    ///
    /// The loop's, before it served.
    pub fn spawn<F>(name: &str, body: F) -> Result<Self, Fail>
    where
        F: FnOnce(&AtomicBool, &(dyn Fn() + Sync)) -> Result<(), Fail> + Send + 'static,
    {
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::sync_channel::<()>(1);
        let s = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name(name.to_string())
            .spawn(move || {
                let tx = std::sync::Mutex::new(tx);
                let ready = move || {
                    let _ = tx
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .try_send(());
                };
                body(&s, &ready)
            })
            .map_err(|e| Fail::io("a thread", &e))?;
        let started = Instant::now();
        loop {
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(()) => break,
                Err(mpsc::RecvTimeoutError::Disconnected | mpsc::RecvTimeoutError::Timeout)
                    if thread.is_finished() =>
                {
                    return Err(match thread.join() {
                        Ok(Err(f)) => f,
                        Ok(Ok(())) => Fail::new("the daemon's loop ended before it served"),
                        Err(_) => Fail::new("the daemon's loop failed"),
                    });
                }
                Err(_) if started.elapsed() > START_BOUND => {
                    stop.store(true, Ordering::Relaxed);
                    return Err(Fail::new("the daemon's loop did not start in time"));
                }
                Err(_) => {}
            }
        }
        Ok(Self {
            ports: Vec::new(),
            stop,
            thread: Some(thread),
            tick: None,
            finish: Vec::new(),
        })
    }

    /// With what it registered.
    #[must_use]
    pub fn with_ports(mut self, ports: Vec<NfsPort>) -> Self {
        self.ports = ports;
        self
    }

    /// With `f` called at each of the job's pauses.
    #[must_use]
    pub fn with_tick(mut self, f: impl FnMut(&mut dyn DaemonEnv) + 'static) -> Self {
        self.tick = Some(Box::new(f));
        self
    }

    /// With `f` run once the loop stopped (the registrations taken back).
    #[must_use]
    pub fn on_end(mut self, f: impl FnOnce() + 'static) -> Self {
        self.finish.push(Box::new(f));
        self
    }

    /// A job's pause: the tick.
    pub fn tick(&mut self, env: &mut dyn DaemonEnv) {
        if let Some(t) = self.tick.as_mut() {
            t(env);
        }
    }

    /// Why the loop ended when nobody asked it to; `None` while it runs.
    pub fn failure(&mut self) -> Option<Fail> {
        if self.stop.load(Ordering::Relaxed) || !self.thread.as_ref()?.is_finished() {
            return None;
        }
        let t = self.thread.take()?;
        Some(match t.join() {
            Ok(Err(f)) => f,
            Ok(Ok(())) => Fail::new("the daemon's loop ended"),
            Err(_) => Fail::new("the daemon's loop failed"),
        })
    }

    /// Stop: the loop has [`END_BOUND`] to end, then the end actions run.
    /// Whether it ended in time.
    pub fn stop(mut self) -> bool {
        self.stop.store(true, Ordering::Relaxed);
        let end = Instant::now() + END_BOUND;
        let mut ended = true;
        if let Some(t) = self.thread.take() {
            while !t.is_finished() && Instant::now() < end {
                std::thread::sleep(Duration::from_millis(20));
            }
            if t.is_finished() {
                let _ = t.join();
            } else {
                ended = false;
            }
        }
        for f in std::mem::take(&mut self.finish) {
            f();
        }
        ended
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registrations_are_the_oracles_form() {
        let net = Net {
            bind: "127.0.0.1".parse().unwrap(),
            binder: "127.0.0.1:111".parse().unwrap(),
            rpc_port: 0,
            nfs_port: 0,
        };
        let l = bind(net, 0).unwrap();
        let port = l.tcp.as_ref().unwrap().local_addr().unwrap().port();
        let e = entries(
            &l,
            Prog {
                number: 100_005,
                versions: &[1, 3],
            },
        );
        assert_eq!(e.len(), 4);
        assert_eq!(
            e[0].r_addr,
            format!("::.{}.{}", port >> 8, port & 0xff).into_bytes()
        );
        assert_eq!(e[0].r_owner, b"4294967295");
        let tab = nfs::rpcb::rpcbtab::encode(&e).unwrap();
        assert_eq!(tab.len(), 4 * nfs::rpcb::rpcbtab::RECORD);
        assert_eq!(ports(&e)[1].proto, "udp");
    }

    #[test]
    fn a_loop_that_fails_before_serving_says_why() {
        let r = Running::spawn("t", |_, _| Err(Fail::new("no")));
        assert_eq!(r.unwrap_err().what, "no");
        let mut r = Running::spawn("t", |stop, ready| {
            ready();
            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(())
        })
        .unwrap();
        assert!(r.failure().is_none());
        assert!(r.stop());
    }
}
