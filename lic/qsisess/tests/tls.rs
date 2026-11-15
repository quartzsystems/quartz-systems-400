// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The session over real TLS on loopback, with `qsitrust`'s trust
//! rules: a console pairs with a host, connects again pinned and is
//! answered; a wrong password, a replayed exchange and an exchange
//! relayed through a third certificate all fail; before a pin, nothing
//! but pairing is answered; once the password is retired, a console
//! nobody pinned does not get past TLS.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use qsi::any::{AnyReply, AnyRequest};
use qsi::{Empty, Hello, PeerKind, base, caps, msgid, pair};
use qsisess::{Incoming, Limits, Reply, Role, Session};
use qsitrust::pair::{Console, Guard, Managed, Paired};
use qsitrust::{Fingerprint, Identity, Pins, tls};
use rustls::{ClientConnection, ServerConnection, StreamOwned};

const PASSWORD: &str = "7K3Q-9VNM-2HXD-PW4T";
const DEADLINE: Duration = Duration::from_secs(20);

// ---- The two ends' state, as a caller keeps it ----

#[derive(Debug, Default)]
struct HostState {
    pins: Vec<Fingerprint>,
    password: Option<String>,
    attempts: u32,
    failed: u32,
}

#[derive(Debug, Clone, Default)]
struct HostPins(Arc<Mutex<HostState>>);

impl Pins for HostPins {
    fn pinned(&self, peer: &Fingerprint) -> bool {
        self.0.lock().unwrap().pins.contains(peer)
    }
    fn pairing_open(&self) -> bool {
        self.0.lock().unwrap().password.is_some()
    }
}

struct HostGuard<'a>(&'a mut HostState);

impl Guard for HostGuard<'_> {
    fn password(&mut self) -> Option<String> {
        self.0.attempts += 1;
        if self.0.attempts > 5 {
            return None;
        }
        self.0.password.clone()
    }
    fn failed(&mut self) {
        self.0.failed += 1;
    }
    fn paired(&mut self, console: &Paired) {
        self.0.pins.push(console.fingerprint);
        self.0.password = None;
    }
}

#[derive(Debug, Default)]
struct ConsoleState {
    host: Option<Fingerprint>,
    pairing: bool,
}

#[derive(Debug, Clone, Default)]
struct ConsolePins(Arc<Mutex<ConsoleState>>);

impl Pins for ConsolePins {
    fn pinned(&self, peer: &Fingerprint) -> bool {
        self.0.lock().unwrap().host == Some(*peer)
    }
    fn pairing_open(&self) -> bool {
        self.0.lock().unwrap().pairing
    }
}

// ---- Driving a session over a TLS stream ----

fn now(t0: Instant) -> u64 {
    u64::try_from(t0.elapsed().as_millis()).unwrap()
}

fn quiet(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
    )
}

/// One end: a session over a stream.
struct End<S: Read + Write> {
    sess: Session,
    io: S,
    t0: Instant,
    gone: bool,
}

impl<S: Read + Write> End<S> {
    /// Send what is waiting, read what came, hand up what happened.
    fn step(&mut self) -> Vec<Incoming> {
        let out = self.sess.take_output();
        if !out.is_empty()
            && !self.gone
            && (self.io.write_all(&out).is_err() || self.io.flush().is_err())
        {
            self.gone = true;
        }
        let mut buf = [0u8; 16 * 1024];
        if !self.gone {
            match self.io.read(&mut buf) {
                Ok(0) => self.gone = true,
                Ok(n) => self.sess.feed(&buf[..n], now(self.t0)),
                Err(e) if quiet(&e) => {}
                Err(_) => self.gone = true,
            }
        }
        self.sess.tick(now(self.t0));
        std::iter::from_fn(|| self.sess.poll()).collect()
    }

    /// Step until `want` finds something, the peer is gone or the
    /// deadline passes.
    fn until<T>(&mut self, mut want: impl FnMut(Incoming) -> Option<T>) -> Option<T> {
        let end = Instant::now() + DEADLINE;
        while Instant::now() < end {
            for i in self.step() {
                if let Some(t) = want(i) {
                    return Some(t);
                }
            }
            if self.gone || self.sess.is_closed() {
                return None;
            }
        }
        None
    }

