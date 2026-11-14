// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One agent's publisher (docs/DIRSRV.md §10.1): the changes it still
//! has to make, the entries it published, its connection to the target,
//! its retry clock and its status. Only entries carrying the agent's
//! `publisherName` value are ever modified, renamed or deleted.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use ldap::client::{Client, ClientError, Options, SearchEntry, SearchResult};
use ldap::code::ResultCode;
use ldap::dn::Dn;
use ldap::filter;
use ldap::msg::{Attribute, LdapResult, Scope, SearchRequest};
use tls::rustls::ClientConfig;

use super::config::{AgentConfig, Security};
use super::map::{Object, Wanted, wanted};
use super::source::Change;

/// What an agent reports (Navigator, QAP).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    /// Whether the agent publishes.
    pub enabled: bool,
    /// Whether it holds a connection to the target now.
    pub connected: bool,
    /// When the target last took all that was pending (seconds since
    /// the epoch).
    pub last_success: Option<u64>,
    /// The last error and when.
    pub last_error: Option<(u64, String)>,
    /// Changes still to make.
    pub pending: u32,
    /// Entries published, as of the last resync and the changes since.
    pub published: u32,
    /// Entries the target refused since the agent started.
    pub errors: u32,
    /// When the target is tried again, while it is down.
    pub next_try: Option<u64>,
}

/// How a publisher reaches its target: a connected client, bound as the
/// agent says.
pub trait Connect {
    /// A client for `agent`, bound with `password`.
    ///
    /// # Errors
    ///
    /// The connection's, the handshake's or the bind's
    /// ([`ClientError::Result`]).
    fn connect(
        &mut self,
        agent: &AgentConfig,
        password: Option<&str>,
    ) -> Result<Client, ClientError>;
}

/// The connector over TCP: in the clear, LDAPS or `StartTLS` with `tls`.
#[derive(Debug, Clone)]
pub struct TcpConnect {
    /// The TLS configuration for `SSL` and `STARTTLS`.
    pub tls: Option<Arc<ClientConfig>>,
    /// The client's options.
    pub options: Options,
}

impl TcpConnect {
    /// A connector waiting at most `timeout` for a connection or an
    /// answer.
    #[must_use]
    pub fn new(tls: Option<Arc<ClientConfig>>, timeout: Duration) -> Self {
        let options = Options {
            connect_timeout: timeout,
            timeout: Some(timeout),
            ..Options::default()
        };
        Self { tls, options }
    }
}

impl Connect for TcpConnect {
    fn connect(&mut self, a: &AgentConfig, password: Option<&str>) -> Result<Client, ClientError> {
        let need_tls = || {
            self.tls.clone().ok_or(ClientError::Unsupported(
                "TLS without a client configuration",
            ))
        };
        let mut c = match a.security {
            Security::None => Client::connect(&a.host, a.port, self.options.clone())?,
            Security::Ssl => {
                Client::connect_tls(&a.host, a.port, need_tls()?, self.options.clone())?
            }
            Security::StartTls => {
                let cfg = need_tls()?;
                let mut c = Client::connect(&a.host, a.port, self.options.clone())?;
                c.start_tls(cfg)?;
                c
            }
        };
        if let Some(dn) = &a.bind_dn {
            c.simple_bind(dn, password.unwrap_or("").as_bytes())?
                .success()?;
        }
        Ok(c)
    }
}

/// Why a step stopped.
enum Fail {
    /// The target: keep everything and try again later.
    Down(String),
    /// This entry only.
    Entry(String),
}

type R<T> = Result<T, Fail>;

fn down(e: &ClientError) -> Fail {
    Fail::Down(e.to_string())
}

/// Codes that say the target, not the entry, is the trouble.
fn is_down_code(c: ResultCode) -> bool {
    matches!(
        c,
        ResultCode::Busy | ResultCode::Unavailable | ResultCode::Other(80)
    )
}

fn result_text(r: &LdapResult) -> String {
    if r.message.is_empty() {
        format!("{} ({})", r.code.text(), r.code.number())
    } else {
        format!("{} ({}): {}", r.code.text(), r.code.number(), r.message)
    }
}

/// A result that is not a success, as a failure of `what`.
fn failed(r: &LdapResult, what: &str) -> Fail {
    let t = format!("{what}: {}", result_text(r));
    if is_down_code(r.code) {
        Fail::Down(t)
    } else {
        Fail::Entry(t)
    }
}

