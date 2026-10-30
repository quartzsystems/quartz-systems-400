// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A small blocking SSH client over the same transport code: key
//! exchange with host key verification, password and key sign-on,
//! session channels, exec, shell and subsystems. The test suite drives
//! the server with it; it also serves tools that talk to an SSH server.

pub mod sftp;

use std::collections::{HashMap, VecDeque};
use std::io::{BufReader, Read, Write};

use crate::error::{SshError, SshResult};
use crate::keys::{PrivateKey, PublicKey};
use crate::transport::algo::{self, KexInit};
use crate::transport::kex::{self, ClientKex};
use crate::transport::packet::{Opener, Sealer};
use crate::transport::{self as tp, read_ident};
use crate::wire::{Reader, Writer, msg};

const WINDOW: u32 = 16 * 1024 * 1024;
const MAX_PACKET: u32 = 32 * 1024;

/// What the client offers.
#[derive(Debug, Clone)]
pub struct Options {
    /// Key exchange methods, preferred first.
    pub kex: Vec<String>,
    /// Host key algorithms, preferred first.
    pub host_key: Vec<String>,
    /// The identification string.
    pub ident: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            kex: algo::KEX.iter().map(|s| (*s).to_string()).collect(),
            host_key: ["ssh-ed25519", "ecdsa-sha2-nistp256", "rsa-sha2-512", "rsa-sha2-256"]
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            ident: "SSH-2.0-QS400client_1".into(),
        }
    }
}

/// Something the server sent on a channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incoming {
    /// `CHANNEL_DATA`.
    Data(u32, Vec<u8>),
    /// `CHANNEL_EXTENDED_DATA` (standard error).
    Stderr(u32, Vec<u8>),
    /// `CHANNEL_EOF`.
    Eof(u32),
    /// `CHANNEL_CLOSE`.
    Close(u32),
    /// `exit-status`.
    ExitStatus(u32, u32),
    /// `CHANNEL_SUCCESS`.
    Success(u32),
    /// `CHANNEL_FAILURE`.
    Failure(u32),
    /// A channel the server opened (`forwarded-tcpip`): our id for it.
    Opened(u32),
    /// Any other message.
    Other(Vec<u8>),
}

/// The result of an `exec`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Output {
    /// Standard output.
    pub stdout: Vec<u8>,
    /// Standard error.
    pub stderr: Vec<u8>,
    /// `exit-status`, if sent.
    pub status: Option<u32>,
}

struct Chan {
    remote: u32,
    window: u64,
    consumed: u32,
}

/// A connected client.
pub struct Client<R: Read, W: Write> {
    r: BufReader<R>,
    w: W,
    opener: Opener,
    sealer: Sealer,
    session_id: Vec<u8>,
    /// The server's host key.
    pub host_key: PublicKey,
    /// The server's identification string.
    pub server_ident: String,
    /// `server-sig-algs` from `EXT_INFO`.
    pub server_sig_algs: Vec<String>,
    /// Banner text received during sign-on.
    pub banner: Option<String>,
    chans: HashMap<u32, Chan>,
    next: u32,
    queue: VecDeque<Vec<u8>>,
}

fn proto(text: &str) -> SshError {
    SshError::Protocol(text.into())
}

impl<R: Read, W: Write> Client<R, W> {
    /// Connect: identification and key exchange, then `ssh-userauth`.
    pub fn connect(r: R, mut w: W, opts: &Options) -> SshResult<Self> {
        w.write_all(format!("{}\r\n", opts.ident).as_bytes())?;
        w.flush()?;
        let mut r = BufReader::new(r);
        let server_ident = read_ident(&mut r)?;
        let mut c = Self {
            r,
            w,
            opener: Opener::default(),
            sealer: Sealer::default(),
            session_id: Vec::new(),
            host_key: PublicKey::Ed25519([0; 32]),
            server_ident,
            server_sig_algs: Vec::new(),
            banner: None,
            chans: HashMap::new(),
            next: 0,
            queue: VecDeque::new(),
        };
        c.kex(opts)?;
        let mut sr = Writer::msg(msg::SERVICE_REQUEST);
        sr.str("ssh-userauth");
        c.send(&sr.finish())?;
        let m = c.next_msg()?;
        if m.first() != Some(&msg::SERVICE_ACCEPT) {
            return Err(proto("expected SERVICE_ACCEPT"));
        }
        Ok(c)
    }

