// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One connection. A reader thread opens packets and posts them; the
//! connection's own thread owns the sending side and every piece of
//! state: key exchange, user authentication, channels. Sessions and
//! forwarded connections run on threads of their own and post their
//! output back, so a slow client blocks only its own channel (through
//! the channel's window).

mod channel;
pub mod credit;
pub mod forward;

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

pub use forward::{Duplex, ForwardListener, Forwarder, TcpForwarder};

use crate::auth::{Authed, Peer, Step, UserAuth};
use crate::error::{SshError, SshResult};
use crate::keys::public::SIG_ALGS;
use crate::server::Server;
use crate::session::Exit;
use crate::transport::algo::{self, KexInit, Negotiated};
use crate::transport::packet::{DirKeys, Opener, Sealer};
use crate::transport::{self, kex};
use crate::wire::{Reader, Writer, msg};
use channel::Channel;

/// What the connection's thread waits for.
pub(crate) enum Event {
    /// The client's identification line.
    Ident(String),
    /// A packet and its sequence number.
    Packet { seq: u32, data: Vec<u8> },
    /// The reader stopped.
    ReaderEnd(SshError),
    /// Output for a channel; its window was already taken.
    Data { chan: u32, ext: bool, data: Vec<u8> },
    /// A channel's input was read this far.
    Consumed { chan: u32, n: usize },
    /// A channel's session or forwarded connection ended.
    Finished { chan: u32, exit: Option<Exit> },
    /// A connection arrived on a remote forward.
    Incoming {
        bind: (String, u16),
        conn: Duplex,
    },
}

/// Our window for each channel and the largest packet we take.
pub(crate) const WINDOW: u32 = 2 * 1024 * 1024;
pub(crate) const MAX_PACKET: u32 = 32 * 1024;

fn reader<R: BufRead>(mut r: R, tx: &Sender<Event>, keys: &Receiver<(DirKeys, bool)>) {
    let ident = match transport::read_ident(&mut r) {
        Ok(v) => v,
        Err(e) => {
            let _ = tx.send(Event::ReaderEnd(e));
            return;
        }
    };
    if tx.send(Event::Ident(ident)).is_err() {
        return;
    }
    let mut o = Opener::default();
    loop {
        let seq = o.seq;
        match o.read(&mut r) {
            Ok(data) => {
                let newkeys = data.first() == Some(&msg::NEWKEYS);
                if tx.send(Event::Packet { seq, data }).is_err() {
                    return;
                }
                if newkeys {
                    match keys.recv() {
                        Ok((k, reset)) => o.rekey(k, reset),
                        Err(_) => return,
                    }
                }
            }
            Err(e) => {
                let _ = tx.send(Event::ReaderEnd(e));
                return;
            }
        }
    }
}

enum Kex {
    Idle,
    Sent { offer: KexInit, i_s: Vec<u8> },
    Agreed {
        neg: Negotiated,
        i_c: Vec<u8>,
        i_s: Vec<u8>,
        skip: bool,
    },
    AwaitNewKeys,
}

/// A remote forward being listened on.
struct Forward {
    stop: Arc<AtomicBool>,
}

struct Conn<'a, W: Write> {
    srv: &'a Server,
    w: W,
    sealer: Sealer,
    tx: Sender<Event>,
    keys_tx: Sender<(DirKeys, bool)>,
    peer: Peer,
    local: Peer,
    v_c: Option<String>,
    v_s: String,
    kex: Kex,
    session_id: Option<Vec<u8>>,
    strict: bool,
    deferred: Vec<Vec<u8>>,
    auth: Option<UserAuth<'a>>,
    authed: Option<Authed>,
    channels: HashMap<u32, Channel>,
    pending_opens: HashMap<u32, Duplex>,
    forwards: HashMap<(String, u16), Forward>,
    next_chan: u32,
    no_more_sessions: bool,
    kex_done_once: bool,
    started: Instant,
    last_rx: Instant,
    alive_missed: u32,
    done: bool,
}

