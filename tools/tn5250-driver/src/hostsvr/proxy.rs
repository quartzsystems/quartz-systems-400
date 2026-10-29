// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The recording proxy: listens on 127.0.0.1 at the host server ports,
//! forwards every connection to the oracle unchanged, and records the
//! bytes both ways, cut into frames and masked, as `hostsvr` traces.

use std::collections::BTreeMap;
use std::io::{Read as _, Write as _};
use std::net::{Shutdown, TcpListener, TcpStream, ToSocketAddrs as _};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, bail};
use hostsvr::trace::{self, Chunk, Connection, Dir, Event, Trace, mask};

/// What the proxy needs.
#[derive(Clone, Debug)]
pub struct ProxyConfig {
    /// The oracle's host.
    pub upstream: String,
    /// The ports to listen on (same number upstream).
    pub ports: Vec<u16>,
    /// Plain-text forms of the password that must never be recorded.
    pub forbidden: Vec<Vec<u8>>,
    /// The user signing on (to check the client's substitutes).
    pub user: String,
    /// The password (kept in memory only).
    pub password: String,
}

/// One connection being recorded.
struct Live {
    conn: Connection,
    start: Instant,
    next_seq: u32,
    client_seed: Option<[u8; 8]>,
    server_seed: Option<[u8; 8]>,
}

