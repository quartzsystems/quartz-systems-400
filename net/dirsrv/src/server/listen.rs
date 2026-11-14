// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The listeners and their accept loop. `ibm-slapdPort` always
//! listens; `ibm-slapdSecurePort` when the instance has a certificate
//! and `ibm-slapdSecurity` is `SSL` or `SSLTLS`; `StartTLS` works on the
//! port in the clear when it has one and the value is `TLS` or `SSLTLS`
//! (otherwise it is refused with protocolError, as IBM's server refuses
//! it without a certificate). Each connection gets a thread and a
//! service link of its own; the counts go to the service when they
//! change.

use std::io;
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use ldap::ResultCode;
use tls::rustls::ServerConfig;

use super::conn::{Connection, Setup};
use super::{Counters, Counts, JobSettings, Links, Service, reply};

/// How often the accept loop looks for connections and the end.
const ACCEPT_TICK: Duration = Duration::from_millis(50);
/// How often changed counts are reported.
const REPORT: Duration = Duration::from_secs(2);

/// The bound listeners.
pub struct Listeners {
    /// `ibm-slapdPort`.
    pub plain: TcpListener,
    /// `ibm-slapdSecurePort`, under TLS.
    pub secure: Option<TcpListener>,
    /// The configuration for the secure port and `StartTLS`.
    pub tls: Option<Arc<ServerConfig>>,
    /// Whether `StartTLS` is taken.
    pub start_tls: bool,
}

/// A port that could not be listened on.
#[derive(Debug)]
pub struct BindError {
    /// The port.
    pub port: u16,
    /// Why.
    pub error: io::Error,
}

impl std::fmt::Display for BindError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "port {}: {}", self.port, self.error)
    }
}

impl Listeners {
    /// Bind the ports `settings` asks for on `addr`, given the
    /// certificate's server configuration when there is one. The
    /// secure port failing to bind is reported and the server goes on
    /// without it.
    ///
    /// # Errors
    /// The port in the clear could not be bound.
    pub fn bind(
        addr: IpAddr,
        settings: &JobSettings,
        tls: Option<Arc<ServerConfig>>,
    ) -> Result<(Self, Option<BindError>), BindError> {
        let listen = |port| {
            TcpListener::bind(SocketAddr::new(addr, port))
                .and_then(|l| l.set_nonblocking(true).map(|()| l))
                .map_err(|error| BindError { port, error })
        };
        let plain = listen(settings.port)?;
        let mut failed = None;
        let secure = match &tls {
            Some(_) if settings.ssl() => match listen(settings.secure_port) {
                Ok(l) => Some(l),
                Err(e) => {
                    failed = Some(e);
                    None
                }
            },
            _ => None,
        };
        let start_tls = tls.is_some() && settings.start_tls();
        Ok((
            Self {
                plain,
                secure,
                tls,
                start_tls,
            },
            failed,
        ))
    }

    /// The ports listened on: the one in the clear, the secure one.
    #[must_use]
    pub fn ports(&self) -> (u16, Option<u16>) {
        let port = |l: &TcpListener| l.local_addr().map_or(0, |a| a.port());
        (port(&self.plain), self.secure.as_ref().map(port))
    }
}

