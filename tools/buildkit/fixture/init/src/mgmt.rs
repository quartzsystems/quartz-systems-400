// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QSI from outside: a managed side and a console, each with an identity
//! made here, over TLS on a socket pair. The console pairs with the
//! one-time password, connects again pinned both ways, and asks
//! `identify`.

use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use qsi::any::{AnyReply, AnyRequest};
use qsi::{Empty, Hello, PeerKind, base, caps, msgid, pair};
use qsisess::{Incoming, Limits, Reply, Role, Session};
use qsitrust::pair::{Console, Guard, Managed, Paired};
use qsitrust::{Fingerprint, Identity, Pins, tls};
use rustls::{ClientConnection, ServerConnection, StreamOwned};

const DEADLINE: Duration = Duration::from_secs(30);

/// Both ends' pins and the managed side's password.
#[derive(Debug, Default)]
struct Keep {
    pins: Vec<Fingerprint>,
    password: Option<String>,
    pairing: bool,
}

#[derive(Debug, Clone, Default)]
struct Shared(Arc<Mutex<Keep>>);

impl Pins for Shared {
    fn pinned(&self, peer: &Fingerprint) -> bool {
        self.0.lock().is_ok_and(|k| k.pins.contains(peer))
    }
    fn pairing_open(&self) -> bool {
        self.0.lock().is_ok_and(|k| k.pairing)
    }
}

struct G<'a>(&'a mut Keep);

impl Guard for G<'_> {
    fn password(&mut self) -> Option<String> {
        self.0.password.clone()
    }
    fn paired(&mut self, console: &Paired) {
        self.0.pins.push(console.fingerprint);
        self.0.password = None;
        self.0.pairing = false;
    }
}

fn hello(kind: PeerKind, name: &str) -> Hello {
    Hello {
        version: qsi::VERSION,
        caps: caps::MANAGED,
        kind,
        name: name.into(),
        release: "V0R3M0".into(),
        serial: String::new(),
    }
}

fn quiet(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
    )
}

/// A session over a stream.
struct End<S> {
    sess: Session,
    io: S,
    t0: Instant,
    gone: bool,
}

impl<S: Read + Write> End<S> {
    fn step(&mut self) -> Vec<Incoming> {
        let now = u64::try_from(self.t0.elapsed().as_millis()).unwrap_or(u64::MAX);
        let out = self.sess.take_output();
        if !out.is_empty() && (self.io.write_all(&out).is_err() || self.io.flush().is_err()) {
            self.gone = true;
        }
        let mut buf = [0u8; 16 * 1024];
        match self.io.read(&mut buf) {
            Ok(0) => self.gone = true,
            Ok(n) => self.sess.feed(&buf[..n], now),
            Err(e) if quiet(&e) => {}
            Err(_) => self.gone = true,
        }
        self.sess.tick(now);
        std::iter::from_fn(|| self.sess.poll()).collect()
    }

    fn ask(&mut self, req: &AnyRequest) -> Result<Reply, String> {
        let id = self
            .sess
            .request_any("QSDEMO", req)
            .map_err(|e| e.to_string())?;
        let end = Instant::now() + DEADLINE;
        while Instant::now() < end && !self.gone {
            for i in self.step() {
                if let Incoming::Reply(r) = i
                    && r.id == id
                {
                    return Ok(r);
                }
            }
        }
        Err("no reply".into())
    }
}

/// Finish a TLS handshake over a socket with a read timeout.
fn handshake<C, D>(conn: &mut C, sock: &mut UnixStream) -> Result<(), String>
where
    C: std::ops::DerefMut<Target = rustls::ConnectionCommon<D>>,
    D: rustls::SideData,
{
    let t0 = Instant::now();
    while conn.is_handshaking() {
        match conn.complete_io(sock) {
            Ok(_) => {}
            Err(e) if quiet(&e) && t0.elapsed() < DEADLINE => {}
            Err(e) => return Err(format!("TLS: {e}")),
        }
    }
    Ok(())
}

