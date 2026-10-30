// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The DDM recording proxy: listens on 127.0.0.1 at the DDM port,
//! forwards each connection to the oracle unchanged and records both
//! directions cut into DSSes and masked, as `ddm` traces. Other ports
//! (the sign-on server a `JTOpen` client needs first) are forwarded
//! without recording.

use std::io::{Read as _, Write as _};
use std::net::{Shutdown, TcpListener, TcpStream, ToSocketAddrs as _};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, bail};
use ddm::Dir;
use ddm::trace::{self, Chunk, Connection, Event, Trace, mask};

/// What the proxy needs.
#[derive(Clone, Debug)]
pub struct ProxyConfig {
    /// The oracle's host.
    pub upstream: String,
    /// The recorded port (same number upstream).
    pub port: u16,
    /// Ports forwarded without recording.
    pub forward: Vec<u16>,
    /// Plain-text forms of the password that must never be recorded.
    pub forbidden: Vec<Vec<u8>>,
    /// The user (masked wherever it appears).
    pub user: String,
    /// The password (in memory only, to check substitutes).
    pub password: String,
}

struct Live {
    conn: Connection,
    start: Instant,
    next_seq: u32,
    accsec: Option<(u16, Vec<u8>)>,
    accsecrd: Option<Vec<u8>>,
}

impl Live {
    fn push(&mut self, dir: Dir, framed: bool, bytes: Vec<u8>) {
        self.next_seq += 1;
        let at_ms = ms(self.start);
        self.conn.events.push(Event::Chunk(Chunk {
            seq: self.next_seq,
            dir,
            at_ms,
            framed,
            bytes,
        }));
    }

    fn note(&mut self, text: String) {
        let at_ms = ms(self.start);
        self.conn.events.push(Event::Note { at_ms, text });
    }
}

fn ms(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// A running proxy.
pub struct Proxy {
    conns: Arc<Mutex<Vec<Arc<Mutex<Live>>>>>,
    active: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    leak: Arc<AtomicBool>,
    sockets: Arc<Mutex<Vec<TcpStream>>>,
    port: u16,
}

impl Proxy {
    /// Bind the ports and start accepting.
    pub fn start(cfg: &ProxyConfig) -> Result<Self> {
        let p = Self {
            conns: Arc::default(),
            active: Arc::default(),
            stop: Arc::default(),
            leak: Arc::default(),
            sockets: Arc::default(),
            port: cfg.port,
        };
        let cfg = Arc::new(cfg.clone());
        let mut ports = vec![cfg.port];
        ports.extend(&cfg.forward);
        for port in ports {
            let l = TcpListener::bind(("127.0.0.1", port))
                .with_context(|| format!("bind 127.0.0.1:{port}"))?;
            l.set_nonblocking(true)?;
            let recorded = port == cfg.port;
            let (conns, active, stop, leak, sockets, cfg) = (
                p.conns.clone(),
                p.active.clone(),
                p.stop.clone(),
                p.leak.clone(),
                p.sockets.clone(),
                cfg.clone(),
            );
            thread::spawn(move || {
                while !stop.load(Ordering::SeqCst) {
                    match l.accept() {
                        Ok((client, _)) => {
                            let _ = client.set_nonblocking(false);
                            let live = Arc::new(Mutex::new(Live {
                                conn: Connection::default(),
                                start: Instant::now(),
                                next_seq: 0,
                                accsec: None,
                                accsecrd: None,
                            }));
                            if recorded {
                                let mut c = conns.lock().unwrap();
                                c.push(live.clone());
                                live.lock().unwrap().conn.id =
                                    u32::try_from(c.len()).unwrap_or(u32::MAX);
                            }
                            active.fetch_add(1, Ordering::SeqCst);
                            let (active, leak, sockets, cfg) =
                                (active.clone(), leak.clone(), sockets.clone(), cfg.clone());
                            thread::spawn(move || {
                                serve(&cfg, port, recorded, client, &live, &leak, &sockets);
                                let end = ms(live.lock().unwrap().start);
                                live.lock().unwrap().conn.end_ms = Some(end);
                                active.fetch_sub(1, Ordering::SeqCst);
                            });
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(10));
                        }
                        Err(e) => eprintln!("accept on {port}: {e}"),
                    }
                }
            });
        }
        Ok(p)
    }

    /// Wait up to `settle` for open connections to close, cut the rest,
    /// and give back the trace.
    pub fn finish(self, settle: Duration, scenario: &str) -> Result<Trace> {
        let until = Instant::now() + settle;
        while self.active.load(Ordering::SeqCst) > 0 && Instant::now() < until {
            thread::sleep(Duration::from_millis(20));
        }
        self.stop.store(true, Ordering::SeqCst);
        for s in self.sockets.lock().unwrap().iter() {
            let _ = s.shutdown(Shutdown::Both);
        }
        let until = Instant::now() + Duration::from_secs(2);
        while self.active.load(Ordering::SeqCst) > 0 && Instant::now() < until {
            thread::sleep(Duration::from_millis(20));
        }
        if self.leak.load(Ordering::SeqCst) {
            bail!("a plain-text password was seen on the wire; nothing written");
        }
        let conns = self.conns.lock().unwrap();
        Ok(Trace {
            server: "ddm".into(),
            port: self.port,
            scenario: scenario.to_string(),
            connections: conns.iter().map(|l| l.lock().unwrap().conn.clone()).collect(),
        })
    }
}

