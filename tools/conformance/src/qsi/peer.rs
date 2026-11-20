// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A management console as the QSI steps drive it: its own identity and
//! pins, on `qsisess`'s client side over `qsitrust`'s TLS
//! (`qsisvr::client`). The console steps use it too.

use std::time::{Duration, Instant};

use qsi::any::{AnyReply, AnyRequest};
use qsi::base;
use qsisess::Reply;
use qsisvr::client::{Client, ConsolePins, hello};
use qsitrust::{Fingerprint, Identity};

/// How long a request may take.
pub(crate) const WAIT: Duration = Duration::from_secs(20);

/// One console.
pub(crate) struct Peer {
    /// Its name, as it pairs.
    pub name: String,
    id: Identity,
    /// What it pins.
    pub pins: ConsolePins,
}

impl Peer {
    /// A console named `name` with an identity of its own.
    pub(crate) fn new(name: &str) -> Result<Self, String> {
        let id = Identity::make(name, "", 1_794_488_700).map_err(|e| e.to_string())?;
        Ok(Self {
            name: name.into(),
            id,
            pins: ConsolePins::default(),
        })
    }

    /// Its certificate's fingerprint.
    pub(crate) fn fingerprint(&self) -> Fingerprint {
        self.id.fingerprint()
    }

    /// A session with the system on `port` (host side).
    pub(crate) fn connect(&self, port: u16) -> Result<Client, String> {
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        let mut c = Client::connect(addr, &self.id, &self.pins, hello(&self.name), WAIT)?;
        c.user = "QSMCOPER".into();
        Ok(c)
    }

    /// A session, tried again until `within` passes: the server may be
    /// starting.
    pub(crate) fn connect_within(&self, port: u16, within: Duration) -> Result<Client, String> {
        let until = Instant::now() + within;
        loop {
            match self.connect(port) {
                Ok(c) => return Ok(c),
                Err(e) if Instant::now() >= until => return Err(e),
                Err(_) => std::thread::sleep(Duration::from_secs(1)),
            }
        }
    }

    /// Pair with `password` on a new session, pin the system, close.
    /// The system's fingerprint.
    pub(crate) fn pair(&self, port: u16, password: &str) -> Result<Fingerprint, String> {
        self.pins.pairing(true);
        let mut c = self.connect(port)?;
        let r = c.pair(password, &self.name, WAIT);
        self.pins.pairing(false);
        let (fp, _) = r?;
        self.pins.pin(fp);
        c.close();
        Ok(fp)
    }
}

/// Ask and decode the reply; a refusal is its identifier and text.
pub(crate) fn ask(c: &mut Client, req: &AnyRequest) -> Result<Result<AnyReply, String>, String> {
    let r: Reply = c.ask(req, WAIT)?;
    if let Some((id, text)) = r.refusal() {
        return Ok(Err(format!("{id} {text}")));
    }
    match r.decode() {
        Ok(Some(a)) => Ok(Ok(a)),
        other => Err(format!("a reply that does not read: {other:?}")),
    }
}

/// A base request answered, or why not.
pub(crate) fn base(c: &mut Client, req: base::Request) -> Result<base::Reply, String> {
    match ask(c, &AnyRequest::Base(req))? {
        Ok(AnyReply::Base(b)) => Ok(b),
        Ok(other) => Err(format!("not a base reply: {other:?}")),
        Err(e) => Err(e),
    }
}

/// The refusal's identifier of `req`, or why it was answered.
pub(crate) fn refused(c: &mut Client, req: &AnyRequest) -> Result<String, String> {
    match ask(c, req)? {
        Err(e) => Ok(e.split_whitespace().next().unwrap_or("").to_string()),
        Ok(r) => Err(format!("answered, not refused: {r:?}")),
    }
}
