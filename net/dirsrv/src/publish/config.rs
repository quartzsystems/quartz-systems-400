// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The publishing configuration, `publish.conf` (docs/DIRSRV.md §10.1):
//! QS/400's own LDIF, one record per agent, read and written with
//! [`ldap::ldif`]. The bind password is kept sealed ([`seal_password`]).

use certs::stash::Sealer;
use ldap::dn::Dn;
use ldap::ldif::{self, AttrValue, Body, Ldif, Record, Value};

/// Where the file lives.
pub const PATH: &str = "/QIBM/UserData/OS400/DirSrv/publish.conf";

/// The format version (`qs400-pubFormat`), frozen at V1R2M0.
pub const FORMAT_VERSION: u32 = 1;

/// The DN of the file's header record.
const HEADER_DN: &str = "cn=publishing";

/// The attribute names, as written.
mod attr {
    pub const FORMAT: &str = "qs400-pubFormat";
    pub const ENABLED: &str = "qs400-pubEnabled";
    pub const HOST: &str = "qs400-pubHost";
    pub const PORT: &str = "qs400-pubPort";
    pub const SECURITY: &str = "qs400-pubSecurity";
    pub const BIND_DN: &str = "qs400-pubBindDN";
    pub const BIND_PW: &str = "qs400-pubBindPW";
    pub const PARENT_DN: &str = "qs400-pubParentDN";
    pub const CREATE_PARENT: &str = "qs400-pubCreateParent";
    pub const RETRY: &str = "qs400-pubRetry";
    pub const RESYNC: &str = "qs400-pubResync";
    pub const PUBLISHER: &str = "qs400-pubPublisher";
}

/// The shortest and longest first retry interval, seconds.
pub const RETRY_RANGE: (u32, u32) = (10, 86_400);
/// The shortest resync interval other than none, seconds.
pub const RESYNC_MIN: u32 = 60;
/// The longest publisher value.
pub const PUBLISHER_MAX: usize = 128;

/// What an agent publishes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    /// User profiles (`QGLDPUBE`).
    Users,
    /// Printer devices (`QGLDPUBA`).
    Printers,
}

impl Kind {
    /// Both, in file order.
    pub const ALL: [Self; 2] = [Self::Users, Self::Printers];

    /// The agent's name (`cn=NAME,cn=publishing`).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Users => "users",
            Self::Printers => "printers",
        }
    }

    /// The job that runs the agent.
    #[must_use]
    pub const fn job(self) -> &'static str {
        match self {
            Self::Users => "QGLDPUBE",
            Self::Printers => "QGLDPUBA",
        }
    }

    /// The agent of `name` (case ignored).
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|k| k.name().eq_ignore_ascii_case(name.trim()))
    }

    /// The agent a job runs, by the job's name.
    #[must_use]
    pub fn from_job(job: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|k| k.job().eq_ignore_ascii_case(job.trim()))
    }

    /// The attribute naming an entry's object (profile or device).
    #[must_use]
    pub const fn key_attr(self) -> &'static str {
        match self {
            Self::Users => "uid",
            Self::Printers => "printer-name",
        }
    }

    /// The parent DN when the file names none.
    #[must_use]
    pub const fn default_parent(self) -> &'static str {
        match self {
            Self::Users => "ou=Users,cn=localhost",
            Self::Printers => "ou=Printers,cn=localhost",
        }
    }

    fn dn(self) -> String {
        format!("cn={},{HEADER_DN}", self.name())
    }
}

/// How the connection to the target is protected.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Security {
    /// In the clear.
    #[default]
    None,
    /// TLS from the start (LDAPS).
    Ssl,
    /// `StartTLS` after connecting.
    StartTls,
}

impl Security {
    /// The value as written.
    #[must_use]
    pub const fn text(self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::Ssl => "SSL",
            Self::StartTls => "STARTTLS",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        [Self::None, Self::Ssl, Self::StartTls]
            .into_iter()
            .find(|v| v.text().eq_ignore_ascii_case(s.trim()))
    }

    /// The port used when the file names none.
    #[must_use]
    pub const fn default_port(self) -> u16 {
        match self {
            Self::Ssl => 636,
            _ => 389,
        }
    }
}