    fn send(&mut self, payload: &[u8]) -> SshResult<()> {
        let b = self.sealer.seal(payload);
        self.w.write_all(&b)?;
        self.w.flush()?;
        Ok(())
    }

    fn read_raw(&mut self) -> SshResult<Vec<u8>> {
        loop {
            let p = self.opener.read(&mut self.r)?;
            match p.first() {
                Some(&msg::IGNORE | &msg::DEBUG) => {}
                Some(&msg::DISCONNECT) => {
                    let mut r = Reader::new(&p);
                    r.u8()?;
                    let reason = r.u32()?;
                    let text = r.str()?.to_string();
                    return Err(SshError::Disconnected { reason, text });
                }
                Some(&msg::EXT_INFO) => {
                    let mut r = Reader::new(&p);
                    r.u8()?;
                    let n = r.u32()?;
                    for _ in 0..n {
                        let name = r.str()?;
                        let value = r.str()?;
                        if name == "server-sig-algs" {
                            self.server_sig_algs = value.split(',').map(str::to_string).collect();
                        }
                    }
                }
                Some(&msg::GLOBAL_REQUEST) => {
                    let mut r = Reader::new(&p);
                    r.u8()?;
                    r.str()?;
                    if r.bool()? {
                        self.send(&[msg::REQUEST_FAILURE])?;
                    }
                }
                _ => return Ok(p),
            }
        }
    }

    fn next_msg(&mut self) -> SshResult<Vec<u8>> {
        match self.queue.pop_front() {
            Some(m) => Ok(m),
            None => self.read_raw(),
        }
    }

    fn kex(&mut self, opts: &Options) -> SshResult<()> {
        let mut kexes: Vec<&str> = opts.kex.iter().map(String::as_str).collect();
        kexes.extend([algo::EXT_INFO_C, algo::STRICT_C]);
        let hk: Vec<&str> = opts.host_key.iter().map(String::as_str).collect();
        let ours = KexInit::offer(&kexes, &hk);
        let i_c = ours.encode();
        self.send(&i_c)?;
        let i_s = self.read_raw()?;
        let theirs = KexInit::decode(&i_s)?;
        let neg = algo::negotiate(&ours, &theirs)?;
        let (state, q_c) =
            ClientKex::start(&neg.kex).ok_or_else(|| SshError::Kex("method not supported".into()))?;
        let mut w = Writer::msg(msg::KEX_ECDH_INIT);
        w.bytes(&q_c);
        self.send(&w.finish())?;
        let reply = self.read_raw()?;
        let mut r = Reader::new(&reply);
        if r.u8()? != msg::KEX_ECDH_REPLY {
            return Err(proto("expected KEX_ECDH_REPLY"));
        }
        let k_s = r.bytes()?.to_vec();
        let q_s = r.bytes()?.to_vec();
        let sig = r.bytes()?.to_vec();
        let k = state
            .finish(&q_s)
            .ok_or_else(|| SshError::Kex("the server's value is not valid".into()))?;
        let host_key =
            PublicKey::from_blob(&k_s).ok_or_else(|| SshError::Kex("host key not readable".into()))?;
        let server_ident = self.server_ident.clone();
        let h = kex::exchange_hash(&kex::HashInput {
            v_c: opts.ident.as_bytes(),
            v_s: server_ident.as_bytes(),
            i_c: &i_c,
            i_s: &i_s,
            k_s: &k_s,
            q_c: &q_c,
            q_s: &q_s,
            k: &k,
        });
        if !host_key.signs_with(&neg.host_key) || !host_key.verify(&sig, &h) {
            return Err(SshError::Kex("the host key signature does not verify".into()));
        }
        self.host_key = host_key;
        self.session_id.clone_from(&h);
        let (cs, sc) = tp::derive_keys(&neg, &k, &h, &h);
        self.send(&[msg::NEWKEYS])?;
        self.sealer.rekey(cs, neg.strict);
        let nk = self.read_raw()?;
        if nk.first() != Some(&msg::NEWKEYS) {
            return Err(proto("expected NEWKEYS"));
        }
        self.opener.rekey(sc, neg.strict);
        Ok(())
    }