/// Run a connection to its end.
pub(crate) fn run<R: BufRead + Send + 'static, W: Write>(
    srv: &Server,
    r: R,
    w: W,
    peer: Peer,
    local: Peer,
) -> SshResult<()> {
    let (tx, rx) = mpsc::channel();
    let (keys_tx, keys_rx) = mpsc::channel();
    {
        let tx = tx.clone();
        std::thread::spawn(move || reader(r, &tx, &keys_rx));
    }
    let now = Instant::now();
    let mut c = Conn {
        srv,
        w,
        sealer: Sealer::default(),
        tx,
        keys_tx,
        peer,
        local,
        v_c: None,
        v_s: transport::server_ident(),
        kex: Kex::Idle,
        session_id: None,
        strict: false,
        deferred: Vec::new(),
        auth: None,
        authed: None,
        channels: HashMap::new(),
        pending_opens: HashMap::new(),
        forwards: HashMap::new(),
        next_chan: 0,
        no_more_sessions: false,
        kex_done_once: false,
        started: now,
        last_rx: now,
        alive_missed: 0,
        done: false,
    };
    let r = c.main(&rx);
    c.shutdown();
    r
}

fn is_transport(kind: u8) -> bool {
    (1..50).contains(&kind)
}

impl<W: Write> Conn<'_, W> {
    fn main(&mut self, rx: &Receiver<Event>) -> SshResult<()> {
        let ident = format!("{}\r\n", self.v_s);
        self.w.write_all(ident.as_bytes())?;
        self.w.flush()?;
        self.send_kexinit()?;
        // Timers run at least once a second, busy or not.
        let mut last_tick = Instant::now();
        while !self.done {
            match rx.recv_timeout(Duration::from_secs(1)) {
                Ok(ev) => self.event(ev)?,
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            if last_tick.elapsed() >= Duration::from_secs(1) {
                last_tick = Instant::now();
                self.tick()?;
            }
        }
        Ok(())
    }

    fn shutdown(&mut self) {
        for (_, ch) in self.channels.drain() {
            ch.abandon();
        }
        for (_, f) in self.forwards.drain() {
            f.stop.store(true, Ordering::SeqCst);
        }
        self.pending_opens.clear();
    }

    // ---- sending

    fn write_packet(&mut self, payload: &[u8]) -> SshResult<()> {
        let bytes = self.sealer.seal(payload);
        self.w.write_all(&bytes)?;
        self.w.flush()?;
        Ok(())
    }

    fn send(&mut self, payload: Vec<u8>) -> SshResult<()> {
        let kind = payload.first().copied().unwrap_or(0);
        if !matches!(self.kex, Kex::Idle) && !is_transport(kind) {
            self.deferred.push(payload);
            return Ok(());
        }
        self.write_packet(&payload)
    }

    fn disconnect(&mut self, reason: u32, text: &str) -> SshError {
        let mut w = Writer::msg(msg::DISCONNECT);
        w.u32(reason).str(text).str("");
        let _ = self.write_packet(&w.finish());
        self.done = true;
        SshError::Refused {
            reason,
            text: text.to_string(),
        }
    }

    // ---- key exchange

    fn send_kexinit(&mut self) -> SshResult<()> {
        let mut kexes: Vec<&str> = algo::KEX.to_vec();
        if self.session_id.is_none() {
            kexes.push(algo::STRICT_S);
        }
        let offer = KexInit::offer(&kexes, &self.srv.keys.algorithms());
        let i_s = offer.encode();
        self.write_packet(&i_s)?;
        self.kex = Kex::Sent { offer, i_s };
        Ok(())
    }

    fn on_kexinit(&mut self, seq: u32, m: &[u8]) -> SshResult<()> {
        if matches!(self.kex, Kex::Idle) {
            self.send_kexinit()?;
        }
        let Kex::Sent { offer, i_s } = std::mem::replace(&mut self.kex, Kex::Idle) else {
            return Err(self.disconnect(msg::reason::PROTOCOL_ERROR, "unexpected KEXINIT"));
        };
        let client = KexInit::decode(m)?;
        let neg = match algo::negotiate(&client, &offer) {
            Ok(n) => n,
            Err(e) => {
                return Err(self.disconnect(msg::reason::KEY_EXCHANGE_FAILED, &e.to_string()));
            }
        };
        if self.session_id.is_none() {
            self.strict = neg.strict;
            if self.strict && seq != 0 {
                return Err(self.disconnect(
                    msg::reason::PROTOCOL_ERROR,
                    "strict key exchange: KEXINIT was not the first packet",
                ));
            }
        }
        let skip = neg.skip_guess;
        self.kex = Kex::Agreed {
            neg,
            i_c: m.to_vec(),
            i_s,
            skip,
        };
        Ok(())
    }

    fn on_kex_init(&mut self, m: &[u8]) -> SshResult<()> {
        let Kex::Agreed { neg, i_c, i_s, skip } = std::mem::replace(&mut self.kex, Kex::Idle)
        else {
            return Err(self.disconnect(msg::reason::PROTOCOL_ERROR, "unexpected key exchange message"));
        };
        if skip {
            self.kex = Kex::Agreed {
                neg,
                i_c,
                i_s,
                skip: false,
            };
            return Ok(());
        }
        let mut r = Reader::new(m);
        r.u8()?;
        let q_c = r.bytes()?;
        let Some((q_s, k)) = kex::server_exchange(&neg.kex, q_c) else {
            return Err(self.disconnect(msg::reason::KEY_EXCHANGE_FAILED, "the client's key exchange value is not valid"));
        };
        let srv = self.srv;
        let Some(key) = srv.keys.for_alg(&neg.host_key) else {
            return Err(self.disconnect(msg::reason::KEY_EXCHANGE_FAILED, "no host key for the algorithm"));
        };
        let k_s = key.public().blob();
        let v_c = self.v_c.clone().unwrap_or_default();
        let h = kex::exchange_hash(&kex::HashInput {
            v_c: v_c.as_bytes(),
            v_s: self.v_s.as_bytes(),
            i_c: &i_c,
            i_s: &i_s,
            k_s: &k_s,
            q_c,
            q_s: &q_s,
            k: &k,
        });
        let first = self.session_id.is_none();
        let session_id = self.session_id.get_or_insert_with(|| h.clone()).clone();
        let Some(sig) = key.sign(&neg.host_key, &h) else {
            return Err(self.disconnect(msg::reason::KEY_EXCHANGE_FAILED, "the host key could not sign"));
        };
        let mut w = Writer::msg(msg::KEX_ECDH_REPLY);
        w.bytes(&k_s).bytes(&q_s).bytes(&sig);
        self.write_packet(&w.finish())?;
        self.write_packet(&[msg::NEWKEYS])?;
        let (cs, sc) = transport::derive_keys(&neg, &k, &h, &session_id);
        self.sealer.rekey(sc, self.strict);
        let _ = self.keys_tx.send((cs, self.strict));
        if first && neg.ext_info {
            let mut w = Writer::msg(msg::EXT_INFO);
            w.u32(1).str("server-sig-algs").names(SIG_ALGS);
            self.write_packet(&w.finish())?;
        }
        self.kex = Kex::AwaitNewKeys;
        Ok(())
    }

    fn on_newkeys(&mut self) -> SshResult<()> {
        if !matches!(self.kex, Kex::AwaitNewKeys) {
            return Err(self.disconnect(msg::reason::PROTOCOL_ERROR, "unexpected NEWKEYS"));
        }
        self.kex = Kex::Idle;
        for p in std::mem::take(&mut self.deferred) {
            self.write_packet(&p)?;
        }
        Ok(())
    }

    // ---- events

    fn event(&mut self, ev: Event) -> SshResult<()> {
        match ev {
            Event::Ident(v) => {
                tracing::debug!(client = v, "client identification");
                self.v_c = Some(v);
                Ok(())
            }
            Event::Packet { seq, data } => {
                self.last_rx = Instant::now();
                self.alive_missed = 0;
                self.packet(seq, &data)
            }
            Event::ReaderEnd(e) => {
                self.done = true;
                match e {
                    SshError::Closed => Ok(()),
                    SshError::Integrity => {
                        Err(self.disconnect(msg::reason::MAC_ERROR, "a packet failed its integrity check"))
                    }
                    e => Err(e),
                }
            }
            Event::Data { chan, ext, data } => self.channel_data_out(chan, ext, &data),
            Event::Consumed { chan, n } => self.consumed(chan, n),
            Event::Finished { chan, exit } => self.finished(chan, exit),
            Event::Incoming { bind, conn } => self.incoming(&bind, conn),
        }
    }

    fn tick(&mut self) -> SshResult<()> {
        let cfg = &self.srv.config;
        if self.authed.is_none()
            && cfg.login_grace_time > 0
            && self.started.elapsed() > Duration::from_secs(u64::from(cfg.login_grace_time))
        {
            return Err(self.disconnect(msg::reason::BY_APPLICATION, "Sign-on was not completed in time"));
        }
        if cfg.client_alive_interval > 0
            && self.authed.is_some()
            && self.last_rx.elapsed() >= Duration::from_secs(u64::from(cfg.client_alive_interval))
        {
            if self.alive_missed >= cfg.client_alive_count_max {
                return Err(self.disconnect(msg::reason::BY_APPLICATION, "The client stopped answering"));
            }
            self.alive_missed += 1;
            self.last_rx = Instant::now();
            let mut w = Writer::msg(msg::GLOBAL_REQUEST);
            w.str("keepalive@openssh.com").bool(true);
            self.send(w.finish())?;
        }
        Ok(())
    }

    fn packet(&mut self, seq: u32, m: &[u8]) -> SshResult<()> {
        let Some(&kind) = m.first() else {
            return Err(self.disconnect(msg::reason::PROTOCOL_ERROR, "empty packet"));
        };
        let in_first_kex = !self.kex_done_once;
        if in_first_kex && !matches!(kind, msg::KEXINIT | msg::KEX_ECDH_INIT | msg::NEWKEYS | msg::DISCONNECT) {
            if self.strict || !matches!(kind, msg::IGNORE | msg::DEBUG | msg::UNIMPLEMENTED) {
                return Err(self.disconnect(
                    msg::reason::PROTOCOL_ERROR,
                    "unexpected message during key exchange",
                ));
            }
            return Ok(());
        }
        match kind {
            msg::DISCONNECT => {
                let mut r = Reader::new(m);
                r.u8()?;
                let reason = r.u32().unwrap_or(0);
                let text = r.str().unwrap_or("").to_string();
                tracing::debug!(reason, text, "client disconnected");
                self.done = true;
                Ok(())
            }
            msg::IGNORE | msg::DEBUG | msg::UNIMPLEMENTED => Ok(()),
            msg::KEXINIT => self.on_kexinit(seq, m),
            msg::KEX_ECDH_INIT => self.on_kex_init(m),
            msg::NEWKEYS => {
                self.on_newkeys()?;
                self.kex_done_once = true;
                Ok(())
            }
            msg::SERVICE_REQUEST => self.service_request(m),
            msg::USERAUTH_REQUEST => self.userauth(m),
            _ if self.authed.is_none() => {
                Err(self.disconnect(msg::reason::PROTOCOL_ERROR, "message before authentication"))
            }
            msg::GLOBAL_REQUEST => self.global_request(m),
            msg::REQUEST_SUCCESS | msg::REQUEST_FAILURE => Ok(()),
            msg::CHANNEL_OPEN => self.channel_open(m),
            msg::CHANNEL_OPEN_CONFIRMATION => self.open_confirmation(m),
            msg::CHANNEL_OPEN_FAILURE => self.open_failure(m),
            msg::CHANNEL_WINDOW_ADJUST => self.window_adjust(m),
            msg::CHANNEL_DATA | msg::CHANNEL_EXTENDED_DATA => self.channel_data_in(m),
            msg::CHANNEL_EOF => self.channel_eof(m),
            msg::CHANNEL_CLOSE => self.channel_close(m),
            msg::CHANNEL_REQUEST => self.channel_request(m),
            msg::CHANNEL_SUCCESS | msg::CHANNEL_FAILURE => Ok(()),
            _ => {
                let mut w = Writer::msg(msg::UNIMPLEMENTED);
                w.u32(seq);
                self.send(w.finish())
            }
        }
    }

    // ---- user authentication

    fn service_request(&mut self, m: &[u8]) -> SshResult<()> {
        let mut r = Reader::new(m);
        r.u8()?;
        let name = r.str()?;
        if name != "ssh-userauth" || self.auth.is_some() || self.authed.is_some() {
            return Err(self.disconnect(
                msg::reason::SERVICE_NOT_AVAILABLE,
                &format!("service {name} is not available"),
            ));
        }
        let srv = self.srv;
        let session_id = self.session_id.clone().unwrap_or_default();
        self.auth = Some(UserAuth::new(
            &srv.config,
            &*srv.deps.accounts,
            &*srv.deps.fs,
            self.peer.clone(),
            session_id,
        ));
        let mut w = Writer::msg(msg::SERVICE_ACCEPT);
        w.str("ssh-userauth");
        self.send(w.finish())
    }

    fn userauth(&mut self, m: &[u8]) -> SshResult<()> {
        if self.authed.is_some() {
            return Ok(());
        }
        let Some(auth) = self.auth.as_mut() else {
            return Err(self.disconnect(msg::reason::PROTOCOL_ERROR, "authentication before the service request"));
        };
        match auth.request(m)? {
            Step::Send(out) => {
                for p in out {
                    self.send(p)?;
                }
                Ok(())
            }
            Step::Done(out, who) => {
                for p in out {
                    self.send(p)?;
                }
                tracing::info!(user = who.account.name, method = ?who.method, peer = self.peer.addr, "signed on");
                self.authed = Some(*who);
                self.auth = None;
                Ok(())
            }
            Step::Disconnect(reason, text) => Err(self.disconnect(reason, &text)),
        }
    }
}