fn pair_socket() -> Result<(UnixStream, UnixStream), String> {
    let (a, b) = UnixStream::pair().map_err(|e| e.to_string())?;
    for s in [&a, &b] {
        s.set_read_timeout(Some(Duration::from_millis(5)))
            .map_err(|e| e.to_string())?;
    }
    Ok((a, b))
}

/// The managed side of one connection: pairing and identify, until the
/// console goes.
fn serve(id: &Identity, keep: &Shared, sock: UnixStream) -> Result<(), String> {
    let config = tls::server_config(id, Arc::new(keep.clone())).map_err(|e| e.to_string())?;
    let mut tls = StreamOwned::new(
        ServerConnection::new(config).map_err(|e| e.to_string())?,
        sock,
    );
    handshake(&mut tls.conn, &mut tls.sock)?;
    let pinned = tls::peer(&tls.conn, keep).is_some_and(|p| p.pinned);
    let sess = Session::new(
        Role::Managed,
        hello(PeerKind::Standalone, "QSDEMO"),
        pinned,
        Limits::default(),
        0,
    )
    .map_err(|e| e.to_string())?;
    let mut end = End {
        sess,
        io: tls,
        t0: Instant::now(),
        gone: false,
    };
    let mut pending: Option<Managed> = None;
    let stop = Instant::now() + DEADLINE;
    while !end.gone && !end.sess.is_closed() && Instant::now() < stop {
        for i in end.step() {
            let Incoming::Request(r) = i else { continue };
            let mut k = keep.0.lock().map_err(|_| "poisoned")?;
            let done = match r.decode() {
                Ok(Some(AnyRequest::Pair(pair::Request::Start(s)))) => {
                    let binding = tls::binding(&end.io.conn, id).map_err(|e| e.to_string())?;
                    let start = qsitrust::pair::Start {
                        spake: s.spake,
                        name: s.name,
                    };
                    match Managed::answer(&mut G(&mut k), binding, &start, "QSDEMO") {
                        Ok((m, a)) => {
                            pending = Some(m);
                            let reply = pair::Reply::Answer(pair::Answer {
                                spake: a.spake,
                                confirm: a.confirm,
                                name: a.name,
                            });
                            end.sess.reply_msg(r.id, &reply)
                        }
                        Err(_) => end.sess.refuse(r.id, msgid::NO_PAIRING, "no pairing"),
                    }
                }
                Ok(Some(AnyRequest::Pair(pair::Request::Confirm(c)))) => {
                    let confirm = qsitrust::pair::Confirm { confirm: c.confirm };
                    match pending.take().map(|m| m.confirm(&mut G(&mut k), &confirm)) {
                        Some(Ok(_)) => end.sess.reply_msg(r.id, &pair::Reply::Done(Empty)),
                        _ => end.sess.refuse(r.id, msgid::PAIRING_FAILED, "pairing failed"),
                    }
                }
                Ok(Some(AnyRequest::Base(base::Request::Identify(_)))) => {
                    let me = base::Identity {
                        kind: PeerKind::Standalone,
                        name: "QSDEMO".into(),
                        release: "V0R3M0".into(),
                        serial: String::new(),
                        model: String::new(),
                        build: env!("QSDEMO_BUILD_ID").into(),
                    };
                    end.sess.reply_msg(r.id, &base::Reply::Identify(me))
                }
                _ => end.sess.refuse_unknown(r.id),
            };
            done.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// A console's connection.
fn connect(
    id: &Identity,
    keep: &Shared,
    sock: UnixStream,
) -> Result<End<StreamOwned<ClientConnection, UnixStream>>, String> {
    let config = tls::client_config(id, Arc::new(keep.clone())).map_err(|e| e.to_string())?;
    let mut tls = StreamOwned::new(
        ClientConnection::new(config, tls::server_name()).map_err(|e| e.to_string())?,
        sock,
    );
    handshake(&mut tls.conn, &mut tls.sock)?;
    let sess = Session::new(
        Role::Console,
        hello(PeerKind::Console, "QSDEMO-MC"),
        true,
        Limits::default(),
        0,
    )
    .map_err(|e| e.to_string())?;
    let mut end = End {
        sess,
        io: tls,
        t0: Instant::now(),
        gone: false,
    };
    // Nothing may be asked before the managed side's hello.
    let stop = Instant::now() + DEADLINE;
    while !end.sess.is_open() {
        if end.gone || Instant::now() > stop {
            return Err("no hello from the managed side".into());
        }
        let _ = end.step();
    }
    Ok(end)
}

/// Pair, reconnect pinned, identify.
pub fn prove() -> Result<Vec<String>, String> {
    let host_id = Identity::make("QSDEMO", "", 1_794_488_700).map_err(|e| e.to_string())?;
    let console_id = Identity::make("QSDEMO-MC", "", 1_794_488_700).map_err(|e| e.to_string())?;
    let password = qsitrust::pair::one_time_password().map_err(|e| e.to_string())?;
    let host_keep = Shared::default();
    {
        let mut k = host_keep.0.lock().map_err(|_| "poisoned")?;
        k.password = Some(password.clone());
        k.pairing = true;
    }
    let console_keep = Shared::default();
    console_keep.0.lock().map_err(|_| "poisoned")?.pairing = true;

    let mut lines = Vec::new();
    // First connection: pair.
    let (a, b) = pair_socket()?;
    let (hid, hk) = (host_id.clone(), host_keep.clone());
    let server = thread::spawn(move || serve(&hid, &hk, a));
    let mut c = connect(&console_id, &console_keep, b)?;
    let refused = c.ask(&AnyRequest::Base(base::Request::Identify(Empty)))?;
    if refused.refusal().map(|(m, _)| m) != Some(msgid::NOT_PINNED) {
        return Err(format!("identify before a pin: {:?}", refused.refusal()));
    }
    let binding = tls::binding(&c.io.conn, &console_id).map_err(|e| e.to_string())?;
    let (console, start) =
        Console::start(&password, binding, "QSDEMO-MC").map_err(|e| e.to_string())?;
    let reply = c.ask(&AnyRequest::Pair(pair::Request::Start(pair::Start {
        spake: start.spake,
        name: start.name,
    })))?;
    let Ok(Some(pair::Reply::Answer(a))) = reply.typed::<pair::Reply>() else {
        return Err(format!("pairing start: {:?}", reply.refusal()));
    };
    let (confirm, host) = console
        .answer(&qsitrust::pair::Answer {
            spake: a.spake,
            confirm: a.confirm,
            name: a.name,
        })
        .map_err(|e| e.to_string())?;
    let done = c.ask(&AnyRequest::Pair(pair::Request::Confirm(pair::Confirm {
        confirm: confirm.confirm,
    })))?;
    if let Some((m, t)) = done.refusal() {
        return Err(format!("pairing: {m} {t}"));
    }
    c.sess.close("paired");
    c.step();
    drop(c);
    server
        .join()
        .map_err(|_| "the managed side panicked")??;
    if host.fingerprint != host_id.fingerprint() {
        return Err("paired with another certificate".into());
    }
    {
        let mut k = console_keep.0.lock().map_err(|_| "poisoned")?;
        k.pins.push(host.fingerprint);
        k.pairing = false;
    }
    lines.push(format!(
        "paired over TLS 1.3, Ed25519, managed side {}",
        &host.fingerprint.hex()[..16]
    ));

    // Second connection: pinned both ways, identify.
    let (a, b) = pair_socket()?;
    let (hid, hk) = (host_id.clone(), host_keep.clone());
    let server = thread::spawn(move || serve(&hid, &hk, a));
    let mut c = connect(&console_id, &console_keep, b)?;
    let reply = c.ask(&AnyRequest::Base(base::Request::Identify(Empty)))?;
    let Ok(Some(AnyReply::Base(base::Reply::Identify(me)))) = reply.decode() else {
        return Err(format!("identify: {:?}", reply.refusal()));
    };
    c.sess.close("done");
    c.step();
    drop(c);
    server
        .join()
        .map_err(|_| "the managed side panicked")??;
    lines.push(format!("identify answered pinned by {} {}", me.name, me.release));
    Ok(lines)
}