    fn auth_reply(&mut self) -> SshResult<Result<(), Vec<String>>> {
        loop {
            let m = self.next_msg()?;
            let mut r = Reader::new(&m);
            match r.u8()? {
                msg::USERAUTH_BANNER => self.banner = Some(r.str()?.to_string()),
                msg::USERAUTH_SUCCESS => return Ok(Ok(())),
                msg::USERAUTH_FAILURE => {
                    return Ok(Err(r.names()?.into_iter().map(str::to_string).collect()));
                }
                msg::USERAUTH_PK_OK => return Ok(Err(vec!["pk-ok".into()])),
                _ => return Err(proto("unexpected answer to USERAUTH_REQUEST")),
            }
        }
    }

    /// `none`: the methods the server offers.
    pub fn auth_none(&mut self, user: &str) -> SshResult<Vec<String>> {
        let mut w = Writer::msg(msg::USERAUTH_REQUEST);
        w.str(user).str("ssh-connection").str("none");
        self.send(&w.finish())?;
        Ok(self.auth_reply()?.err().unwrap_or_default())
    }

    /// `password`: whether it was accepted.
    pub fn auth_password(&mut self, user: &str, password: &str) -> SshResult<bool> {
        let mut w = Writer::msg(msg::USERAUTH_REQUEST);
        w.str(user)
            .str("ssh-connection")
            .str("password")
            .bool(false)
            .str(password);
        self.send(&w.finish())?;
        Ok(self.auth_reply()?.is_ok())
    }

    /// `publickey` without a signature: whether the server would take
    /// the key.
    pub fn query_key(&mut self, user: &str, key: &PrivateKey) -> SshResult<bool> {
        let alg = key.sig_algs()[0];
        let mut w = Writer::msg(msg::USERAUTH_REQUEST);
        w.str(user)
            .str("ssh-connection")
            .str("publickey")
            .bool(false)
            .str(alg)
            .bytes(&key.public().blob());
        self.send(&w.finish())?;
        Ok(matches!(self.auth_reply()?, Err(v) if v == ["pk-ok"]))
    }

    /// `publickey` with a signature: whether it was accepted.
    pub fn auth_key(&mut self, user: &str, key: &PrivateKey) -> SshResult<bool> {
        let alg = key.sig_algs()[0];
        let blob = key.public().blob();
        let mut data = Writer::new();
        data.bytes(&self.session_id)
            .u8(msg::USERAUTH_REQUEST)
            .str(user)
            .str("ssh-connection")
            .str("publickey")
            .bool(true)
            .str(alg)
            .bytes(&blob);
        let sig = key
            .sign(alg, &data.finish())
            .ok_or_else(|| proto("the key could not sign"))?;
        let mut w = Writer::msg(msg::USERAUTH_REQUEST);
        w.str(user)
            .str("ssh-connection")
            .str("publickey")
            .bool(true)
            .str(alg)
            .bytes(&blob)
            .bytes(&sig);
        self.send(&w.finish())?;
        Ok(self.auth_reply()?.is_ok())
    }

    /// Open a channel of `kind` with extra open data: our id for it, or
    /// the failure text.
    pub fn open(&mut self, kind: &str, extra: &[u8]) -> SshResult<Result<u32, String>> {
        let id = self.next;
        self.next += 1;
        let mut w = Writer::msg(msg::CHANNEL_OPEN);
        w.str(kind).u32(id).u32(WINDOW).u32(MAX_PACKET).raw(extra);
        self.send(&w.finish())?;
        loop {
            let m = self.read_raw()?;
            let mut r = Reader::new(&m);
            match r.u8()? {
                msg::CHANNEL_OPEN_CONFIRMATION if r.u32()? == id => {
                    let remote = r.u32()?;
                    let window = u64::from(r.u32()?);
                    self.chans.insert(
                        id,
                        Chan {
                            remote,
                            window,
                            consumed: 0,
                        },
                    );
                    return Ok(Ok(id));
                }
                msg::CHANNEL_OPEN_FAILURE if r.u32()? == id => {
                    r.u32()?;
                    return Ok(Err(r.str()?.to_string()));
                }
                _ => self.queue.push_back(m),
            }
        }
    }

    /// Open a session channel.
    pub fn session(&mut self) -> SshResult<u32> {
        self.open("session", &[])?.map_err(|e| proto(&e))
    }

    fn remote(&self, chan: u32) -> SshResult<u32> {
        self.chans
            .get(&chan)
            .map(|c| c.remote)
            .ok_or_else(|| proto("no such channel"))
    }