/// One agent's settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentConfig {
    /// What it publishes.
    pub kind: Kind,
    /// Whether it publishes.
    pub enabled: bool,
    /// The target's host name or address.
    pub host: String,
    /// The target's port.
    pub port: u16,
    /// TLS or not.
    pub security: Security,
    /// The DN bound as; none binds anonymously.
    pub bind_dn: Option<String>,
    /// The password, sealed ([`seal_password`]).
    pub bind_pw: Option<Vec<u8>>,
    /// Where the entries go; none is [`Kind::default_parent`].
    pub parent_dn: Option<String>,
    /// Whether the parent and its missing ancestors are added.
    pub create_parent: bool,
    /// Seconds before the first retry.
    pub retry: u32,
    /// Seconds between full resyncs, 0 for none.
    pub resync: u32,
    /// The `publisherName` value; none is `QS400 ` and the system name.
    pub publisher: Option<String>,
}

impl AgentConfig {
    /// The shipped settings: disabled, the local instance in the clear.
    #[must_use]
    pub fn new(kind: Kind) -> Self {
        Self {
            kind,
            enabled: false,
            host: "localhost".into(),
            port: Security::None.default_port(),
            security: Security::None,
            bind_dn: None,
            bind_pw: None,
            parent_dn: None,
            create_parent: true,
            retry: 300,
            resync: 86_400,
            publisher: None,
        }
    }

    /// The parent DN in effect.
    #[must_use]
    pub fn parent(&self) -> &str {
        self.parent_dn
            .as_deref()
            .unwrap_or_else(|| self.kind.default_parent())
    }

    /// The `publisherName` value in effect on system `system`.
    #[must_use]
    pub fn publisher_name(&self, system: &str) -> String {
        match &self.publisher {
            Some(p) => p.clone(),
            None => default_publisher(system),
        }
    }
}

/// `QS400 ` and the system name: the publisher value by default.
#[must_use]
pub fn default_publisher(system: &str) -> String {
    format!("QS400 {}", system.trim())
}

/// The whole file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublishConfig {
    /// Both agents, in [`Kind::ALL`]'s order.
    pub agents: [AgentConfig; 2],
}

impl Default for PublishConfig {
    fn default() -> Self {
        Self {
            agents: [
                AgentConfig::new(Kind::Users),
                AgentConfig::new(Kind::Printers),
            ],
        }
    }
}

impl PublishConfig {
    /// The settings of `kind`.
    #[must_use]
    pub fn agent(&self, kind: Kind) -> &AgentConfig {
        &self.agents[kind as usize]
    }

    /// The settings of `kind`, to change.
    pub fn agent_mut(&mut self, kind: Kind) -> &mut AgentConfig {
        &mut self.agents[kind as usize]
    }
}

/// Why a file or a password is refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    /// Not LDIF.
    #[error("{0}")]
    Ldif(#[from] ldif::LdifError),
    /// LDIF that is not a publishing configuration.
    #[error("publish.conf line {line}: {what}")]
    Bad {
        /// The record's line (1-based; 0 when unknown).
        line: usize,
        /// What is wrong.
        what: String,
    },
    /// The password could not be sealed or unsealed.
    #[error("publishing password: {0}")]
    Seal(String),
}

fn bad<T>(line: usize, what: impl Into<String>) -> Result<T, ConfigError> {
    Err(ConfigError::Bad {
        line,
        what: what.into(),
    })
}

/// Two DNs the same once parsed (ASCII case ignored).
fn same_dn(a: &str, b: &str) -> bool {
    match (Dn::parse(a), Dn::parse(b)) {
        (Ok(a), Ok(b)) => a.to_string().eq_ignore_ascii_case(&b.to_string()),
        _ => false,
    }
}

/// One record's values by attribute, each at most once.
struct Values<'a> {
    line: usize,
    vals: Vec<(&'static str, &'a [u8])>,
}

