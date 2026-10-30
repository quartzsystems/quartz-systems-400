// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SMTP client (RFC 5321) that hands a message to the smart host:
//! greeting, `EHLO` (`HELO` when refused), `STARTTLS` (RFC 3207) when
//! the relay offers it or must, `AUTH PLAIN` or `AUTH LOGIN` (RFC 4954)
//! when credentials are configured, `MAIL FROM`, one `RCPT TO` per
//! recipient, `DATA` with dot-stuffing, `QUIT`. Recipients the relay
//! refuses are reported; the message goes when one is accepted.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use thiserror::Error;

use crate::encode::base64;

/// The submission port.
pub const PORT: u16 = 25;
/// The implicit-TLS submission port (RFC 8314).
pub const TLS_PORT: u16 = 465;

/// How TLS is used with the relay.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TlsMode {
    /// Never.
    None,
    /// `STARTTLS` when the relay offers it.
    #[default]
    Opportunistic,
    /// `STARTTLS`, and fail when the relay does not offer it.
    Required,
    /// TLS from the first byte.
    Implicit,
}

/// The smart host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Relay {
    /// Its name or address.
    pub host: String,
    /// Its port.
    pub port: u16,
    /// Its addresses when the caller resolved them; empty to resolve
    /// `host` here.
    pub addrs: Vec<SocketAddr>,
    /// TLS.
    pub tls: TlsMode,
    /// User and password for `AUTH`.
    pub auth: Option<(String, String)>,
    /// The name this system gives in `EHLO`.
    pub helo: String,
    /// How long a connect or a reply may take.
    pub timeout: Duration,
}

impl Relay {
    /// `host` on port 25, opportunistic TLS, no `AUTH`.
    #[must_use]
    pub fn new(host: &str, helo: &str) -> Self {
        Self {
            host: host.to_string(),
            port: PORT,
            addrs: Vec::new(),
            tls: TlsMode::Opportunistic,
            auth: None,
            helo: helo.to_string(),
            timeout: Duration::from_secs(60),
        }
    }
}

/// Who the message is from and to, as `MAIL FROM` and `RCPT TO` say.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Envelope {
    /// The sender's mailbox.
    pub from: String,
    /// The recipients' mailboxes.
    pub to: Vec<String>,
}

/// What the relay did with a message.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Receipt {
    /// Recipients accepted.
    pub accepted: Vec<String>,
    /// Recipients refused: mailbox, code, text.
    pub refused: Vec<(String, u16, String)>,
    /// The relay's answer to the message.
    pub reply: String,
    /// Whether the session ran under TLS.
    pub secure: bool,
}

/// Why a message was not handed over.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SmtpError {
    /// No connection to the relay.
    #[error("cannot connect to {0}")]
    Connect(String),
    /// The connection failed.
    #[error("{0}")]
    Io(String),
    /// TLS could not start.
    #[error("TLS with the mail router failed: {0}")]
    Tls(String),
    /// The relay answered a step with an error.
    #[error("{stage}: {code} {text}")]
    Reply {
        /// The step.
        stage: String,
        /// Its reply code.
        code: u16,
        /// Its text.
        text: String,
    },
    /// Every recipient was refused.
    #[error("every recipient was refused")]
    NoRecipients(Vec<(String, u16, String)>),
}

/// A connection, plain or TLS.
pub trait Stream: Read + Write + Send {}

impl<T: Read + Write + Send> Stream for T {}

/// Starts TLS on a connection.
pub trait Connector {
    /// TLS to `host` over `tcp`.
    fn connect(&self, host: &str, tcp: TcpStream) -> io::Result<Box<dyn Stream>>;

    /// Whether TLS can be had at all (`STARTTLS` is tried only then
    /// unless the relay requires it).
    fn available(&self) -> bool {
        true
    }
}

/// TLS through `net/tls` with a client configuration.
pub struct RustlsConnector(pub Arc<tls::rustls::ClientConfig>);

impl Connector for RustlsConnector {
    fn connect(&self, host: &str, tcp: TcpStream) -> io::Result<Box<dyn Stream>> {
        let name = tls::config::server_name(host).map_err(io::Error::other)?;
        let s = tls::stream::TlsStream::connect(self.0.clone(), name, tcp).map_err(io::Error::other)?;
        Ok(Box::new(s))
    }
}

/// No TLS available: `STARTTLS` is not tried, and a relay that needs
/// TLS fails.
pub struct NoTls;