    /// Send a channel request with `want_reply`; whether it succeeded.
    pub fn request(&mut self, chan: u32, kind: &str, data: &[u8]) -> SshResult<bool> {
        let remote = self.remote(chan)?;
        let mut w = Writer::msg(msg::CHANNEL_REQUEST);
        w.u32(remote).str(kind).bool(true).raw(data);
        self.send(&w.finish())?;
        loop {
            match self.recv()? {
                Incoming::Success(c) if c == chan => return Ok(true),
                Incoming::Failure(c) if c == chan => return Ok(false),
                other => self.requeue(other),
            }
        }
    }

    fn requeue(&mut self, i: Incoming) {
        // Put channel traffic back as a message for later `recv`.
        let mut w;
        match i {
            Incoming::Data(c, d) => {
                w = Writer::msg(msg::CHANNEL_DATA);
                w.u32(c).bytes(&d);
            }
            Incoming::Stderr(c, d) => {
                w = Writer::msg(msg::CHANNEL_EXTENDED_DATA);
                w.u32(c).u32(1).bytes(&d);
            }
            Incoming::Eof(c) => {
                w = Writer::msg(msg::CHANNEL_EOF);
                w.u32(c);
            }
            Incoming::Close(c) => {
                w = Writer::msg(msg::CHANNEL_CLOSE);
                w.u32(c);
            }
            Incoming::ExitStatus(c, n) => {
                w = Writer::msg(msg::CHANNEL_REQUEST);
                w.u32(c).str("exit-status").bool(false).u32(n);
            }
            Incoming::Success(c) => {
                w = Writer::msg(msg::CHANNEL_SUCCESS);
                w.u32(c);
            }
            Incoming::Failure(c) => {
                w = Writer::msg(msg::CHANNEL_FAILURE);
                w.u32(c);
            }
            Incoming::Opened(_) | Incoming::Other(_) => return,
        }
        self.queue.push_back(w.finish());
    }

    /// `pty-req`.
    pub fn pty(&mut self, chan: u32, term: &str, cols: u32, rows: u32) -> SshResult<bool> {
        let mut w = Writer::new();
        w.str(term).u32(cols).u32(rows).u32(0).u32(0).bytes(&[0]);
        self.request(chan, "pty-req", &w.finish())
    }

    /// `exec`.
    pub fn exec(&mut self, chan: u32, cmd: &str) -> SshResult<bool> {
        let mut w = Writer::new();
        w.str(cmd);
        self.request(chan, "exec", &w.finish())
    }

    /// `subsystem`.
    pub fn subsystem(&mut self, chan: u32, name: &str) -> SshResult<bool> {
        let mut w = Writer::new();
        w.str(name);
        self.request(chan, "subsystem", &w.finish())
    }

    /// `shell`.
    pub fn shell(&mut self, chan: u32) -> SshResult<bool> {
        self.request(chan, "shell", &[])
    }

    /// Send data, waiting for window as needed.
    pub fn send_data(&mut self, chan: u32, mut data: &[u8]) -> SshResult<()> {
        while !data.is_empty() {
            let (remote, window) = {
                let c = self.chans.get(&chan).ok_or_else(|| proto("no such channel"))?;
                (c.remote, c.window)
            };
            if window == 0 {
                let i = self.recv()?;
                self.requeue(i);
                continue;
            }
            let n = data
                .len()
                .min(MAX_PACKET as usize)
                .min(usize::try_from(window).unwrap_or(usize::MAX));
            let mut w = Writer::msg(msg::CHANNEL_DATA);
            w.u32(remote).bytes(&data[..n]);
            self.send(&w.finish())?;
            if let Some(c) = self.chans.get_mut(&chan) {
                c.window -= n as u64;
            }
            data = &data[n..];
        }
        Ok(())
    }

    /// `CHANNEL_EOF`.
    pub fn send_eof(&mut self, chan: u32) -> SshResult<()> {
        let remote = self.remote(chan)?;
        let mut w = Writer::msg(msg::CHANNEL_EOF);
        w.u32(remote);
        self.send(&w.finish())
    }

    /// `CHANNEL_CLOSE`.
    pub fn close(&mut self, chan: u32) -> SshResult<()> {
        let remote = self.remote(chan)?;
        let mut w = Writer::msg(msg::CHANNEL_CLOSE);
        w.u32(remote);
        self.send(&w.finish())
    }

