// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the product needs from the system, as traits: the running
//! system answers them in the `ADMIN` job (`work/qsjob`), the fixture
//! ([`crate::fixture`]) in tests and the demo server.

use std::sync::Arc;

use httpd::files::Files;
use qsweb::msg::{Message, SecondLevel};

/// A special authority or other right an action needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    /// `*IOSYSCFG`: instances, ports, TLS.
    IoSysCfg,
    /// `*JOBCTL`: starting and ending server jobs.
    JobCtl,
    /// `*SECADM`.
    SecAdm,
    /// `*ALLOBJ`: certificate stores and their passwords.
    AllObj,
}

impl Need {
    /// The special authority's name.
    pub fn name(self) -> &'static str {
        match self {
            Need::IoSysCfg => "*IOSYSCFG",
            Need::JobCtl => "*JOBCTL",
            Need::SecAdm => "*SECADM",
            Need::AllObj => "*ALLOBJ",
        }
    }
}

/// An audit journal entry the product writes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Audit {
    /// `PW`: a refused sign-on (user, the reason code).
    Password {
        /// The user named.
        user: String,
        /// `P` wrong password, `U` no such user, `D` disabled, ...
        code: char,
    },
    /// `AF`: a refused action.
    Refused {
        /// The user.
        user: String,
        /// The object or function.
        object: String,
        /// What was missing.
        need: String,
    },
    /// `ZC`: a change made through the product (Global settings' audit
    /// flag on).
    Change {
        /// The user.
        user: String,
        /// The object changed.
        object: String,
        /// What changed.
        what: String,
    },
}

/// Why a sign-on was refused: the message for the sign-on page.
pub type SignOnRefused = Message;

/// The system: its name and clock, users, authority, audit, messages.
pub trait System: Send + Sync {
    /// The system name (`QSHQ400`).
    fn name(&self) -> String;
    /// Seconds since 1970.
    fn now(&self) -> u64;
    /// `secs` as the date and time the user's job shows
    /// (`10/29/26`, `14:02:17`).
    fn stamp(&self, secs: u64) -> (String, String);
    /// Check a user and password with the 5250 sign-on rules
    /// (`QMAXSIGN`, disabled profiles, expired passwords). `from` is
    /// the client's address. Gives the profile name, upper case.
    fn sign_on(&self, user: &str, password: &str, from: &str) -> Result<String, SignOnRefused>;
    /// The user's special authorities (`*ALLOBJ`, `*IOSYSCFG`, ...).
    fn special(&self, user: &str) -> Vec<String>;
    /// The user's group profiles.
    fn groups(&self, user: &str) -> Vec<String>;
    /// The user's class (`*SECOFR`, `*USER`, ...).
    fn user_class(&self, user: &str) -> String;
    /// Whether a user profile exists.
    fn profile_exists(&self, user: &str) -> bool;
    /// Write an audit journal entry.
    fn audit(&self, entry: Audit);
    /// A message's text from its message file, `data` substituted.
    fn message(&self, id: &str, data: &[&str]) -> Option<SecondLevel>;
    /// This job, qualified (`123457/QTMHHTTP/ADMIN`).
    fn job(&self) -> String;
    /// The addresses of this system's TCP/IP interfaces.
    fn interfaces(&self) -> Vec<String>;
}

/// How an instance ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndHow {
    /// `OPTION(*CNTRLD) DELAY(n)`: requests in progress get `n` seconds.
    Controlled(u32),
    /// `OPTION(*IMMED)`.
    Immediate,
}

/// An instance's active job in `QHTTPSVR`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpJob {
    /// The instance (the job name).
    pub instance: String,
    /// The job number, six digits.
    pub number: String,
    /// The job's user (`QTMHHTTP`).
    pub user: String,
    /// Seconds since 1970 when it started.
    pub started: u64,
    /// Client connections open now.
    pub connections: u32,
}

impl HttpJob {
    /// `number/user/name`.
    pub fn qualified(&self) -> String {
        format!("{}/{}/{}", self.number, self.user, self.instance)
    }
}

/// The instances' jobs: which run, and starting and ending them.
pub trait Jobs: Send + Sync {
    /// Every active job of subsystem `QHTTPSVR` that runs an instance.
    fn active(&self) -> Vec<HttpJob>;
    /// `STRTCPSVR SERVER(*HTTP) HTTPSVR(instance)` as `user`. Gives the
    /// completion message.
    fn start(&self, user: &str, instance: &str) -> Result<Message, Message>;
    /// `ENDTCPSVR SERVER(*HTTP) HTTPSVR(instance)` as `user`.
    fn end(&self, user: &str, instance: &str, how: EndHow) -> Result<Message, Message>;
}

/// The certificate manager's side (steps 12 to 15 shape it).
pub trait Certs: Send + Sync {
    /// The `*SYSTEM` server certificates application `app` may present,
    /// the one assigned to it first: label and expiry (seconds since
    /// 1970). Read-only; the TLS page of an instance shows them.
    fn for_app(&self, _app: &str) -> Vec<(String, u64)> {
        Vec::new()
    }
}

/// Everything the product runs over.
#[derive(Clone)]
pub struct Env {
    /// The system.
    pub system: Arc<dyn System>,
    /// The instances' jobs.
    pub jobs: Arc<dyn Jobs>,
    /// The IFS, as the administration job sees it.
    pub files: Arc<dyn Files>,
    /// Certificates.
    pub certs: Arc<dyn Certs>,
}