    /// Ask, and wait for the reply.
    fn ask(&mut self, req: &AnyRequest) -> Option<Reply> {
        let id = self.sess.request_any("OPERATOR", req).ok()?;
        self.until(|i| match i {
            Incoming::Reply(r) if r.id == id => Some(r),
            _ => None,
        })
    }
}

fn hello(kind: PeerKind, name: &str) -> Hello {
    Hello {
        version: qsi::VERSION,
        caps: caps::HOST_V1,
        kind,
        name: name.into(),
        release: "V1R1M0".into(),
        serial: if kind == PeerKind::Console {
            String::new()
        } else {
            "S0000001".into()
        },
    }
}

fn identity_reply() -> AnyReply {
    AnyReply::Base(base::Reply::Identify(base::Identity {
        kind: PeerKind::Host,
        name: "VS01".into(),
        release: "V1R1M0".into(),
        serial: "S0000001".into(),
        model: String::new(),
        build: "test".into(),
    }))
}

// ---- The host ----

/// Serve connections on `listener` until `count` have ended.
fn host(
    listener: TcpListener,
    id: Identity,
    state: HostPins,
    count: usize,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let config = tls::server_config(&id, Arc::new(state.clone())).unwrap();
        for _ in 0..count {
            let (sock, _) = listener.accept().unwrap();
            sock.set_read_timeout(Some(Duration::from_millis(10)))
                .unwrap();
            let mut tls = StreamOwned::new(ServerConnection::new(config.clone()).unwrap(), sock);
            let t0 = Instant::now();
            // The handshake; a refused console ends here.
            let mut ok = true;
            while tls.conn.is_handshaking() {
                match tls.conn.complete_io(&mut tls.sock) {
                    Ok(_) => {}
                    Err(e) if quiet(&e) && t0.elapsed() < DEADLINE => {}
                    Err(_) => {
                        ok = false;
                        break;
                    }
                }
            }
            if !ok {
                continue;
            }
            let peer = tls::peer(&tls.conn, &state).unwrap();
            let sess = Session::new(
                Role::Managed,
                hello(PeerKind::Host, "VS01"),
                peer.pinned,
                Limits::default(),
                0,
            )
            .unwrap();
            serve(
                End {
                    sess,
                    io: tls,
                    t0,
                    gone: false,
                },
                &id,
                &state,
            );
        }
    })
}

/// One connection: pairing and identify.
fn serve(mut end: End<StreamOwned<ServerConnection, TcpStream>>, id: &Identity, state: &HostPins) {
    let mut pending: Option<Managed> = None;
    let deadline = Instant::now() + DEADLINE;
    while !end.gone && !end.sess.is_closed() && Instant::now() < deadline {
        for i in end.step() {
            let Incoming::Request(r) = i else { continue };
            match r.decode() {
                Ok(Some(AnyRequest::Pair(pair::Request::Start(s)))) => {
                    let binding = tls::binding(&end.io.conn, id).unwrap();
                    let start = qsitrust::pair::Start {
                        spake: s.spake,
                        name: s.name,
                    };
                    let mut st = state.0.lock().unwrap();
                    match Managed::answer(&mut HostGuard(&mut st), binding, &start, "VS01") {
                        Ok((m, a)) => {
                            pending = Some(m);
                            let reply = pair::Reply::Answer(pair::Answer {
                                spake: a.spake,
                                confirm: a.confirm,
                                name: a.name,
                            });
                            end.sess.reply_msg(r.id, &reply).unwrap();
                        }
                        Err(_) => end
                            .sess
                            .refuse(r.id, msgid::NO_PAIRING, "no pairing")
                            .unwrap(),
                    }
                }
                Ok(Some(AnyRequest::Pair(pair::Request::Confirm(c)))) => {
                    let Some(m) = pending.take() else {
                        end.sess
                            .refuse(r.id, msgid::PAIRING_ORDER, "no start")
                            .unwrap();
                        continue;
                    };
                    let mut st = state.0.lock().unwrap();
                    let confirm = qsitrust::pair::Confirm { confirm: c.confirm };
                    match m.confirm(&mut HostGuard(&mut st), &confirm) {
                        Ok(_) => end.sess.reply_msg(r.id, &pair::Reply::Done(Empty)).unwrap(),
                        Err(_) => end
                            .sess
                            .refuse(r.id, msgid::PAIRING_FAILED, "pairing failed")
                            .unwrap(),
                    }
                }
                Ok(Some(AnyRequest::Base(base::Request::Identify(_)))) => {
                    end.sess.reply_any(r.id, &identity_reply()).unwrap();
                }
                _ => end.sess.refuse_unknown(r.id).unwrap(),
            }
        }
    }
}

