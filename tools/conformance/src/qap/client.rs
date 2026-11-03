// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A QAP client over TLS for steps 89 and 90, from `qap-proto`'s types.
//! The server's certificate is pinned: the first connection takes it
//! (and checks its name), every later one must present the same.

use std::collections::VecDeque;
use std::io::{ErrorKind, Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use ipc::Wire;
use qap_proto::ops::connection::{Hello, HelloReply, JobInfoReply};
use qap_proto::ops::signon::{Attach, SignOn, SignOnReply};
use qap_proto::{Envelope, Kind, Op, Reassembly, Status};
use qapd::frames::FrameReader;

type Tls = tls::stream::TlsStream<TcpStream>;

/// How long a reply may take unless a call says otherwise.
pub const WAIT: Duration = Duration::from_secs(60);

/// One connection.
pub struct Conn {
    tls: Tls,
    frames: FrameReader,
    asm: Reassembly,
    next_id: u32,
    /// Replies and events read while waiting for something else.
    held: VecDeque<Envelope>,
    closed: bool,
}

/// Connect to host port `port`; `pin` is the certificate to expect,
/// or empty to take the one the server presents (whose common name
/// must be `cn`). The certificate presented.
pub fn connect(port: u16, cn: &str, pin: &[u8]) -> Result<(Conn, Vec<u8>), String> {
    connect_for(port, cn, pin, Duration::from_secs(60))
}

/// [`connect`], trying for at most `wait` (zero: once).
pub fn connect_for(port: u16, cn: &str, pin: &[u8], wait: Duration) -> Result<(Conn, Vec<u8>), String> {
    let policy = tls::policy::TlsPolicy::opsys();
    let cfg = tls::config::client_config(
        &policy,
        tls::rustls::RootCertStore::empty(),
        None,
        tls::config::ServerAuthMode::Passthru,
    )
    .map_err(|e| e.to_string())?;
    let name = tls::config::server_name(cn).map_err(|e| e.to_string())?;
    let end = Instant::now() + wait;
    let tls = loop {
        let r = TcpStream::connect(("127.0.0.1", port))
            .map_err(|e| e.to_string())
            .and_then(|c| {
                let _ = c.set_nodelay(true);
                let _ = c.set_read_timeout(Some(Duration::from_secs(30)));
                tls::stream::TlsStream::connect(cfg.clone(), name.clone(), c).map_err(|e| e.to_string())
            });
        match r {
            Ok(t) => break t,
            Err(e) if Instant::now() > end => return Err(format!("QAP over TLS on host port {port}: {e}")),
            Err(_) => std::thread::sleep(Duration::from_millis(500)),
        }
    };
    let der = tls
        .session()
        .and_then(|s| s.peer_certs.into_iter().next())
        .ok_or("the server presented no certificate")?;
    if pin.is_empty() {
        let info = certs::x509::CertInfo::from_der(&der).map_err(|e| e.to_string())?;
        if info.common_name.as_deref() != Some(cn) {
            return Err(format!("QAP presented {} rather than CN={cn}", info.subject));
        }
    } else if der != pin {
        return Err("QAP presented another certificate than the first connection's".into());
    }
    let _ = tls.get_ref().set_read_timeout(Some(Duration::from_millis(200)));
    Ok((
        Conn {
            tls,
            frames: FrameReader::new(),
            asm: Reassembly::new(),
            next_id: 0,
            held: VecDeque::new(),
            closed: false,
        },
        der,
    ))
}

impl Conn {
    fn send(&mut self, env: &Envelope) -> Result<(), String> {
        let mut b = Vec::new();
        env.write_to(&mut b).map_err(|e| e.to_string())?;
        self.tls.write_all(&b).map_err(|e| format!("sending: {e}"))?;
        self.tls.flush().map_err(|e| format!("sending: {e}"))
    }

    /// Send a request for `op`; its ID.
    pub fn request<B: Wire>(&mut self, op: Op, body: &B) -> Result<u32, String> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&Envelope::request(id, op, body))?;
        Ok(id)
    }

    /// Cancel request `id`.
    pub fn cancel(&mut self, id: u32) -> Result<(), String> {
        self.send(&Envelope::cancel(id))
    }

    /// The next whole envelope within `wait`: `Ok(None)` at the
    /// deadline, `Err` once the server closed.
    fn next(&mut self, wait: Duration) -> Result<Option<Envelope>, String> {
        let end = Instant::now() + wait;
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            while let Some(env) = self.frames.next_frame().map_err(|e| format!("a bad frame: {e}"))? {
                if let Some(whole) = self.asm.push(env) {
                    return Ok(Some(whole));
                }
            }
            if self.closed {
                return Err("the server closed the connection".into());
            }
            if Instant::now() >= end {
                return Ok(None);
            }
            match self.tls.read(&mut buf) {
                Ok(0) => self.closed = true,
                Ok(n) => self.frames.push(&buf[..n]),
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(_) => self.closed = true,
            }
        }
    }

    /// The reply to request `id` within `wait`, or `None`; whatever else
    /// comes meanwhile is kept.
    pub fn poll_reply(&mut self, id: u32, wait: Duration) -> Result<Option<Envelope>, String> {
        if let Some(i) = self.held.iter().position(|e| e.kind == Kind::Reply && e.request_id == id) {
            return Ok(self.held.remove(i));
        }
        let end = Instant::now() + wait;
        loop {
            let left = end.saturating_duration_since(Instant::now());
            match self.next(left)? {
                Some(e) if e.kind == Kind::Reply && e.request_id == id => return Ok(Some(e)),
                Some(e) => self.held.push_back(e),
                None => return Ok(None),
            }
        }
    }

    /// The reply to request `id`.
    pub fn reply(&mut self, id: u32, wait: Duration) -> Result<Envelope, String> {
        self.poll_reply(id, wait)?
            .ok_or_else(|| format!("no reply to request {id} in {}s", wait.as_secs()))
    }

    /// Send and take the reply.
    pub fn call<B: Wire>(&mut self, op: Op, body: &B) -> Result<Envelope, String> {
        let id = self.request(op, body)?;
        self.reply(id, WAIT).map_err(|e| format!("{op:?}: {e}"))
    }

    /// Send, take the reply, which must be `Ok`, and decode its body.
    pub fn ok<B: Wire, R: Wire>(&mut self, op: Op, body: &B) -> Result<R, String> {
        let r = self.call(op, body)?;
        if r.status != Status::Ok {
            return Err(format!("{op:?}: {}", brief(&r)));
        }
        r.body_as().map_err(|e| format!("{op:?}'s body: {e}"))
    }

    /// The next event within `wait`.
    pub fn event(&mut self, wait: Duration) -> Result<Envelope, String> {
        if let Some(i) = self.held.iter().position(|e| e.kind == Kind::Event) {
            return Ok(self.held.remove(i).expect("held"));
        }
        let end = Instant::now() + wait;
        loop {
            let left = end.saturating_duration_since(Instant::now());
            match self.next(left)? {
                Some(e) if e.kind == Kind::Event => return Ok(e),
                Some(e) => self.held.push_back(e),
                None => return Err(format!("no event in {}s", wait.as_secs())),
            }
        }
    }

    /// Whether the server closes the connection within `wait`.
    pub fn closes(&mut self, wait: Duration) -> bool {
        let end = Instant::now() + wait;
        while Instant::now() < end {
            match self.next(Duration::from_millis(500)) {
                Err(_) => return true,
                Ok(Some(e)) => self.held.push_back(e),
                Ok(None) => {}
            }
        }
        false
    }

    /// `Hello` for versions 1 to 4, request ID 1.
    pub fn hello(&mut self) -> Result<HelloReply, String> {
        self.next_id = 0;
        self.ok(
            Op::Hello,
            &Hello {
                min_version: 1,
                max_version: qap_proto::VERSION,
                client_name: "conformance".into(),
                client_version: "89".into(),
                capabilities: Vec::new(),
            },
        )
    }

    /// Sign on as `user`.
    pub fn sign_on(&mut self, user: &str, password: &str) -> Result<SignOnReply, String> {
        self.ok(
            Op::SignOn,
            &SignOn {
                user: user.into(),
                password: qap_proto::Secret::from(password),
            },
        )
    }

    /// Attach to the session `token` names.
    pub fn attach(&mut self, token: &qap_proto::Token) -> Result<SignOnReply, String> {
        self.ok(Op::Attach, &Attach { token: token.clone() })
    }

    /// The connection's job.
    pub fn job_info(&mut self) -> Result<JobInfoReply, String> {
        self.ok(Op::JobInfo, &())
    }
}

/// An envelope in a line: status, message ID and text.
pub fn brief(e: &Envelope) -> String {
    match &e.message {
        Some(m) => format!("{:?} {} {}", e.status, m.id, m.text),
        None => format!("{:?}", e.status),
    }
}

/// The message ID an envelope carries, or empty.
pub fn msgid(e: &Envelope) -> &str {
    e.message.as_ref().map_or("", |m| m.id.as_str())
}