fn serve(
    cfg: &ProxyConfig,
    port: u16,
    recorded: bool,
    client: TcpStream,
    live: &Arc<Mutex<Live>>,
    leak: &Arc<AtomicBool>,
    sockets: &Arc<Mutex<Vec<TcpStream>>>,
) {
    let addr = (cfg.upstream.as_str(), port)
        .to_socket_addrs()
        .ok()
        .and_then(|mut a| a.next());
    let server = match addr.map(|a| TcpStream::connect_timeout(&a, Duration::from_secs(5))) {
        Some(Ok(s)) => s,
        Some(Err(e)) => {
            live.lock()
                .unwrap()
                .note(format!("upstream connect to port {port} failed: {}", e.kind()));
            let _ = client.shutdown(Shutdown::Both);
            return;
        }
        None => {
            live.lock().unwrap().note("upstream does not resolve".into());
            return;
        }
    };
    let _ = server.set_nodelay(true);
    let _ = client.set_nodelay(true);
    {
        let mut s = sockets.lock().unwrap();
        if let Ok(c) = client.try_clone() {
            s.push(c);
        }
        if let Ok(c) = server.try_clone() {
            s.push(c);
        }
    }
    let up = {
        let (from, to) = (client.try_clone().expect("clone"), server.try_clone().expect("clone"));
        let (live, leak, cfg) = (live.clone(), leak.clone(), cfg.clone());
        thread::spawn(move || pump(from, to, Dir::Request, recorded, &live, &leak, &cfg))
    };
    pump(server, client, Dir::Reply, recorded, live, leak, cfg);
    let _ = up.join();
}

fn pump(
    mut from: TcpStream,
    mut to: TcpStream,
    dir: Dir,
    recorded: bool,
    live: &Arc<Mutex<Live>>,
    leak: &AtomicBool,
    cfg: &ProxyConfig,
) {
    let forms = mask::user_forms(&cfg.user);
    let mut pending: Vec<u8> = Vec::new();
    let mut buf = vec![0u8; 65536];
    loop {
        let n = match from.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        if to.write_all(&buf[..n]).is_err() {
            break;
        }
        if !recorded {
            continue;
        }
        pending.extend_from_slice(&buf[..n]);
        let (chunks, used) = trace::cut(&pending);
        pending.drain(..used);
        let mut l = live.lock().unwrap();
        for (framed, mut bytes) in chunks {
            if framed {
                if let Some(note) = verify(&mut l, dir, &bytes, cfg) {
                    l.note(note);
                }
                mask::mask_dss(&mut bytes);
            }
            mask::mask_user(&mut bytes, &forms);
            check(&bytes, leak, &cfg.forbidden);
            l.push(dir, framed, bytes);
        }
    }
    if recorded && !pending.is_empty() {
        let mut bytes = std::mem::take(&mut pending);
        mask::mask_user(&mut bytes, &forms);
        check(&bytes, leak, &cfg.forbidden);
        live.lock().unwrap().push(dir, false, bytes);
    }
    let _ = to.shutdown(Shutdown::Write);
}

/// Learn the tokens of `ACCSEC`/`ACCSECRD` and check a `SECCHK`
/// substitute against `ddm::sec`; the trace keeps the verdict only.
fn verify(l: &mut Live, dir: Dir, b: &[u8], cfg: &ProxyConfig) -> Option<String> {
    use ddm::cp::{cmd, param, rpy};
    let (d, _) = ddm::Dss::decode(b).ok()?;
    let o = d.first()?;
    let tok = || o.get(param::SECTKN).map(|t| t.bytes().to_vec());
    match (dir, o.cp) {
        (Dir::Request, cmd::ACCSEC) => {
            l.accsec = Some((o.get_u16(param::SECMEC)?, tok().unwrap_or_default()));
            None
        }
        (Dir::Reply, rpy::ACCSECRD) => {
            l.accsecrd = tok();
            None
        }
        (Dir::Request, cmd::SECCHK) => {
            let mech = ddm::sec::Mechanism::from_code(o.get_u16(param::SECMEC)?)?;
            if !mech.uses_seed() {
                return Some(format!("SECCHK with {mech:?}: secrets masked"));
            }
            let (_, client) = l.accsec.clone()?;
            let server = l.accsecrd.clone()?;
            let sent = o.get(param::PASSWORD)?.bytes().to_vec();
            let seeds = hostsvr::crypt::Seeds {
                client: client.try_into().ok()?,
                server: server.try_into().ok()?,
            };
            let level = match sent.len() {
                8 => hostsvr::crypt::Level::Des,
                20 => hostsvr::crypt::Level::Sha1,
                _ => hostsvr::crypt::Level::Sha512,
            };
            let ours = hostsvr::crypt::substitute(level, &cfg.user, &cfg.password, &seeds).ok()?;
            Some(if ours == sent {
                format!("SECCHK substitute ({mech:?}, {level:?}) matches ddm::sec")
            } else {
                format!("SECCHK substitute ({mech:?}, {level:?}) differs from ddm::sec")
            })
        }
        _ => None,
    }
}

fn check(bytes: &[u8], leak: &AtomicBool, forbidden: &[Vec<u8>]) {
    if forbidden.iter().any(|f| mask::contains(bytes, f)) {
        leak.store(true, Ordering::SeqCst);
    }
}