// ---- The console ----

type Client = End<StreamOwned<ClientConnection, TcpStream>>;

/// Connect, finish TLS, wait for the session to open.
fn connect(addr: SocketAddr, id: &Identity, pins: &ConsolePins) -> Option<Client> {
    let config = tls::client_config(id, Arc::new(pins.clone())).unwrap();
    let sock = TcpStream::connect(addr).unwrap();
    sock.set_read_timeout(Some(Duration::from_millis(10)))
        .unwrap();
    let mut tls = StreamOwned::new(
        ClientConnection::new(config, tls::server_name()).unwrap(),
        sock,
    );
    let t0 = Instant::now();
    while tls.conn.is_handshaking() {
        match tls.conn.complete_io(&mut tls.sock) {
            Ok(_) => {}
            Err(e) if quiet(&e) && t0.elapsed() < DEADLINE => {}
            Err(_) => return None,
        }
    }
    let sess = Session::new(
        Role::Console,
        hello(PeerKind::Console, "MC01"),
        true,
        Limits::default(),
        0,
    )
    .unwrap();
    let mut end = End {
        sess,
        io: tls,
        t0,
        gone: false,
    };
    end.until(|i| matches!(i, Incoming::Ready { .. }).then_some(()))?;
    Some(end)
}

fn start_req(s: &qsitrust::pair::Start) -> AnyRequest {
    AnyRequest::Pair(pair::Request::Start(pair::Start {
        spake: s.spake.clone(),
        name: s.name.clone(),
    }))
}

fn confirm_req(c: &[u8]) -> AnyRequest {
    AnyRequest::Pair(pair::Request::Confirm(pair::Confirm {
        confirm: c.to_vec(),
    }))
}

fn answer_of(r: &Reply) -> Option<qsitrust::pair::Answer> {
    match r.typed::<pair::Reply>() {
        Ok(Some(pair::Reply::Answer(a))) => Some(qsitrust::pair::Answer {
            spake: a.spake,
            confirm: a.confirm,
            name: a.name,
        }),
        _ => None,
    }
}

/// What one pairing attempt came to, and its messages.
struct Attempt {
    paired: Option<Paired>,
    start: qsitrust::pair::Start,
    confirm: Option<Vec<u8>>,
    last: Option<Reply>,
}

/// Pair over `c` with `password`; on a failed check at the console it
/// still sends a confirmation (zeros), as a hostile console would.
fn pair_with(c: &mut Client, id: &Identity, password: &str) -> Attempt {
    let binding = tls::binding(&c.io.conn, id).unwrap();
    let (console, start) = Console::start(password, binding, "MC01").unwrap();
    let reply = c.ask(&start_req(&start)).unwrap();
    let Some(answer) = answer_of(&reply) else {
        return Attempt {
            paired: None,
            start,
            confirm: None,
            last: Some(reply),
        };
    };
    let Ok((confirm, host)) = console.answer(&answer) else {
        let done = c.ask(&confirm_req(&[0; 32])).unwrap();
        return Attempt {
            paired: None,
            start,
            confirm: None,
            last: Some(done),
        };
    };
    let done = c.ask(&confirm_req(&confirm.confirm)).unwrap();
    let ok = done.refusal().is_none();
    Attempt {
        paired: ok.then_some(host),
        start,
        confirm: Some(confirm.confirm),
        last: Some(done),
    }
}

