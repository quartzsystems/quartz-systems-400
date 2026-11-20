// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The listener: each connection admitted within the [`Gate`]'s limits,
//! its TLS handshake done within [`HANDSHAKE`], then its session on a
//! thread of its own, reading with a short timeout so the session's
//! clock, its events and its streams move while the console says
//! nothing. A pin removed ends its session within a second.

use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use qsi::Hello;
use qsisess::Limits;
use qsitrust::{Identity, Pins};
use rustls::{ServerConfig, ServerConnection, StreamOwned};

use crate::conn::{Config, Conn, SharedLog};
use crate::gate::{Gate, HANDSHAKE, Turned};
use crate::host::{Host, Opener};
use crate::trust::{Keeper, SharedPins};

/// How long a session's loop waits for bytes.
const TURN: Duration = Duration::from_millis(20);

/// What makes a session's host and opener, on the session's thread.
pub type MakeHost = dyn Fn() -> Result<(Box<dyn Host>, Box<dyn Opener>), String> + Send + Sync;

/// Everything a connection needs, shared by all of them.
pub struct Server {
    /// The TLS configuration over the identity and the pins.
    pub tls: Arc<ServerConfig>,
    /// The system's identity.
    pub identity: Identity,
    /// Who may connect.
    pub pins: SharedPins,
    /// The system's hello.
    pub hello: Hello,
    /// The session's limits.
    pub limits: Limits,
    /// The counts connections are admitted by.
    pub gate: Mutex<Gate>,
    /// The event log.
    pub log: SharedLog,
    /// The password and the pins.
    pub keeper: Arc<dyn Keeper>,
    /// The server is ending: sessions say goodbye.
    pub stopping: AtomicBool,
    /// Lines for the job log.
    lines: Mutex<Vec<String>>,
    make: Box<MakeHost>,
}

impl std::fmt::Debug for Server {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Server").finish_non_exhaustive()
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn quiet(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
    )
}

fn now_ms(t0: Instant) -> u64 {
    u64::try_from(t0.elapsed().as_millis()).unwrap_or(u64::MAX)
}

impl Server {
    /// A server over `identity`, answering with `hello`; `make` gives
    /// each session its host.
    ///
    /// # Errors
    /// The TLS configuration could not be made.
    pub fn new(
        identity: Identity,
        pins: SharedPins,
        hello: Hello,
        log: SharedLog,
        keeper: Arc<dyn Keeper>,
        make: Box<MakeHost>,
    ) -> Result<Self, String> {
        let tls = qsitrust::tls::server_config(&identity, Arc::new(pins.clone()))
            .map_err(|e| e.to_string())?;
        Ok(Self {
            tls,
            identity,
            pins,
            hello,
            limits: Limits::default(),
            gate: Mutex::new(Gate::default()),
            log,
            keeper,
            stopping: AtomicBool::new(false),
            lines: Mutex::new(Vec::new()),
            make,
        })
    }

    /// A line for the job log.
    pub fn say(&self, line: &str) {
        lock(&self.lines).push(line.to_string());
    }

    /// The lines said since the last call.
    pub fn said(&self) -> Vec<String> {
        std::mem::take(&mut *lock(&self.lines))
    }

    /// Sessions open now.
    pub fn sessions(&self) -> usize {
        lock(&self.gate).sessions()
    }
}