impl<'a> Values<'a> {
    fn of(r: &'a Record, allowed: &[&'static str]) -> Result<Self, ConfigError> {
        let Body::Content(lines) = &r.body else {
            return bad(r.line, "a change record is not a publishing setting");
        };
        let mut vals: Vec<(&'static str, &[u8])> = Vec::new();
        for l in lines {
            let Some(name) = allowed.iter().find(|a| a.eq_ignore_ascii_case(&l.ty)) else {
                return bad(
                    r.line,
                    format!("{} is not a publishing attribute here", l.ty),
                );
            };
            let Value::Data(d) = &l.value else {
                return bad(r.line, format!("{name} cannot be a URL"));
            };
            if vals.iter().any(|(n, _)| n == name) {
                return bad(r.line, format!("{name} is given more than once"));
            }
            vals.push((name, d));
        }
        Ok(Self { line: r.line, vals })
    }

    fn bytes(&self, name: &str) -> Option<&'a [u8]> {
        self.vals.iter().find(|(n, _)| *n == name).map(|(_, v)| *v)
    }

    fn text(&self, name: &str) -> Result<Option<&'a str>, ConfigError> {
        match self.bytes(name) {
            None => Ok(None),
            Some(b) => match std::str::from_utf8(b) {
                Ok(s) => Ok(Some(s.trim())),
                Err(_) => bad(self.line, format!("{name} is not UTF-8")),
            },
        }
    }

    fn number(&self, name: &str) -> Result<Option<u32>, ConfigError> {
        match self.text(name)? {
            None => Ok(None),
            Some(s) => match s.parse::<u32>() {
                Ok(n) => Ok(Some(n)),
                Err(_) => bad(self.line, format!("{name} {s} is not a number")),
            },
        }
    }

    fn flag(&self, name: &str) -> Result<Option<bool>, ConfigError> {
        match self.text(name)? {
            None => Ok(None),
            Some(s) if s.eq_ignore_ascii_case("TRUE") => Ok(Some(true)),
            Some(s) if s.eq_ignore_ascii_case("FALSE") => Ok(Some(false)),
            Some(s) => bad(self.line, format!("{name} {s} is neither TRUE nor FALSE")),
        }
    }

    fn dn(&self, name: &str) -> Result<Option<String>, ConfigError> {
        match self.text(name)? {
            None => Ok(None),
            Some(s) => match Dn::parse(s) {
                Ok(d) if !d.is_root() => Ok(Some(s.to_string())),
                _ => bad(self.line, format!("{name} {s} is not a DN")),
            },
        }
    }
}

const AGENT_ATTRS: &[&str] = &[
    attr::ENABLED,
    attr::HOST,
    attr::PORT,
    attr::SECURITY,
    attr::BIND_DN,
    attr::BIND_PW,
    attr::PARENT_DN,
    attr::CREATE_PARENT,
    attr::RETRY,
    attr::RESYNC,
    attr::PUBLISHER,
];

fn agent(kind: Kind, v: &Values<'_>) -> Result<AgentConfig, ConfigError> {
    let line = v.line;
    let mut a = AgentConfig::new(kind);
    if let Some(e) = v.flag(attr::ENABLED)? {
        a.enabled = e;
    }
    if let Some(h) = v.text(attr::HOST)? {
        if h.is_empty() || h.contains(char::is_whitespace) || h.len() > 255 {
            return bad(line, format!("{} {h} is not a host name", attr::HOST));
        }
        a.host = h.to_string();
    }
    if let Some(s) = v.text(attr::SECURITY)? {
        a.security = Security::parse(s).ok_or_else(|| ConfigError::Bad {
            line,
            what: format!("{} {s} is not NONE, SSL or STARTTLS", attr::SECURITY),
        })?;
    }
    a.port = match v.number(attr::PORT)? {
        None => a.security.default_port(),
        Some(p) => match u16::try_from(p) {
            Ok(p) if p > 0 => p,
            _ => return bad(line, format!("{} {p} is not 1 to 65535", attr::PORT)),
        },
    };
    a.bind_dn = v.dn(attr::BIND_DN)?;
    a.bind_pw = v.bytes(attr::BIND_PW).map(<[u8]>::to_vec);
    if a.bind_pw.is_some() && a.bind_dn.is_none() {
        return bad(line, format!("{} without {}", attr::BIND_PW, attr::BIND_DN));
    }
    a.parent_dn = v.dn(attr::PARENT_DN)?;
    if let Some(c) = v.flag(attr::CREATE_PARENT)? {
        a.create_parent = c;
    }
    if let Some(r) = v.number(attr::RETRY)? {
        if !(RETRY_RANGE.0..=RETRY_RANGE.1).contains(&r) {
            return bad(
                line,
                format!(
                    "{} {r} is not {} to {}",
                    attr::RETRY,
                    RETRY_RANGE.0,
                    RETRY_RANGE.1
                ),
            );
        }
        a.retry = r;
    }
    if let Some(r) = v.number(attr::RESYNC)? {
        if r != 0 && r < RESYNC_MIN {
            return bad(
                line,
                format!("{} {r} is neither 0 nor {RESYNC_MIN} or more", attr::RESYNC),
            );
        }
        a.resync = r;
    }
    if let Some(p) = v.text(attr::PUBLISHER)? {
        if p.is_empty() || p.len() > PUBLISHER_MAX {
            return bad(
                line,
                format!(
                    "{} is empty or longer than {PUBLISHER_MAX}",
                    attr::PUBLISHER
                ),
            );
        }
        a.publisher = Some(p.to_string());
    }
    Ok(a)
}