fn identify(c: &mut Client) -> Reply {
    c.ask(&AnyRequest::Base(base::Request::Identify(Empty)))
        .unwrap()
}

// ---- The tests ----

fn ids() -> (Identity, Identity) {
    (
        Identity::make("VS01", "S0000001", 1_794_488_700).unwrap(),
        Identity::make("MC01", "", 1_794_488_700).unwrap(),
    )
}

fn listener() -> (TcpListener, SocketAddr) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let a = l.local_addr().unwrap();
    (l, a)
}

#[test]
fn pair_then_reconnect_pinned() {
    let (host_id, console_id) = ids();
    let (lsn, addr) = listener();
    let hs = HostPins::default();
    hs.0.lock().unwrap().password = Some(PASSWORD.into());
    let served = host(lsn, host_id.clone(), hs.clone(), 3);

    // Unpinned, pairing: nothing but pairing is answered.
    let cp = ConsolePins::default();
    cp.0.lock().unwrap().pairing = true;
    let mut c = connect(addr, &console_id, &cp).unwrap();
    let r = identify(&mut c);
    assert_eq!(r.refusal().map(|(m, _)| m), Some(msgid::NOT_PINNED));
    // Typed as the operator might: lower case, spaces.
    let a = pair_with(&mut c, &console_id, "7k3q 9vnm 2hxd pw4t");
    let host_seen = a.paired.expect("paired");
    assert_eq!(host_seen.fingerprint, host_id.fingerprint());
    assert_eq!(host_seen.name, "VS01");
    assert_eq!(hs.0.lock().unwrap().pins, vec![console_id.fingerprint()]);
    assert!(hs.0.lock().unwrap().password.is_none());
    c.sess.close("paired");
    c.step();
    drop(c);

    // Pinned both ways now: identify is answered.
    {
        let mut st = cp.0.lock().unwrap();
        st.host = Some(host_seen.fingerprint);
        st.pairing = false;
    }
    let mut c = connect(addr, &console_id, &cp).unwrap();
    let r = identify(&mut c);
    assert_eq!(
        r.decode().unwrap().unwrap(),
        identity_reply(),
        "{:?}",
        r.refusal()
    );
    c.sess.close("done");
    c.step();
    drop(c);

    // A console nobody pinned, with no password out, does not get a
    // session: the host refuses it in TLS.
    let stranger = Identity::make("MC99", "", 0).unwrap();
    let sp = ConsolePins::default();
    sp.0.lock().unwrap().host = Some(host_id.fingerprint());
    if let Some(mut c) = connect(addr, &stranger, &sp) {
        assert!(
            c.ask(&AnyRequest::Base(base::Request::Identify(Empty)))
                .is_none()
        );
    }
    served.join().unwrap();
}

#[test]
fn a_wrong_password_fails() {
    let (host_id, console_id) = ids();
    let (lsn, addr) = listener();
    let hs = HostPins::default();
    hs.0.lock().unwrap().password = Some(PASSWORD.into());
    let served = host(lsn, host_id, hs.clone(), 1);
    let cp = ConsolePins::default();
    cp.0.lock().unwrap().pairing = true;
    let mut c = connect(addr, &console_id, &cp).unwrap();
    let a = pair_with(&mut c, &console_id, "7K3Q-9VNM-2HXD-PW4X");
    assert!(a.paired.is_none());
    assert_eq!(
        a.last.unwrap().refusal().map(|(m, _)| m),
        Some(msgid::PAIRING_FAILED)
    );
    c.sess.close("done");
    c.step();
    drop(c);
    served.join().unwrap();
    let st = hs.0.lock().unwrap();
    assert!(st.pins.is_empty());
    assert_eq!(st.failed, 1);
}