impl Live {
    fn push_chunk(&mut self, dir: Dir, framed: bool, bytes: Vec<u8>) {
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

/// Every port's connections, in accept order.
type Book = Arc<Mutex<BTreeMap<u16, Vec<Arc<Mutex<Live>>>>>>;

/// A running proxy.
pub struct Proxy {
    book: Book,
    active: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    leak: Arc<AtomicBool>,
    sockets: Arc<Mutex<Vec<TcpStream>>>,
}

impl Proxy {
    /// Bind every port and start accepting.
    pub fn start(cfg: &ProxyConfig) -> Result<Self> {
        let p = Self {
            book: Arc::default(),
            active: Arc::default(),
            stop: Arc::default(),
            leak: Arc::default(),
            sockets: Arc::default(),
        };
        let cfg = Arc::new(cfg.clone());
        for &port in &cfg.ports {
            let l = TcpListener::bind(("127.0.0.1", port))
                .with_context(|| format!("bind 127.0.0.1:{port}"))?;
            l.set_nonblocking(true)?;
            let (book, active, stop, leak, sockets, cfg) = (
                p.book.clone(),
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
                                client_seed: None,
                                server_seed: None,
                            }));
                            {
                                let mut b = book.lock().unwrap();
                                let list = b.entry(port).or_default();
                                list.push(live.clone());
                                live.lock().unwrap().conn.id =
                                    u32::try_from(list.len()).unwrap_or(u32::MAX);
                            }
                            active.fetch_add(1, Ordering::SeqCst);
                            let (active, leak, sockets, cfg) =
                                (active.clone(), leak.clone(), sockets.clone(), cfg.clone());
                            thread::spawn(move || {
                                serve(&cfg, port, client, &live, &leak, &sockets);
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

    /// Wait up to `settle` for open connections to close, then cut the
    /// rest, and give back the traces by port.
    pub fn finish(self, settle: Duration, scenario: &str) -> Result<Vec<Trace>> {
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
        let book = self.book.lock().unwrap();
        let mut out = Vec::new();
        for (&port, conns) in book.iter() {
            let server = hostsvr::ServerId::from_port(port).map_or_else(
                || {
                    if port == hostsvr::server::SVRMAP_PORT {
                        "svrmap".to_string()
                    } else {
                        format!("port{port}")
                    }
                },
                |s| s.name().to_string(),
            );
            out.push(Trace {
                server,
                port,
                scenario: scenario.to_string(),
                connections: conns
                    .iter()
                    .map(|l| l.lock().unwrap().conn.clone())
                    .collect(),
            });
        }
        Ok(out)
    }
}

/// Forward one connection both ways while recording it.
fn serve(
    cfg: &ProxyConfig,
    port: u16,
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
            live.lock().unwrap().note(format!(
                "upstream connect to port {port} failed: {}",
                e.kind()
            ));
            let _ = client.shutdown(Shutdown::Both);
            return;
        }
        None => {
            live.lock()
                .unwrap()
                .note(format!("upstream {} does not resolve", cfg.upstream));
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
    let raw = port == hostsvr::server::SVRMAP_PORT;
    let up = {
        let (from, to) = (
            client.try_clone().expect("clone"),
            server.try_clone().expect("clone"),
        );
        let (live, leak, cfg) = (live.clone(), leak.clone(), cfg.clone());
        thread::spawn(move || pump(from, to, Dir::Request, raw, &live, &leak, &cfg))
    };
    pump(server, client, Dir::Reply, raw, live, leak, cfg);
    let _ = up.join();
}

/// Copy one direction, recording frames as they complete.
fn pump(
    mut from: TcpStream,
    mut to: TcpStream,
    dir: Dir,
    raw: bool,
    live: &Arc<Mutex<Live>>,
    leak: &AtomicBool,
    cfg: &ProxyConfig,
) {
    let forbidden = &cfg.forbidden;
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
        pending.extend_from_slice(&buf[..n]);
        record(&mut pending, dir, raw, live, leak, cfg);
    }
    if !pending.is_empty() {
        let bytes = std::mem::take(&mut pending);
        check(&bytes, leak, forbidden);
        live.lock().unwrap().push_chunk(dir, false, bytes);
    }
    let _ = to.shutdown(Shutdown::Write);
}

fn record(
    pending: &mut Vec<u8>,
    dir: Dir,
    raw: bool,
    live: &Arc<Mutex<Live>>,
    leak: &AtomicBool,
    cfg: &ProxyConfig,
) {
    let forbidden = &cfg.forbidden;
    if raw {
        let bytes = std::mem::take(pending);
        check(&bytes, leak, forbidden);
        live.lock().unwrap().push_chunk(dir, false, bytes);
        return;
    }
    let (chunks, used) = trace::cut_frames(pending);
    pending.drain(..used);
    let mut l = live.lock().unwrap();
    for (framed, mut bytes) in chunks {
        check(&bytes, leak, forbidden);
        if framed {
            if let Some(note) = verify(&mut l, dir, &bytes, cfg) {
                l.note(note);
            }
            mask::mask_frame(&mut bytes);
        }
        l.push_chunk(dir, framed, bytes);
    }
}

/// Learn the seeds and check the client's password substitutes against
/// `hostsvr::crypt`: the trace records whether they matched, never the
/// substitute.
fn verify(l: &mut Live, dir: Dir, b: &[u8], cfg: &ProxyConfig) -> Option<String> {
    use hostsvr::cp::signon as cp;
    use hostsvr::{Body, Message, common};
    let m = Message::decode(dir, b).ok()?;
    let seed = |m: &Message| {
        m.param(cp::SEED)
            .and_then(|p| <[u8; 8]>::try_from(p.data.as_slice()).ok())
    };
    match &m.body {
        Body::Common(common::Template::SeedRequest(t)) => l.client_seed = Some(t.client_seed),
        Body::Common(common::Template::SeedReply(t)) => l.server_seed = Some(t.server_seed),
        Body::Signon(_) if dir == Dir::Request && seed(&m).is_some() => l.client_seed = seed(&m),
        Body::Signon(_) if dir == Dir::Reply && seed(&m).is_some() => l.server_seed = seed(&m),
        _ => {}
    }
    if dir != Dir::Request || !matches!(m.header.req_rep_id, 0x7002 | 0x7004 | 0x7007) {
        return None;
    }
    let sent = m.param(cp::PASSWORD)?;
    let seeds = hostsvr::crypt::Seeds {
        client: l.client_seed?,
        server: l.server_seed?,
    };
    let level = match sent.data.len() {
        8 => hostsvr::crypt::Level::Des,
        20 => hostsvr::crypt::Level::Sha1,
        _ => hostsvr::crypt::Level::Sha512,
    };
    let ours = hostsvr::crypt::substitute(level, &cfg.user, &cfg.password, &seeds).ok()?;
    Some(if ours == sent.data {
        format!("password substitute ({level:?}) matches hostsvr::crypt")
    } else {
        format!("password substitute ({level:?}) differs from hostsvr::crypt")
    })
}

fn check(bytes: &[u8], leak: &AtomicBool, forbidden: &[Vec<u8>]) {
    if forbidden.iter().any(|f| mask::contains(bytes, f)) {
        leak.store(true, Ordering::SeqCst);
    }
}

/// The plain-text forms of a password: as typed, upper case, in ASCII,
/// EBCDIC and UTF-16.
pub fn forbidden_forms(password: &str) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for p in [password.to_string(), password.to_ascii_uppercase()] {
        if p.len() < 4 {
            continue;
        }
        out.push(p.as_bytes().to_vec());
        if let Ok(e) = ccsid::to_ebcdic(&p) {
            out.push(e);
        }
        out.push(p.encode_utf16().flat_map(u16::to_be_bytes).collect());
    }
    out
}