/// Two DNs the same once parsed (ASCII case ignored).
fn same_dn(a: &str, b: &str) -> bool {
    match (Dn::parse(a), Dn::parse(b)) {
        (Ok(a), Ok(b)) => a.to_string().eq_ignore_ascii_case(&b.to_string()),
        _ => a.eq_ignore_ascii_case(b),
    }
}

/// The most times the retry interval doubles.
const MAX_DOUBLINGS: u32 = 6;
/// The longest wait between tries, seconds.
const MAX_WAIT: u64 = 86_400;
/// The page size of a resync's search.
const PAGE: i32 = 500;

/// One agent's publisher.
pub struct Publisher {
    agent: AgentConfig,
    password: Option<String>,
    owner: String,
    parent: Dn,
    conn: Option<Client>,
    parent_checked: bool,
    /// The last change wanted per object: an entry, or none to delete.
    queue: BTreeMap<String, Option<Wanted>>,
    /// A full resync to make, before the queue.
    resync: Option<BTreeMap<String, Wanted>>,
    /// The DNs of the entries this agent published, by object.
    known: BTreeMap<String, String>,
    failures: u32,
    next_try: u64,
    last_resync: Option<u64>,
    status: Status,
}

impl std::fmt::Debug for Publisher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Publisher")
            .field("kind", &self.agent.kind)
            .field("publisher", &self.owner)
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

impl Publisher {
    /// A publisher for `agent` on system `system`, binding with
    /// `password` (in the clear).
    ///
    /// # Errors
    ///
    /// The parent DN does not parse.
    pub fn new(agent: AgentConfig, password: Option<String>, system: &str) -> Result<Self, String> {
        let parent = Dn::parse(agent.parent())
            .map_err(|_| format!("the parent DN {} is not a DN", agent.parent()))?;
        let publisher = agent.publisher_name(system);
        let status = Status {
            enabled: agent.enabled,
            ..Status::default()
        };
        Ok(Self {
            agent,
            password,
            owner: publisher,
            parent,
            conn: None,
            parent_checked: false,
            queue: BTreeMap::new(),
            resync: None,
            known: BTreeMap::new(),
            failures: 0,
            next_try: 0,
            last_resync: None,
            status,
        })
    }

    /// The agent's settings.
    #[must_use]
    pub const fn agent(&self) -> &AgentConfig {
        &self.agent
    }

    /// The `publisherName` value this agent writes and owns.
    #[must_use]
    pub fn publisher_name(&self) -> &str {
        &self.owner
    }

    /// The status now.
    #[must_use]
    pub fn status(&self) -> Status {
        let mut s = self.status.clone();
        s.connected = self.conn.is_some();
        s.pending = u32::try_from(self.pending()).unwrap_or(u32::MAX);
        s.published = u32::try_from(self.known.len()).unwrap_or(u32::MAX);
        s.next_try = (self.failures > 0).then_some(self.next_try);
        s
    }

    /// Changes still to make.
    #[must_use]
    pub fn pending(&self) -> usize {
        self.queue.len() + self.resync.as_ref().map_or(0, BTreeMap::len)
    }

    /// Whether a full resync is waiting.
    #[must_use]
    pub const fn resync_pending(&self) -> bool {
        self.resync.is_some()
    }

    /// Whether the periodic resync is due at `now`.
    #[must_use]
    pub fn resync_due(&self, now: u64) -> bool {
        self.resync.is_none()
            && self.agent.resync > 0
            && self
                .last_resync
                .is_some_and(|t| now >= t + u64::from(self.agent.resync))
    }

    /// Publish exactly `objects`: what is published and not among them
    /// is deleted. Replaces whatever was queued.
    pub fn resync(&mut self, objects: &[Object]) {
        let kind = self.agent.kind;
        let mut want = BTreeMap::new();
        for o in objects.iter().filter(|o| o.kind() == kind) {
            let w = wanted(o, &self.owner);
            want.insert(w.key.clone(), w);
        }
        self.queue.clear();
        self.resync = Some(want);
    }

    /// Queue one change (the last per object wins).
    pub fn apply(&mut self, change: &Change) {
        match change {
            Change::Created(o) | Change::Changed(o) if o.kind() == self.agent.kind => {
                let w = wanted(o, &self.owner);
                self.queue.insert(w.key.clone(), Some(w));
            }
            Change::Deleted(k, name) if *k == self.agent.kind => {
                self.queue.insert(name.trim().to_string(), None);
            }
            _ => {}
        }
    }