#[test]
fn a_replayed_exchange_fails() {
    let (host_id, console_id) = ids();
    let (lsn, addr) = listener();
    let hs = HostPins::default();
    hs.0.lock().unwrap().password = Some(PASSWORD.into());
    let served = host(lsn, host_id, hs.clone(), 2);
    let cp = ConsolePins::default();
    cp.0.lock().unwrap().pairing = true;

    let mut c = connect(addr, &console_id, &cp).unwrap();
    let first = pair_with(&mut c, &console_id, PASSWORD);
    assert!(first.paired.is_some());
    c.sess.close("done");
    c.step();
    drop(c);

    // The same messages on a new connection, the password out again
    // and the console's pin forgotten.
    {
        let mut st = hs.0.lock().unwrap();
        st.pins.clear();
        st.password = Some(PASSWORD.into());
    }
    let mut c = connect(addr, &console_id, &cp).unwrap();
    let r = c.ask(&start_req(&first.start)).unwrap();
    assert!(answer_of(&r).is_some());
    let r = c.ask(&confirm_req(&first.confirm.unwrap())).unwrap();
    assert_eq!(r.refusal().map(|(m, _)| m), Some(msgid::PAIRING_FAILED));
    c.sess.close("done");
    c.step();
    drop(c);
    served.join().unwrap();
    assert!(hs.0.lock().unwrap().pins.is_empty());
}

/// Pass bytes both ways between two TLS streams until either ends.
fn relay<A: Read + Write, B: Read + Write>(mut a: A, mut b: B) {
    let mut buf = [0u8; 16 * 1024];
    let end = Instant::now() + DEADLINE;
    while Instant::now() < end {
        match a.read(&mut buf) {
            Ok(0) => return,
            Ok(n) => {
                if b.write_all(&buf[..n]).and_then(|()| b.flush()).is_err() {
                    return;
                }
            }
            Err(e) if quiet(&e) => {}
            Err(_) => return,
        }
        match b.read(&mut buf) {
            Ok(0) => return,
            Ok(n) => {
                if a.write_all(&buf[..n]).and_then(|()| a.flush()).is_err() {
                    return;
                }
            }
            Err(e) if quiet(&e) => {}
            Err(_) => return,
        }
    }
}

#[test]
fn an_exchange_relayed_through_a_third_certificate_fails() {
    let (host_id, console_id) = ids();
    let (lsn, host_addr) = listener();
    let hs = HostPins::default();
    hs.0.lock().unwrap().password = Some(PASSWORD.into());
    let served = host(lsn, host_id, hs.clone(), 1);

    // The relay: TLS with its own certificate to each side, the
    // plaintext passed through unchanged. It even knows the password;
    // it cannot make the confirmations hold.
    let relay_id = Identity::make("RELAY", "", 0).unwrap();
    let (rl, relay_addr) = listener();
    let r = thread::spawn(move || {
        let open = ConsolePins::default();
        open.0.lock().unwrap().pairing = true;
        let sc = tls::server_config(&relay_id, Arc::new(open.clone())).unwrap();
        let cc = tls::client_config(&relay_id, Arc::new(open)).unwrap();
        let (down, _) = rl.accept().unwrap();
        down.set_read_timeout(Some(Duration::from_millis(5)))
            .unwrap();
        let up = TcpStream::connect(host_addr).unwrap();
        up.set_read_timeout(Some(Duration::from_millis(5))).unwrap();
        let a = StreamOwned::new(ServerConnection::new(sc).unwrap(), down);
        let b = StreamOwned::new(ClientConnection::new(cc, tls::server_name()).unwrap(), up);
        relay(a, b);
    });

    let cp = ConsolePins::default();
    cp.0.lock().unwrap().pairing = true;
    let mut c = connect(relay_addr, &console_id, &cp).unwrap();
    let a = pair_with(&mut c, &console_id, PASSWORD);
    assert!(a.paired.is_none());
    assert_eq!(
        a.last.unwrap().refusal().map(|(m, _)| m),
        Some(msgid::PAIRING_FAILED)
    );
    c.sess.close("done");
    c.step();
    drop(c);
    r.join().unwrap();
    served.join().unwrap();
    let st = hs.0.lock().unwrap();
    assert!(st.pins.is_empty());
    assert_eq!(st.failed, 1);
}