/// A running server: the accept loop and its connections.
pub struct Server {
    stop: Arc<AtomicBool>,
    counters: Arc<Counters>,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    /// Serve `listeners` for `instance`, a link from `links` for each
    /// connection and one for the counts.
    #[must_use]
    pub fn start<L: Links>(links: Arc<L>, instance: &str, listeners: Listeners) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let counters = Arc::new(Counters::default());
        let (s, c, inst) = (
            Arc::clone(&stop),
            Arc::clone(&counters),
            instance.to_string(),
        );
        let thread = std::thread::Builder::new()
            .name("ldap-accept".into())
            .spawn(move || accept_loop(&links, &inst, &listeners, &s, &c))
            .ok();
        Self {
            stop,
            counters,
            thread,
        }
    }

    /// The counts now.
    #[must_use]
    pub fn counts(&self) -> Counts {
        self.counters.get()
    }

    /// Whether the accept loop has ended on its own.
    #[must_use]
    pub fn finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// End: the listeners close, every connection gets the notice of
    /// disconnection and ends; at most `wait` for them.
    pub fn stop(mut self, wait: Duration) {
        self.stop.store(true, Ordering::Relaxed);
        let deadline = Instant::now() + wait;
        if let Some(t) = self.thread.take() {
            while !t.is_finished() && Instant::now() < deadline {
                std::thread::sleep(ACCEPT_TICK);
            }
            if t.is_finished() {
                let _ = t.join();
            }
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn accept_loop<L: Links>(
    links: &Arc<L>,
    instance: &str,
    l: &Listeners,
    stop: &Arc<AtomicBool>,
    counters: &Arc<Counters>,
) {
    let mut reporter = links.link().ok();
    let mut sent = Counts::default();
    let mut last_report = Instant::now();
    let mut threads: Vec<JoinHandle<()>> = Vec::new();
    let start_tls = if l.start_tls { l.tls.clone() } else { None };
    // A blocking accept on each listener, on a thread of its own, so a
    // connection is taken at once (a poll every tick made each wait up
    // to a tick: 1 200 connections a minute at most).
    let (tx, rx) = mpsc::channel::<(TcpStream, Option<Arc<ServerConfig>>)>();
    let mut acceptors = Vec::new();
    for (listener, ssl) in
        std::iter::once((&l.plain, None)).chain(l.secure.iter().map(|s| (s, l.tls.clone())))
    {
        let Ok(listener) = listener.try_clone() else {
            continue;
        };
        let wake = listener.local_addr().ok();
        let (tx, stop) = (tx.clone(), Arc::clone(stop));
        let t = std::thread::Builder::new()
            .name("ldap-accept".into())
            .spawn(move || acceptor(&listener, ssl.as_ref(), &stop, &tx));
        if let Ok(t) = t {
            acceptors.push((t, wake));
        }
    }
    drop(tx);
    while !stop.load(Ordering::Relaxed) {
        match rx.recv_timeout(ACCEPT_TICK) {
            Ok((s, ssl)) => {
                let setup = Setup {
                    instance: instance.to_string(),
                    ssl,
                    start_tls: start_tls.clone(),
                    stop: Arc::clone(stop),
                    counters: Arc::clone(counters),
                };
                let links = Arc::clone(links);
                let t = std::thread::Builder::new()
                    .name("ldap-conn".into())
                    .spawn(move || connection(&*links, s, setup));
                match t {
                    Ok(t) => threads.push(t),
                    Err(e) => tracing::warn!("LDAP: no thread for a connection: {e}"),
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        threads.retain(|t| !t.is_finished());
        if last_report.elapsed() >= REPORT {
            last_report = Instant::now();
            report(&mut reporter, instance, counters, &mut sent);
        }
    }
    // Each acceptor wakes for a connection of our own and sees the flag.
    stop.store(true, Ordering::Relaxed);
    for (t, wake) in acceptors {
        if let Some(mut a) = wake {
            if a.ip().is_unspecified() {
                a.set_ip(if a.is_ipv4() {
                    IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
                } else {
                    IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)
                });
            }
            let _ = TcpStream::connect_timeout(&a, Duration::from_secs(1));
        }
        let _ = t.join();
    }
    // The connections see the flag within a tick.
    for t in threads {
        let _ = t.join();
    }
    report(&mut reporter, instance, counters, &mut sent);
}

/// Blocking accepts on `listener` until `stop`, each connection to `tx`.
fn acceptor(
    listener: &TcpListener,
    ssl: Option<&Arc<ServerConfig>>,
    stop: &AtomicBool,
    tx: &mpsc::Sender<(TcpStream, Option<Arc<ServerConfig>>)>,
) {
    let _ = listener.set_nonblocking(false);
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((s, _)) => {
                if stop.load(Ordering::Relaxed) || tx.send((s, ssl.cloned())).is_err() {
                    break;
                }
            }
            Err(e) => {
                tracing::debug!("LDAP: accept: {e}");
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

fn report<S: Service>(
    reporter: &mut Option<S>,
    instance: &str,
    counters: &Counters,
    sent: &mut Counts,
) {
    let now = counters.get();
    if now == *sent {
        return;
    }
    if let Some(r) = reporter.as_mut() {
        match r.conns(instance, now) {
            Ok(()) => *sent = now,
            Err(e) => tracing::debug!("LDAP: counts: {e}"),
        }
    }
}

fn connection<L: Links>(links: &L, s: TcpStream, setup: Setup) {
    let _ = s.set_nonblocking(false);
    match links.link() {
        Ok(svc) => {
            let end = Connection::serve(svc, s, setup);
            tracing::debug!("LDAP connection ended: {end:?}");
        }
        Err(e) => {
            use std::io::Write as _;
            tracing::warn!("LDAP: no link to the storage service: {e}");
            let mut s = s;
            let _ = s.write_all(&reply::notice(
                ResultCode::Unavailable,
                "The directory server cannot reach its data.",
            ));
        }
    }
}