impl Connector for NoTls {
    fn connect(&self, host: &str, _: TcpStream) -> io::Result<Box<dyn Stream>> {
        Err(io::Error::other(format!("no TLS client for {host}")))
    }

    fn available(&self) -> bool {
        false
    }
}

/// One reply: its code and lines of text.
fn read_reply(s: &mut dyn Stream) -> io::Result<(u16, Vec<String>)> {
    let mut lines = Vec::new();
    loop {
        let mut line = Vec::new();
        let mut b = [0u8; 1];
        loop {
            if s.read(&mut b)? == 0 {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "the mail router closed the connection"));
            }
            if b[0] == b'\n' {
                break;
            }
            if line.len() < 4096 {
                line.push(b[0]);
            }
        }
        let text = String::from_utf8_lossy(&line).trim_end_matches('\r').to_string();
        let code: u16 = text.get(..3).and_then(|c| c.parse().ok()).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, format!("not an SMTP reply: {text}"))
        })?;
        let more = text.as_bytes().get(3) == Some(&b'-');
        lines.push(text.get(4..).unwrap_or("").to_string());
        if !more {
            return Ok((code, lines));
        }
    }
}

/// The connection: plain TCP until TLS starts.
enum Conn {
    Plain(TcpStream),
    Secure(Box<dyn Stream>),
    /// Between the two, while the socket is with TLS.
    Gone,
}

struct Session {
    conn: Conn,
}

impl Session {
    fn io(e: &io::Error) -> SmtpError {
        SmtpError::Io(e.to_string())
    }

    fn stream(&mut self) -> Result<&mut dyn Stream, SmtpError> {
        match &mut self.conn {
            Conn::Plain(t) => Ok(t),
            Conn::Secure(s) => Ok(s.as_mut()),
            Conn::Gone => Err(SmtpError::Io("the connection is closed".into())),
        }
    }

    fn secure(&self) -> bool {
        matches!(self.conn, Conn::Secure(_))
    }

    fn write(&mut self, b: &[u8]) -> Result<(), SmtpError> {
        let s = self.stream()?;
        s.write_all(b).and_then(|()| s.flush()).map_err(|e| Self::io(&e))
    }

    fn reply(&mut self) -> Result<(u16, Vec<String>), SmtpError> {
        read_reply(self.stream()?).map_err(|e| Self::io(&e))
    }

    /// Start TLS on the plain connection.
    fn upgrade(&mut self, host: &str, tls: &dyn Connector) -> Result<(), SmtpError> {
        let Conn::Plain(tcp) = std::mem::replace(&mut self.conn, Conn::Gone) else {
            return Err(SmtpError::Tls("TLS is already on".into()));
        };
        let s = tls.connect(host, tcp).map_err(|e| SmtpError::Tls(e.to_string()))?;
        self.conn = Conn::Secure(s);
        Ok(())
    }

    /// Send a line, read the answer; an error unless its code starts
    /// with `ok`.
    fn step(&mut self, line: &str, stage: &str, ok: u16) -> Result<(u16, Vec<String>), SmtpError> {
        self.write(format!("{line}\r\n").as_bytes())?;
        let (code, text) = self.reply()?;
        if code / 100 != ok {
            return Err(SmtpError::Reply {
                stage: stage.into(),
                code,
                text: text.join(" "),
            });
        }
        Ok((code, text))
    }

    /// `EHLO`, then `HELO` when refused: the extensions offered.
    fn hello(&mut self, name: &str) -> Result<Vec<String>, SmtpError> {
        match self.step(&format!("EHLO {name}"), "EHLO", 2) {
            Ok((_, lines)) => Ok(lines.into_iter().skip(1).map(|l| l.to_ascii_uppercase()).collect()),
            Err(SmtpError::Reply { code, .. }) if code / 100 == 5 => {
                self.step(&format!("HELO {name}"), "HELO", 2)?;
                Ok(Vec::new())
            }
            Err(e) => Err(e),
        }
    }
}

fn offers(ext: &[String], word: &str) -> bool {
    ext.iter().any(|l| l.split_whitespace().next() == Some(word))
}

/// The `AUTH` mechanisms offered.
fn mechanisms(ext: &[String]) -> Vec<String> {
    ext.iter()
        .filter_map(|l| l.strip_prefix("AUTH").map(str::to_string))
        .flat_map(|rest| rest.split([' ', '=']).filter(|w| !w.is_empty()).map(str::to_string).collect::<Vec<_>>())
        .collect()
}