/// Reads a file.
///
/// # Errors
///
/// [`ConfigError`] for anything the format does not allow.
pub fn parse(src: &[u8]) -> Result<PublishConfig, ConfigError> {
    let l = ldif::parse(src)?;
    if l.version.is_some_and(|v| v != 1) {
        return bad(0, "only LDIF version 1 is read");
    }
    let mut records = l.records.iter();
    let Some(head) = records.next() else {
        return bad(0, format!("no {HEADER_DN} record"));
    };
    if !same_dn(&head.dn, HEADER_DN) {
        return bad(head.line, format!("the first record is not {HEADER_DN}"));
    }
    let h = Values::of(head, &[attr::FORMAT])?;
    match h.number(attr::FORMAT)? {
        Some(FORMAT_VERSION) => {}
        Some(n) => return bad(head.line, format!("format version {n} is not known")),
        None => return bad(head.line, format!("{} is missing", attr::FORMAT)),
    }
    let mut cfg = PublishConfig::default();
    let mut seen: Vec<Kind> = Vec::new();
    for r in records {
        let Some(kind) = Kind::ALL.into_iter().find(|k| same_dn(&r.dn, &k.dn())) else {
            return bad(r.line, format!("{} is not a publishing agent", r.dn));
        };
        if seen.contains(&kind) {
            return bad(r.line, format!("agent {} is given twice", kind.name()));
        }
        seen.push(kind);
        *cfg.agent_mut(kind) = agent(kind, &Values::of(r, AGENT_ATTRS)?)?;
    }
    Ok(cfg)
}

fn line(ty: &str, v: impl Into<Vec<u8>>) -> AttrValue {
    AttrValue {
        ty: ty.to_string(),
        value: Value::Data(v.into()),
    }
}

fn flag(b: bool) -> &'static str {
    if b { "TRUE" } else { "FALSE" }
}

/// Prints a file [`parse`] reads back the same.
#[must_use]
pub fn write(cfg: &PublishConfig) -> String {
    let mut records = vec![Record {
        line: 0,
        dn: HEADER_DN.into(),
        controls: Vec::new(),
        body: Body::Content(vec![line(attr::FORMAT, FORMAT_VERSION.to_string())]),
    }];
    for a in &cfg.agents {
        let mut v = vec![
            line(attr::ENABLED, flag(a.enabled)),
            line(attr::HOST, a.host.as_str()),
            line(attr::PORT, a.port.to_string()),
            line(attr::SECURITY, a.security.text()),
        ];
        if let Some(d) = &a.bind_dn {
            v.push(line(attr::BIND_DN, d.as_str()));
        }
        if let Some(p) = &a.bind_pw {
            v.push(line(attr::BIND_PW, p.clone()));
        }
        if let Some(d) = &a.parent_dn {
            v.push(line(attr::PARENT_DN, d.as_str()));
        }
        v.push(line(attr::CREATE_PARENT, flag(a.create_parent)));
        v.push(line(attr::RETRY, a.retry.to_string()));
        v.push(line(attr::RESYNC, a.resync.to_string()));
        if let Some(p) = &a.publisher {
            v.push(line(attr::PUBLISHER, p.as_str()));
        }
        records.push(Record {
            line: 0,
            dn: a.kind.dn(),
            controls: Vec::new(),
            body: Body::Content(v),
        });
    }
    ldif::write(&Ldif {
        version: Some(1),
        charset: None,
        records,
    })
}

/// The authenticated data `kind`'s password is sealed with.
#[must_use]
pub fn aad(kind: Kind) -> Vec<u8> {
    format!("QS400 DIRSRV PUBLISH {}", kind.name()).into_bytes()
}

/// `plain` sealed for `kind`'s agent.
///
/// # Errors
///
/// [`ConfigError::Seal`].
pub fn seal_password(sealer: &dyn Sealer, kind: Kind, plain: &str) -> Result<Vec<u8>, ConfigError> {
    sealer
        .seal(&aad(kind), plain.as_bytes())
        .map_err(|e| ConfigError::Seal(e.to_string()))
}

/// `kind`'s sealed password opened.
///
/// # Errors
///
/// [`ConfigError::Seal`] when the key or the agent does not match.
pub fn unseal_password(key: &dyn Sealer, kind: Kind, sealed: &[u8]) -> Result<String, ConfigError> {
    let b = key
        .unseal(&aad(kind), sealed)
        .map_err(|e| ConfigError::Seal(e.to_string()))?;
    String::from_utf8(b).map_err(|_| ConfigError::Seal("not UTF-8".into()))
}