    /// Whether there is work and the retry clock lets it run at `now`.
    #[must_use]
    pub fn due(&self, now: u64) -> bool {
        self.pending() > 0 && now >= self.next_try
    }

    /// Drop the connection (the job is ending, or the settings changed).
    pub fn close(&mut self) {
        if let Some(c) = self.conn.take() {
            let _ = c.unbind();
        }
        self.parent_checked = false;
    }

    /// Make what is pending, if it is due at `now`. A target that is
    /// down keeps everything for the next try; an entry the target
    /// refuses is logged and left for the next resync.
    pub fn run(&mut self, connect: &mut dyn Connect, now: u64) {
        if !self.due(now) {
            return;
        }
        match self.work(connect, now) {
            Ok(()) => {
                self.failures = 0;
                self.next_try = 0;
                self.status.last_success = Some(now);
            }
            Err(Fail::Down(e) | Fail::Entry(e)) => {
                self.close();
                self.failures += 1;
                let wait = u64::from(self.agent.retry) << (self.failures - 1).min(MAX_DOUBLINGS);
                self.next_try = now + wait.min(MAX_WAIT);
                tracing::warn!(agent = self.agent.kind.name(), "publishing target: {e}");
                self.status.last_error = Some((now, e));
            }
        }
    }

    fn work(&mut self, connect: &mut dyn Connect, now: u64) -> R<()> {
        if self.conn.is_none() {
            let c = connect
                .connect(&self.agent, self.password.as_deref())
                .map_err(|e| match e {
                    ClientError::Result(r) => Fail::Down(format!("bind: {}", result_text(&r))),
                    e => down(&e),
                })?;
            self.conn = Some(c);
        }
        if !self.parent_checked {
            self.ensure_parent()?;
            self.parent_checked = true;
        }
        if let Some(want) = self.resync.clone() {
            self.full(&want, now)?;
            self.resync = None;
            self.last_resync = Some(now);
        }
        while let Some((key, w)) = self.queue.pop_first() {
            let r = match &w {
                Some(w) => self.upsert(w),
                None => self.remove(&key),
            };
            match r {
                Ok(()) => {}
                Err(Fail::Entry(e)) => self.entry_error(now, e),
                Err(e @ Fail::Down(_)) => {
                    self.queue.entry(key).or_insert(w);
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    fn entry_error(&mut self, now: u64, e: String) {
        tracing::warn!(agent = self.agent.kind.name(), "publishing: {e}");
        self.status.errors += 1;
        self.status.last_error = Some((now, e));
    }

    fn client(&mut self) -> R<&mut Client> {
        self.conn
            .as_mut()
            .ok_or_else(|| Fail::Down("not connected".into()))
    }

    fn search(
        &mut self,
        base: &str,
        scope: Scope,
        filter: &str,
        attrs: &[&str],
    ) -> R<SearchResult> {
        let c = self.client()?;
        let req: SearchRequest = c
            .search_request(base, scope, filter, attrs)
            .map_err(|e| down(&e))?;
        let r = if scope == Scope::Base {
            c.search(&req, &[])
        } else {
            match c.search_paged(&req, PAGE, &[]) {
                Ok(r) if r.result.code == ResultCode::UnavailableCriticalExtension => {
                    c.search(&req, &[])
                }
                r => r,
            }
        };
        r.map_err(|e| down(&e))
    }

    /// The entry at `dn`, with its key and publisher values; none when
    /// it does not exist.
    fn read(&mut self, dn: &str) -> R<Option<SearchEntry>> {
        let key = self.agent.kind.key_attr();
        let r = self.search(dn, Scope::Base, "(objectClass=*)", &["publisherName", key])?;
        match r.result.code {
            ResultCode::Success => Ok(r.entries.into_iter().next()),
            ResultCode::NoSuchObject => Ok(None),
            _ => Err(failed(&r.result, &format!("read {dn}"))),
        }
    }

    fn is_ours(&self, e: &SearchEntry) -> bool {
        e.values("publisherName").is_some_and(|v| {
            v.iter()
                .any(|v| v.eq_ignore_ascii_case(self.owner.as_bytes()))
        })
    }

    fn key_of(&self, e: &SearchEntry) -> Option<String> {
        e.first(self.agent.kind.key_attr())
            .map(|s| s.trim().to_string())
    }

    /// The parent, and its missing ancestors when the settings let it.
    fn ensure_parent(&mut self) -> R<()> {
        let parent = self.parent.to_string();
        let r = self.search(&parent, Scope::Base, "(objectClass=*)", &["1.1"])?;
        match r.result.code {
            ResultCode::Success => return Ok(()),
            ResultCode::NoSuchObject => {}
            _ => {
                return Err(Fail::Down(format!(
                    "parent {parent}: {}",
                    result_text(&r.result)
                )));
            }
        }
        if !self.agent.create_parent {
            return Err(Fail::Down(format!("the parent {parent} does not exist")));
        }
        let matched = Dn::parse(&r.result.matched_dn).map_or(0, |d| d.len());
        // From the highest missing ancestor down to the parent.
        let n = self.parent.len();
        for depth in (0..n.saturating_sub(matched)).rev() {
            let dn = Dn(self.parent.0[depth..].to_vec());
            let attrs = container(&dn).ok_or_else(|| {
                Fail::Down(format!(
                    "{dn} cannot be added: its RDN type is not a container's"
                ))
            })?;
            let c = self.client()?;
            let resp = c.add(&dn.to_string(), attrs, &[]).map_err(|e| down(&e))?;
            match resp.result.code {
                ResultCode::Success | ResultCode::EntryAlreadyExists => {}
                _ => {
                    return Err(Fail::Down(format!(
                        "add {dn}: {}",
                        result_text(&resp.result)
                    )));
                }
            }
        }
        Ok(())
    }

    /// The DN of the entry this agent published for `key`.
    fn find(&mut self, key: &str) -> R<Option<String>> {
        if let Some(dn) = self.known.get(key) {
            return Ok(Some(dn.clone()));
        }
        let ka = self.agent.kind.key_attr();
        let f = format!(
            "(&(publisherName={})({ka}={}))",
            filter::escape(self.owner.as_bytes()),
            filter::escape(key.as_bytes())
        );
        let parent = self.parent.to_string();
        let r = self.search(&parent, Scope::Sub, &f, &["publisherName", ka])?;
        match r.result.code {
            ResultCode::Success => {}
            ResultCode::NoSuchObject => return Ok(None),
            _ => return Err(failed(&r.result, &format!("search for {key}"))),
        }
        let found = r
            .entries
            .iter()
            .find(|e| self.is_ours(e) && self.key_of(e).as_deref() == Some(key))
            .map(|e| e.dn.clone());
        if let Some(dn) = &found {
            self.known.insert(key.to_string(), dn.clone());
        }
        Ok(found)
    }

    /// Add or bring up to date the entry of `w`.
    fn upsert(&mut self, w: &Wanted) -> R<()> {
        let primary = w.dn(&self.parent, false);
        let fallback = w.dn(&self.parent, true);
        match self.find(&w.key)? {
            Some(dn) => {
                let dn = if same_dn(&dn, &primary) || same_dn(&dn, &fallback) {
                    dn
                } else {
                    self.rename(&dn, w)?
                };
                self.modify(&dn, w)
            }
            None => self.add(w),
        }
    }

    fn rename(&mut self, dn: &str, w: &Wanted) -> R<String> {
        for fallback in [false, true] {
            let rdn = if fallback { &w.fallback } else { &w.rdn };
            let c = self.client()?;
            let r = c
                .modify_dn(dn, &rdn.to_string(), true, None, &[])
                .map_err(|e| down(&e))?;
            match r.result.code {
                ResultCode::Success => {
                    let new = w.dn(&self.parent, fallback);
                    self.known.insert(w.key.clone(), new.clone());
                    return Ok(new);
                }
                ResultCode::EntryAlreadyExists if !fallback => {}
                _ => return Err(failed(&r.result, &format!("rename {dn}"))),
            }
        }
        Err(Fail::Entry(format!("rename {dn}: both names are taken")))
    }

    fn modify(&mut self, dn: &str, w: &Wanted) -> R<()> {
        let c = self.client()?;
        let r = c.modify(dn, w.changes(), &[]).map_err(|e| down(&e))?;
        match r.result.code {
            ResultCode::Success => {
                self.known.insert(w.key.clone(), dn.to_string());
                Ok(())
            }
            ResultCode::NoSuchObject => {
                // Gone since it was found: add it again.
                self.known.remove(&w.key);
                self.add(w)
            }
            _ => Err(failed(&r.result, &format!("modify {dn}"))),
        }
    }

    fn add(&mut self, w: &Wanted) -> R<()> {
        for fallback in [false, true] {
            let dn = w.dn(&self.parent, fallback);
            let c = self.client()?;
            let r = c.add(&dn, w.add_attrs(), &[]).map_err(|e| down(&e))?;
            match r.result.code {
                ResultCode::Success => {
                    self.known.insert(w.key.clone(), dn);
                    return Ok(());
                }
                ResultCode::EntryAlreadyExists => {
                    // Ours for this object: bring it up to date; anyone
                    // else's: never touched.
                    if let Some(e) = self.read(&dn)?
                        && self.is_ours(&e)
                        && self.key_of(&e).as_deref() == Some(w.key.as_str())
                    {
                        return self.modify(&dn, w);
                    }
                }
                ResultCode::NoSuchObject => {
                    self.parent_checked = false;
                    return Err(Fail::Down(format!("add {dn}: the parent is gone")));
                }
                _ => return Err(failed(&r.result, &format!("add {dn}"))),
            }
        }
        Err(Fail::Entry(format!(
            "{} {}: its names are held by entries this agent did not publish",
            self.agent.kind.key_attr(),
            w.key
        )))
    }

    fn remove(&mut self, key: &str) -> R<()> {
        let Some(dn) = self.find(key)? else {
            return Ok(());
        };
        self.delete(key, &dn)
    }

    fn delete(&mut self, key: &str, dn: &str) -> R<()> {
        let c = self.client()?;
        let r = c.delete(dn, &[]).map_err(|e| down(&e))?;
        match r.result.code {
            ResultCode::Success | ResultCode::NoSuchObject => {
                if self.known.get(key).is_some_and(|k| same_dn(k, dn)) {
                    self.known.remove(key);
                }
                Ok(())
            }
            _ => Err(failed(&r.result, &format!("delete {dn}"))),
        }
    }

    /// The full resync: every wanted entry made, every published entry
    /// whose object is gone deleted.
    fn full(&mut self, want: &BTreeMap<String, Wanted>, now: u64) -> R<()> {
        let ka = self.agent.kind.key_attr();
        let f = format!("(publisherName={})", filter::escape(self.owner.as_bytes()));
        let parent = self.parent.to_string();
        let r = self.search(&parent, Scope::Sub, &f, &["publisherName", ka])?;
        let entries = match r.result.code {
            ResultCode::Success => r.entries,
            ResultCode::NoSuchObject => Vec::new(),
            _ => {
                return Err(Fail::Down(format!(
                    "search {parent}: {}",
                    result_text(&r.result)
                )));
            }
        };
        self.known.clear();
        let mut stale: Vec<(String, String)> = Vec::new();
        for e in &entries {
            // Never another publisher's, whatever the server's filter did.
            if !self.is_ours(e) {
                continue;
            }
            match self.key_of(e) {
                Some(k) if !self.known.contains_key(&k) => {
                    self.known.insert(k, e.dn.clone());
                }
                // A second entry for the object, or one without a key.
                k => stale.push((k.unwrap_or_default(), e.dn.clone())),
            }
        }
        for w in want.values() {
            match self.upsert(w) {
                Ok(()) => {}
                Err(Fail::Entry(e)) => self.entry_error(now, e),
                Err(e) => return Err(e),
            }
        }
        let gone: Vec<(String, String)> = self
            .known
            .iter()
            .filter(|(k, _)| !want.contains_key(*k))
            .map(|(k, d)| (k.clone(), d.clone()))
            .collect();
        for (k, dn) in gone.into_iter().chain(stale) {
            match self.delete(&k, &dn) {
                Ok(()) => {}
                Err(Fail::Entry(e)) => self.entry_error(now, e),
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

/// The attributes of a container entry named `dn`.
fn container(dn: &Dn) -> Option<Vec<Attribute>> {
    let ava = dn.rdn()?.0.first()?;
    let class = match ava.ty.to_ascii_lowercase().as_str() {
        "ou" => "organizationalUnit",
        "o" => "organization",
        "dc" => "domain",
        "cn" => "container",
        "l" => "locality",
        _ => return None,
    };
    let mut attrs = vec![Attribute::new("objectClass", &["top", class])];
    for a in &dn.rdn()?.0 {
        attrs.push(Attribute::new(&a.ty, &[a.value_str()?]));
    }
    Some(attrs)
}