/// The message as `DATA` sends it: CRLF line ends, lines that start
/// with a period doubled, the closing period line.
#[must_use]
pub fn dot_stuff(message: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(message.len() + 64);
    let body = message.strip_suffix(b"\n").unwrap_or(message);
    for line in body.split(|b| *b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.first() == Some(&b'.') {
            out.push(b'.');
        }
        out.extend_from_slice(line);
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(b".\r\n");
    out
}

fn open(relay: &Relay) -> Result<TcpStream, SmtpError> {
    let addrs: Vec<SocketAddr> = if relay.addrs.is_empty() {
        (relay.host.as_str(), relay.port)
            .to_socket_addrs()
            .map_err(|e| SmtpError::Connect(format!("{}: {e}", relay.host)))?
            .collect()
    } else {
        relay.addrs.iter().map(|a| SocketAddr::new(a.ip(), relay.port)).collect()
    };
    let mut last = String::from("no address");
    for a in addrs {
        match TcpStream::connect_timeout(&a, relay.timeout) {
            Ok(t) => {
                let _ = t.set_read_timeout(Some(relay.timeout));
                let _ = t.set_write_timeout(Some(relay.timeout));
                return Ok(t);
            }
            Err(e) => last = e.to_string(),
        }
    }
    Err(SmtpError::Connect(format!("{}:{}: {last}", relay.host, relay.port)))
}

/// Hand `message` to the relay for the envelope's recipients.
pub fn send(relay: &Relay, env: &Envelope, message: &[u8], tls: &dyn Connector) -> Result<Receipt, SmtpError> {
    let tcp = open(relay)?;
    let mut sess = Session { conn: Conn::Plain(tcp) };
    if relay.tls == TlsMode::Implicit {
        sess.upgrade(&relay.host, tls)?;
    }
    let (code, text) = sess.reply()?;
    if code / 100 != 2 {
        return Err(SmtpError::Reply {
            stage: "greeting".into(),
            code,
            text: text.join(" "),
        });
    }
    let mut ext = sess.hello(&relay.helo)?;
    if !sess.secure() && relay.tls != TlsMode::None {
        if offers(&ext, "STARTTLS") && (tls.available() || relay.tls == TlsMode::Required) {
            sess.step("STARTTLS", "STARTTLS", 2)?;
            sess.upgrade(&relay.host, tls)?;
            ext = sess.hello(&relay.helo)?;
        } else if relay.tls == TlsMode::Required {
            return Err(SmtpError::Tls("the mail router does not offer STARTTLS".into()));
        }
    }
    if let Some((user, pass)) = &relay.auth {
        let mech = mechanisms(&ext);
        if mech.iter().any(|m| m == "PLAIN") {
            let token = base64(format!("\0{user}\0{pass}").as_bytes());
            sess.step(&format!("AUTH PLAIN {token}"), "AUTH", 2)?;
        } else if mech.iter().any(|m| m == "LOGIN") {
            sess.step("AUTH LOGIN", "AUTH", 3)?;
            sess.step(&base64(user.as_bytes()), "AUTH", 3)?;
            sess.step(&base64(pass.as_bytes()), "AUTH", 2)?;
        } else {
            return Err(SmtpError::Reply {
                stage: "AUTH".into(),
                code: 504,
                text: "the mail router offers neither AUTH PLAIN nor AUTH LOGIN".into(),
            });
        }
    }
    sess.step(&format!("MAIL FROM:<{}>", env.from), "MAIL FROM", 2)?;
    let mut receipt = Receipt {
        secure: sess.secure(),
        ..Receipt::default()
    };
    for r in &env.to {
        match sess.step(&format!("RCPT TO:<{r}>"), "RCPT TO", 2) {
            Ok(_) => receipt.accepted.push(r.clone()),
            Err(SmtpError::Reply { code, text, .. }) => receipt.refused.push((r.clone(), code, text)),
            Err(e) => return Err(e),
        }
    }
    if receipt.accepted.is_empty() {
        let _ = sess.step("QUIT", "QUIT", 2);
        return Err(SmtpError::NoRecipients(receipt.refused));
    }
    sess.step("DATA", "DATA", 3)?;
    sess.write(&dot_stuff(message))?;
    let (code, text) = sess.reply()?;
    if code / 100 != 2 {
        return Err(SmtpError::Reply {
            stage: "DATA".into(),
            code,
            text: text.join(" "),
        });
    }
    receipt.reply = format!("{code} {}", text.join(" "));
    let _ = sess.step("QUIT", "QUIT", 2);
    Ok(receipt)
}