/// Finish the handshake within [`HANDSHAKE`].
fn handshake(tls: &mut StreamOwned<ServerConnection, TcpStream>) -> Result<(), String> {
    let until = Instant::now() + HANDSHAKE;
    while tls.conn.is_handshaking() {
        match tls.conn.complete_io(&mut tls.sock) {
            Ok(_) => {}
            Err(e) if quiet(&e) && Instant::now() < until => {}
            Err(e) if quiet(&e) => return Err("it took too long".into()),
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

/// One connection, from its handshake to its end.
pub fn connection(server: &Arc<Server>, stream: TcpStream, from: SocketAddr) {
    let ip = from.ip();
    let fail = |why: &str| {
        lock(&server.gate).failed(ip, Instant::now());
        server.say(&format!(
            "A connection from {from} failed its handshake: {why}."
        ));
    };
    if stream.set_read_timeout(Some(TURN)).is_err() || stream.set_nodelay(true).is_err() {
        fail("the socket");
        return;
    }
    let Ok(conn) = ServerConnection::new(server.tls.clone()) else {
        fail("no TLS");
        return;
    };
    let mut tls = StreamOwned::new(conn, stream);
    if let Err(e) = handshake(&mut tls) {
        fail(&e);
        return;
    }
    let Some(peer) = qsitrust::tls::peer(&tls.conn, &server.pins) else {
        fail("no certificate");
        return;
    };
    lock(&server.gate).opened();
    let binding = qsitrust::tls::binding(&tls.conn, &server.identity).ok();
    let config = Config {
        hello: server.hello.clone(),
        address: from.to_string(),
        peer: peer.clone(),
        binding,
        limits: server.limits,
    };
    match (server.make)() {
        Ok((host, opener)) => {
            let t0 = Instant::now();
            match Conn::new(
                config,
                host,
                opener,
                server.log.clone(),
                server.keeper.clone(),
                0,
            ) {
                Ok(mut conn) => serve(server, &mut conn, &mut tls, t0, &peer, ip),
                Err(e) => server.say(&format!("A session from {from} did not start: {e}.")),
            }
        }
        Err(e) => server.say(&format!(
            "A session from {from} has no way to the system: {e}."
        )),
    }
    lock(&server.gate).closed();
}

/// The session's loop: bytes in, a turn, bytes out.
fn serve(
    server: &Arc<Server>,
    conn: &mut Conn,
    tls: &mut StreamOwned<ServerConnection, TcpStream>,
    t0: Instant,
    peer: &qsitrust::Peer,
    ip: IpAddr,
) {
    let mut buf = vec![0u8; 64 * 1024];
    let mut checked = Instant::now();
    let mut gone = false;
    loop {
        match tls.read(&mut buf) {
            Ok(0) => gone = true,
            Ok(n) => conn.feed(&buf[..n], now_ms(t0)),
            Err(e) if quiet(&e) => {}
            Err(_) => gone = true,
        }
        if gone {
            conn.gone();
        } else {
            conn.step(now_ms(t0));
        }
        if server.stopping.load(Ordering::Relaxed) && !conn.is_closed() {
            conn.close("the QSI server is ending");
        }
        if peer.pinned && checked.elapsed() >= Duration::from_secs(1) {
            checked = Instant::now();
            if !server.pins.pinned(&peer.fingerprint) {
                conn.close("the console was removed at the system");
            }
        }
        if let Some(fp) = conn.paired() {
            server.pins.add(fp);
        }
        for line in conn.said() {
            server.say(&line);
        }
        for _ in 0..conn.failed_pairings() {
            lock(&server.gate).failed_pairing(ip, Instant::now());
        }
        if gone {
            break;
        }
        let out = conn.take_output();
        if !out.is_empty() && (tls.write_all(&out).is_err() || tls.flush().is_err()) {
            conn.gone();
            break;
        }
        if conn.is_closed() {
            break;
        }
    }
    tls.conn.send_close_notify();
    let _ = tls.flush();
}

/// The accept loop on a thread: [`Running::stop`] ends it.
#[derive(Debug)]
pub struct Running {
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Running {
    /// Stop accepting; sessions open now are asked to end.
    pub fn stop(mut self, server: &Server) {
        server.stopping.store(true, Ordering::Relaxed);
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Accept on `listener` until stopped, each connection admitted by the
/// gate on a thread of its own.
///
/// # Errors
/// The listener could not be made to poll.
pub fn listen(listener: TcpListener, server: Arc<Server>) -> std::io::Result<Running> {
    listener.set_nonblocking(true)?;
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let thread = thread::Builder::new()
        .name("qsi-accept".into())
        .spawn(move || accept(&listener, &server, &flag))?;
    Ok(Running {
        stop,
        thread: Some(thread),
    })
}

fn accept(listener: &TcpListener, server: &Arc<Server>, stop: &AtomicBool) {
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, from)) => {
                let admitted = lock(&server.gate).admit(from.ip(), Instant::now());
                match admitted {
                    Ok(()) => {
                        let _ = stream.set_nonblocking(false);
                        let s = server.clone();
                        let spawned = thread::Builder::new()
                            .name("qsi-session".into())
                            .spawn(move || connection(&s, stream, from));
                        if spawned.is_err() {
                            lock(&server.gate).failed(from.ip(), Instant::now());
                        }
                    }
                    Err(why) => {
                        let first =
                            why != Turned::Failing || lock(&server.gate).say_failing(from.ip());
                        if first {
                            server.say(&format!(
                                "A connection from {from} was turned away: {}.",
                                why.text()
                            ));
                        }
                    }
                }
            }
            Err(_) => thread::sleep(Duration::from_millis(50)),
        }
    }
}