    /// Send a global request; the reply payload, or `None` on failure.
    pub fn global(&mut self, name: &str, data: &[u8]) -> SshResult<Option<Vec<u8>>> {
        let mut w = Writer::msg(msg::GLOBAL_REQUEST);
        w.str(name).bool(true).raw(data);
        self.send(&w.finish())?;
        loop {
            let m = self.read_raw()?;
            match m.first() {
                Some(&msg::REQUEST_SUCCESS) => return Ok(Some(m[1..].to_vec())),
                Some(&msg::REQUEST_FAILURE) => return Ok(None),
                _ => self.queue.push_back(m),
            }
        }
    }

    /// The next channel event.
    pub fn recv(&mut self) -> SshResult<Incoming> {
        let m = self.next_msg()?;
        let mut r = Reader::new(&m);
        let kind = r.u8()?;
        Ok(match kind {
            msg::CHANNEL_DATA | msg::CHANNEL_EXTENDED_DATA => {
                let c = r.u32()?;
                if kind == msg::CHANNEL_EXTENDED_DATA {
                    r.u32()?;
                }
                let d = r.bytes()?.to_vec();
                self.consume(c, d.len())?;
                if kind == msg::CHANNEL_DATA {
                    Incoming::Data(c, d)
                } else {
                    Incoming::Stderr(c, d)
                }
            }
            msg::CHANNEL_WINDOW_ADJUST => {
                let c = r.u32()?;
                let n = r.u32()?;
                if let Some(ch) = self.chans.get_mut(&c) {
                    ch.window += u64::from(n);
                }
                return self.recv();
            }
            msg::CHANNEL_EOF => Incoming::Eof(r.u32()?),
            msg::CHANNEL_CLOSE => Incoming::Close(r.u32()?),
            msg::CHANNEL_SUCCESS => Incoming::Success(r.u32()?),
            msg::CHANNEL_FAILURE => Incoming::Failure(r.u32()?),
            msg::CHANNEL_REQUEST => {
                let c = r.u32()?;
                let name = r.str()?;
                r.bool()?;
                if name == "exit-status" {
                    Incoming::ExitStatus(c, r.u32()?)
                } else {
                    Incoming::Other(m.clone())
                }
            }
            msg::CHANNEL_OPEN => {
                r.str()?;
                let remote = r.u32()?;
                let window = u64::from(r.u32()?);
                let id = self.next;
                self.next += 1;
                self.chans.insert(
                    id,
                    Chan {
                        remote,
                        window,
                        consumed: 0,
                    },
                );
                let mut w = Writer::msg(msg::CHANNEL_OPEN_CONFIRMATION);
                w.u32(remote).u32(id).u32(WINDOW).u32(MAX_PACKET);
                self.send(&w.finish())?;
                Incoming::Opened(id)
            }
            _ => Incoming::Other(m.clone()),
        })
    }

    fn consume(&mut self, chan: u32, n: usize) -> SshResult<()> {
        let Some(c) = self.chans.get_mut(&chan) else {
            return Ok(());
        };
        c.consumed += u32::try_from(n).unwrap_or(0);
        if c.consumed > WINDOW / 2 {
            let mut w = Writer::msg(msg::CHANNEL_WINDOW_ADJUST);
            w.u32(c.remote).u32(c.consumed);
            c.consumed = 0;
            self.send(&w.finish())?;
        }
        Ok(())
    }

    /// Collect a channel's output until it closes.
    pub fn collect(&mut self, chan: u32) -> SshResult<Output> {
        let mut out = Output::default();
        loop {
            match self.recv()? {
                Incoming::Data(c, d) if c == chan => out.stdout.extend(d),
                Incoming::Stderr(c, d) if c == chan => out.stderr.extend(d),
                Incoming::ExitStatus(c, n) if c == chan => out.status = Some(n),
                Incoming::Close(c) if c == chan => {
                    let _ = self.close(chan);
                    self.chans.remove(&chan);
                    return Ok(out);
                }
                _ => {}
            }
        }
    }

    /// Run a command with `input` on its standard input.
    pub fn run(&mut self, cmd: &str, input: &[u8]) -> SshResult<Output> {
        let chan = self.session()?;
        if !self.exec(chan, cmd)? {
            return Err(proto("exec refused"));
        }
        self.send_data(chan, input)?;
        self.send_eof(chan)?;
        self.collect(chan)
    }

    /// Send `SSH_MSG_DISCONNECT`.
    pub fn disconnect(&mut self) -> SshResult<()> {
        let mut w = Writer::msg(msg::DISCONNECT);
        w.u32(msg::reason::BY_APPLICATION).str("bye").str("");
        self.send(&w.finish())
    }
}
