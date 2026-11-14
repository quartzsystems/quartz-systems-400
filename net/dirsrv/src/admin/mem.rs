// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! [`MemAdmin`]: a directory server in memory behind [`DirAdmin`], for
//! tests, Navigator's fixture and QAP's test system. Its instances hold
//! a real `ibmslapd.conf` text (changed through [`super::conf`]) and the
//! shipped schema; entries, counters and the publishing agents are a
//! small sample.
//!
//! The sample ([`MemAdmin::new`]): `QUSRDIR` running with suffix
//! `dc=quartz,dc=example`, entries under it with an ACL and an owner,
//! one configured index; `DEVDIR` stopped on ports 1389 and 1636; the
//! users agent enabled toward the local instance, the printers agent
//! not.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

use ldap::msg::{Attribute, ModOp, Modification};

use super::{
    Access, AdminChange, AdminError, AdminInfo, AgentKind, AgentStatus, Counters, DirAdmin,
    ErrorKind, Index, InstanceInfo, InstanceState, Properties, PublishAgent, Result, SchemaDef,
    SchemaKind, SchemaPage, SchemaQuery, Suffix, SuffixKind, access, conf,
};
use crate::config::{ConfigError, DEFAULT_INSTANCE, ROOT, SlapdConf, dn_key, shipped};
use crate::schema::Schema;

/// Whether profile `user` has special authority `what`.
pub type HasSpecial = dyn Fn(&str, &str) -> bool + Send + Sync;

/// The default ACL of an entry no ACL reaches (docs/DIRSRV.md §12).
pub const DEFAULT_ACL: &str = "group:cn=anybody:normal:rsc";

#[derive(Debug, Clone)]
struct Entry {
    dn: String,
    acl: Option<(Vec<String>, bool)>,
    owner: Option<(Vec<String>, bool)>,
}

struct Inst {
    info: InstanceInfo,
    conf: SlapdConf,
    schema: Schema,
    entries: BTreeMap<String, Entry>,
    counters: Counters,
}

struct State {
    instances: Vec<Inst>,
    agents: Vec<(PublishAgent, Option<String>)>,
    status: Vec<AgentStatus>,
    now: u64,
    next_job: u32,
}

/// A directory server in memory.
pub struct MemAdmin {
    state: Mutex<State>,
    has: Box<HasSpecial>,
}

fn entry(dn: &str) -> Entry {
    Entry {
        dn: dn.into(),
        acl: None,
        owner: None,
    }
}

fn instance(name: &str, system: &str, port: u16, now: u64, running: bool) -> Inst {
    let path = crate::config::conf_path(ROOT, name);
    let mut conf = SlapdConf::parse(&path, &shipped::slapd_conf(name, system))
        .expect("the shipped file reads");
    if port != 389 {
        let p = Properties {
            port,
            secure_port: port + 247,
            ..conf::properties(&conf)
        };
        conf::set_properties(&mut conf, &p).expect("the ports are valid");
    }
    let mut entries = BTreeMap::new();
    for dn in ["cn=localhost", "cn=ibmpolicies"] {
        entries.insert(dn_key(dn), entry(dn));
    }
    let p = conf::properties(&conf);
    let mut inst = Inst {
        info: InstanceInfo {
            name: name.into(),
            desc: name.into(),
            autostart: running,
            state: InstanceState::Stopped,
            job: String::new(),
            port: p.port,
            secure_port: p.secure_port,
            security: p.security,
            certificate: String::new(),
            version: "6.1".into(),
            location: ROOT.into(),
            since: 0,
        },
        conf,
        schema: Schema::shipped().expect("the shipped schema resolves"),
        entries,
        counters: Counters::default(),
    };
    if running {
        inst.info.state = InstanceState::Running;
        inst.info.job = format!("123470/QDIRSRV/{name}");
        inst.info.since = now.saturating_sub(86_400);
    }
    inst
}

fn not_found(instance: &str) -> AdminError {
    ConfigError::NoInstance(instance.trim().to_ascii_uppercase()).into()
}

fn sample_counters(since: u64) -> Counters {
    Counters {
        started: since,
        connections: 4,
        total_connections: 1_288,
        ops_initiated: 40_216,
        ops_completed: 40_212,
        binds: 2_051,
        searches: 36_904,
        compares: 12,
        adds: 640,
        modifies: 518,
        deletes: 37,
        modrdns: 3,
        extended: 51,
        abandons: 2,
        entries_sent: 118_330,
        persistent: 1,
    }
}

fn status_of(kind: AgentKind) -> AgentStatus {
    let job = kind.job_name();
    AgentStatus {
        kind,
        job: String::new(),
        active: false,
        published: 0,
        failed: 0,
        last: 0,
        msgid: "GLD010B".into(),
        text: format!("Publishing agent {job} ended: publishing is not available yet."),
    }
}

impl MemAdmin {
    /// The sample on system `system` (its DNS name, as the projected
    /// suffix names it), at `now` seconds since 1970; `has` answers
    /// whether a profile has a special authority.
    #[must_use]
    pub fn new(system: &str, now: u64, has: Box<HasSpecial>) -> Self {
        let mut q = instance(DEFAULT_INSTANCE, system, 389, now, true);
        conf::add_suffix(&mut q.conf, "dc=quartz,dc=example").expect("a new suffix");
        conf::put_index(
            &mut q.conf,
            &Index {
                attr: "telephoneNumber".into(),
                presence: true,
                equality: true,
                ..Index::default()
            },
        )
        .expect("an index");
        let mut top = entry("dc=quartz,dc=example");
        top.acl = Some((
            vec![
                DEFAULT_ACL.into(),
                "group:cn=Directory Admins,ou=Groups,dc=quartz,dc=example:object:ad:normal:rwsc:sensitive:rwsc".into(),
            ],
            true,
        ));
        top.owner = Some((vec!["access-id:cn=Administrator".into()], true));
        let mut people = entry("ou=People,dc=quartz,dc=example");
        people.owner = Some((
            vec!["group:cn=Directory Admins,ou=Groups,dc=quartz,dc=example".into()],
            true,
        ));
        for e in [
            top,
            people,
            entry("ou=Groups,dc=quartz,dc=example"),
            entry("cn=Directory Admins,ou=Groups,dc=quartz,dc=example"),
            entry("uid=jsmith,ou=People,dc=quartz,dc=example"),
        ] {
            q.entries.insert(dn_key(&e.dn), e);
        }
        q.counters = sample_counters(q.info.since);
        let dev = instance("DEVDIR", system, 1389, now, false);
        let users = PublishAgent {
            kind: AgentKind::Users,
            enabled: true,
            server: String::new(),
            port: 389,
            tls: false,
            bind_dn: "cn=Administrator".into(),
            password_set: true,
            parent_dn: "ou=Users,dc=quartz,dc=example".into(),
        };
        let printers = PublishAgent {
            kind: AgentKind::Printers,
            enabled: false,
            parent_dn: "ou=Printers,dc=quartz,dc=example".into(),
            password_set: false,
            ..users.clone()
        };
        Self {
            state: Mutex::new(State {
                instances: vec![q, dev],
                agents: vec![(users, Some("{SSHA512}x".into())), (printers, None)],
                status: AgentKind::ALL.into_iter().map(status_of).collect(),
                now,
                next_job: 123_501,
            }),
            has,
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// `CPF222E` naming the first of `what` `user` lacks.
    fn need(&self, user: &str, what: &[&str]) -> Result<()> {
        match what.iter().find(|w| !(self.has)(user, w)) {
            Some(w) => Err(AdminError::special(w)),
            None => Ok(()),
        }
    }

    fn read(&self, user: &str) -> Result<()> {
        self.need(user, &["*IOSYSCFG"])
    }

    fn change(&self, user: &str) -> Result<()> {
        self.need(user, &["*ALLOBJ", "*IOSYSCFG"])
    }

    fn with<T>(&self, instance: &str, f: impl FnOnce(&mut Inst) -> Result<T>) -> Result<T> {
        let mut s = self.lock();
        let i = s
            .instances
            .iter_mut()
            .find(|i| i.info.name.eq_ignore_ascii_case(instance.trim()))
            .ok_or_else(|| not_found(instance))?;
        f(i)
    }

    /// The instance's configuration file text, for tests.
    #[must_use]
    pub fn conf_text(&self, instance: &str) -> Option<String> {
        self.with(instance, |i| Ok(i.conf.text().to_string())).ok()
    }
}

fn def_of(s: &Schema, kind: SchemaKind, oid: &str) -> Option<SchemaDef> {
    let name_of = |id: &String| {
        s.attribute(id)
            .map_or_else(|| id.clone(), |a| a.name().to_string())
    };
    match kind {
        SchemaKind::Attribute => {
            let a = s.attribute(oid)?;
            Some(SchemaDef {
                kind,
                oid: a.oid().into(),
                names: a.def.names.clone(),
                desc: a.def.desc.clone().unwrap_or_default(),
                sup: a.def.sup.iter().cloned().collect(),
                syntax: match (&a.syntax, a.syntax_len) {
                    (Some(o), Some(n)) => format!("{o}{{{n}}}"),
                    (Some(o), None) => o.clone(),
                    _ => String::new(),
                },
                single_value: a.single_value(),
                access: a.access.keyword().into(),
                custom: s.is_modified(a.oid()),
                protected: s.is_protected(a.oid()),
                text: a.def.to_string(),
                ..SchemaDef::default()
            })
        }
        SchemaKind::Class => {
            let c = s.class(oid)?;
            Some(SchemaDef {
                kind,
                oid: c.oid().into(),
                names: c.def.names.clone(),
                desc: c.def.desc.clone().unwrap_or_default(),
                sup: c.def.sup.clone(),
                class_kind: match c.kind() {
                    ldap::schema::ClassKind::Abstract => "ABSTRACT",
                    ldap::schema::ClassKind::Structural => "STRUCTURAL",
                    ldap::schema::ClassKind::Auxiliary => "AUXILIARY",
                }
                .into(),
                must: c.must.iter().map(name_of).collect(),
                may: c.may.iter().map(name_of).collect(),
                custom: s.is_modified(c.oid()),
                protected: s.is_protected(c.oid()),
                text: c.def.to_string(),
                ..SchemaDef::default()
            })
        }
    }
}

fn schema_attr(kind: SchemaKind) -> &'static str {
    match kind {
        SchemaKind::Attribute => "attributetypes",
        SchemaKind::Class => "objectclasses",
    }
}

fn schema_error(e: &crate::schema::SchemaError) -> AdminError {
    use crate::schema::SchemaError as S;
    let kind = match e {
        S::Duplicate(_) => ErrorKind::Exists,
        S::NotFound(_) => ErrorKind::NotFound,
        _ => ErrorKind::Invalid,
    };
    AdminError::plain(kind, &format!("The schema was not changed: {e}."))
}

/// The entry `dn` and its ancestors' keys, nearest first.
fn lineage(dn: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut d = ldap::Dn::parse(dn.trim()).ok();
    while let Some(x) = d {
        if x.is_root() {
            break;
        }
        out.push(dn_key(&x.to_string()));
        d = x.parent();
    }
    out
}

/// The ACL or owner in effect at `keys[0]`: the values, propagate and
/// the source DN; `None` when nothing reaches it.
fn effective(
    entries: &BTreeMap<String, Entry>,
    keys: &[String],
    pick: impl Fn(&Entry) -> Option<&(Vec<String>, bool)>,
) -> Option<(Vec<String>, bool, String)> {
    for (n, k) in keys.iter().enumerate() {
        if let Some(e) = entries.get(k)
            && let Some((v, prop)) = pick(e)
            && (n == 0 || *prop)
        {
            return Some((v.clone(), *prop, e.dn.clone()));
        }
    }
    None
}

impl DirAdmin for MemAdmin {
    fn instances(&self, user: &str) -> Result<Vec<InstanceInfo>> {
        self.read(user)?;
        Ok(self
            .lock()
            .instances
            .iter()
            .map(|i| i.info.clone())
            .collect())
    }

    fn set_autostart(&self, user: &str, instance: &str, on: bool) -> Result<()> {
        self.change(user)?;
        self.with(instance, |i| {
            i.info.autostart = on;
            Ok(())
        })
    }

    fn start(&self, user: &str, instance: &str) -> Result<()> {
        self.need(user, &["*IOSYSCFG"])?;
        let mut s = self.lock();
        let (now, job) = (s.now, s.next_job);
        let i = s
            .instances
            .iter_mut()
            .find(|i| i.info.name.eq_ignore_ascii_case(instance.trim()))
            .ok_or_else(|| {
                AdminError::from(ConfigError::NoServerInstance(
                    instance.trim().to_ascii_uppercase(),
                ))
            })?;
        if i.info.state == InstanceState::Running {
            return Ok(());
        }
        i.conf.validate(&i.info.name)?;
        let p = conf::properties(&i.conf);
        i.info.state = InstanceState::Running;
        i.info.job = format!("{job:06}/QDIRSRV/{}", i.info.name);
        i.info.since = now;
        (i.info.port, i.info.secure_port, i.info.security) = (p.port, p.secure_port, p.security);
        i.counters = Counters {
            started: now,
            ..Counters::default()
        };
        s.next_job += 1;
        Ok(())
    }

    fn end(&self, user: &str, instance: &str) -> Result<()> {
        self.need(user, &["*IOSYSCFG"])?;
        self.with(instance, |i| {
            i.info.state = InstanceState::Stopped;
            i.info.job.clear();
            i.info.since = 0;
            Ok(())
        })
        .map_err(|_| ConfigError::NoServerInstance(instance.trim().to_ascii_uppercase()).into())
    }

    fn restart(&self, user: &str, instance: &str) -> Result<()> {
        self.end(user, instance)?;
        self.start(user, instance)
    }

    fn properties(&self, user: &str, instance: &str) -> Result<Properties> {
        self.read(user)?;
        self.with(instance, |i| Ok(conf::properties(&i.conf)))
    }

    fn set_properties(&self, user: &str, instance: &str, to: &Properties) -> Result<Vec<String>> {
        self.change(user)?;
        self.with(instance, |i| conf::set_properties(&mut i.conf, to))
    }

    fn suffixes(&self, user: &str, instance: &str) -> Result<Vec<Suffix>> {
        self.read(user)?;
        self.with(instance, |i| {
            let running = i.info.state == InstanceState::Running;
            Ok(conf::suffixes(&i.conf)
                .into_iter()
                .map(|mut s| {
                    if running && matches!(s.kind, SuffixKind::User | SuffixKind::Local) {
                        let k = dn_key(&s.dn);
                        let n = i
                            .entries
                            .keys()
                            .filter(|e| **e == k || e.ends_with(&format!(",{k}")))
                            .count();
                        s.entries = Some(n as u64);
                    }
                    s
                })
                .collect())
        })
    }

    fn add_suffix(&self, user: &str, instance: &str, dn: &str) -> Result<()> {
        self.change(user)?;
        self.with(instance, |i| conf::add_suffix(&mut i.conf, dn))
    }

    fn remove_suffix(&self, user: &str, instance: &str, dn: &str) -> Result<()> {
        self.change(user)?;
        self.with(instance, |i| conf::remove_suffix(&mut i.conf, dn))
    }

    fn indexes(&self, user: &str, instance: &str) -> Result<Vec<Index>> {
        self.read(user)?;
        self.with(instance, |i| Ok(conf::indexes(&i.conf)))
    }

    fn put_index(&self, user: &str, instance: &str, index: &Index) -> Result<()> {
        self.change(user)?;
        self.with(instance, |i| {
            let Some(a) = i.schema.attribute(index.attr.trim()) else {
                return Err(AdminError::plain(
                    ErrorKind::NotFound,
                    &format!("Attribute type {} is not in the schema.", index.attr.trim()),
                ));
            };
            let named = Index {
                attr: a.name().to_string(),
                ..index.clone()
            };
            conf::put_index(&mut i.conf, &named)
        })
    }

    fn remove_index(&self, user: &str, instance: &str, attr: &str) -> Result<()> {
        self.change(user)?;
        self.with(instance, |i| conf::remove_index(&mut i.conf, attr))
    }

    fn admin(&self, user: &str, instance: &str) -> Result<AdminInfo> {
        self.read(user)?;
        self.with(instance, |i| Ok(conf::admin(&i.conf)))
    }

    fn set_admin(
        &self,
        user: &str,
        instance: &str,
        change: &AdminChange,
        password: &str,
    ) -> Result<()> {
        self.change(user)?;
        self.with(instance, |i| {
            let name = i.info.name.clone();
            conf::set_admin(&mut i.conf, &name, change, password)
        })
    }

    fn schema(&self, user: &str, instance: &str, q: &SchemaQuery) -> Result<SchemaPage> {
        self.read(user)?;
        self.with(instance, |i| {
            let needle = q.contains.trim().to_ascii_lowercase();
            let oids: Vec<String> = match q.kind {
                SchemaKind::Attribute => i.schema.attributes().map(|a| a.oid().into()).collect(),
                SchemaKind::Class => i.schema.classes().map(|c| c.oid().into()).collect(),
            };
            let mut all: Vec<SchemaDef> = oids
                .iter()
                .filter_map(|o| def_of(&i.schema, q.kind, o))
                .filter(|d| !q.custom_only || d.custom)
                .filter(|d| {
                    needle.is_empty()
                        || d.oid.contains(&needle)
                        || d.names
                            .iter()
                            .any(|n| n.to_ascii_lowercase().contains(&needle))
                })
                .collect();
            let key = |d: &SchemaDef| {
                d.names
                    .first()
                    .map_or_else(|| d.oid.clone(), |n| n.to_ascii_lowercase())
            };
            all.sort_by_key(key);
            let total = u32::try_from(all.len()).unwrap_or(u32::MAX);
            let start = usize::try_from(q.start).unwrap_or(usize::MAX);
            let take = if q.max == 0 {
                usize::MAX
            } else {
                usize::try_from(q.max).unwrap_or(usize::MAX)
            };
            Ok(SchemaPage {
                total,
                defs: all.into_iter().skip(start).take(take).collect(),
            })
        })
    }

    fn add_schema(&self, user: &str, instance: &str, kind: SchemaKind, text: &str) -> Result<()> {
        self.change(user)?;
        self.with(instance, |i| {
            let m = Modification {
                op: ModOp::Add,
                attr: Attribute {
                    ty: schema_attr(kind).into(),
                    vals: vec![text.trim().as_bytes().to_vec()],
                },
            };
            i.schema.modify(&[m]).map_err(|e| schema_error(&e))
        })
    }

    fn remove_schema(&self, user: &str, instance: &str, kind: SchemaKind, id: &str) -> Result<()> {
        self.change(user)?;
        self.with(instance, |i| {
            let oid = match kind {
                SchemaKind::Attribute => i.schema.attribute(id.trim()).map(|a| a.oid().to_string()),
                SchemaKind::Class => i.schema.class(id.trim()).map(|c| c.oid().to_string()),
            }
            .ok_or_else(|| schema_error(&crate::schema::SchemaError::NotFound(id.trim().into())))?;
            if i.schema.is_protected(&oid) {
                return Err(schema_error(&crate::schema::SchemaError::Protected(
                    id.trim().into(),
                )));
            }
            let m = Modification {
                op: ModOp::Delete,
                attr: Attribute {
                    ty: schema_attr(kind).into(),
                    vals: vec![format!("( {oid} )").into_bytes()],
                },
            };
            i.schema.modify(&[m]).map_err(|e| schema_error(&e))
        })
    }

    fn access(&self, user: &str, instance: &str, dn: &str) -> Result<Access> {
        self.read(user)?;
        self.with(instance, |i| {
            let keys = lineage(dn);
            let Some(e) = keys.first().and_then(|k| i.entries.get(k)) else {
                return Err(AdminError::plain(
                    ErrorKind::NotFound,
                    &format!("Entry {} was not found.", dn.trim()),
                ));
            };
            let admin = conf::admin(&i.conf).dn;
            let (acl, acl_propagate, acl_source) = effective(&i.entries, &keys, |e| e.acl.as_ref())
                .unwrap_or_else(|| (vec![DEFAULT_ACL.into()], true, "default".into()));
            let (owners, owner_propagate, owner_source) = effective(&i.entries, &keys, |e| {
                e.owner.as_ref()
            })
            .unwrap_or_else(|| (vec![format!("access-id:{admin}")], true, "default".into()));
            Ok(Access {
                dn: e.dn.clone(),
                explicit_acl: e.acl.is_some(),
                acl,
                acl_propagate,
                acl_source,
                explicit_owner: e.owner.is_some(),
                owners,
                owner_propagate,
                owner_source,
            })
        })
    }

    fn set_access(&self, user: &str, instance: &str, a: &Access) -> Result<()> {
        self.change(user)?;
        if a.explicit_acl {
            if a.acl.is_empty() {
                return Err(AdminError::plain(
                    ErrorKind::Invalid,
                    "An entry's own ACL needs at least one value.",
                ));
            }
            a.acl.iter().try_for_each(|v| access::check_acl(v))?;
        }
        if a.explicit_owner {
            if a.owners.is_empty() {
                return Err(AdminError::plain(
                    ErrorKind::Invalid,
                    "An entry's own owners need at least one value.",
                ));
            }
            a.owners.iter().try_for_each(|v| access::check_owner(v))?;
        }
        self.with(instance, |i| {
            let Some(e) = lineage(&a.dn).first().and_then(|k| i.entries.get_mut(k)) else {
                return Err(AdminError::plain(
                    ErrorKind::NotFound,
                    &format!("Entry {} was not found.", a.dn.trim()),
                ));
            };
            e.acl = a.explicit_acl.then(|| (a.acl.clone(), a.acl_propagate));
            e.owner = a
                .explicit_owner
                .then(|| (a.owners.clone(), a.owner_propagate));
            Ok(())
        })
    }

    fn publishing(&self, user: &str) -> Result<Vec<PublishAgent>> {
        self.read(user)?;
        Ok(self.lock().agents.iter().map(|(a, _)| a.clone()).collect())
    }

    fn set_publishing(
        &self,
        user: &str,
        agent: &PublishAgent,
        password: Option<&str>,
    ) -> Result<()> {
        self.change(user)?;
        for dn in [&agent.bind_dn, &agent.parent_dn] {
            if ldap::Dn::parse(dn.trim()).map_or(true, |d| d.is_root()) {
                return Err(ConfigError::BadDn(dn.trim().into()).into());
            }
        }
        if agent.port == 0 {
            return Err(AdminError::plain(
                ErrorKind::Invalid,
                "The port must be 1 to 65535.",
            ));
        }
        if agent
            .server
            .chars()
            .any(|c| !(c.is_ascii_alphanumeric() || ".-:".contains(c)))
        {
            return Err(AdminError::plain(
                ErrorKind::Invalid,
                &format!("Server {} is not a host name or address.", agent.server),
            ));
        }
        let mut s = self.lock();
        let Some((a, pw)) = s.agents.iter_mut().find(|(a, _)| a.kind == agent.kind) else {
            return Err(AdminError::plain(ErrorKind::NotFound, "No such agent."));
        };
        if let Some(p) = password {
            *pw = (!p.is_empty()).then(|| crate::pwd::hash(p.as_bytes()));
        }
        *a = PublishAgent {
            server: agent.server.trim().into(),
            bind_dn: agent.bind_dn.trim().into(),
            parent_dn: agent.parent_dn.trim().into(),
            password_set: pw.is_some(),
            ..agent.clone()
        };
        Ok(())
    }

    fn publishing_status(&self, user: &str) -> Result<Vec<AgentStatus>> {
        self.read(user)?;
        Ok(self.lock().status.clone())
    }

    fn counters(&self, user: &str, instance: &str) -> Result<Counters> {
        self.read(user)?;
        self.with(instance, |i| {
            if i.info.state != InstanceState::Running {
                return Err(AdminError::plain(
                    ErrorKind::Unavailable,
                    &format!("Directory server instance {} is not active.", i.info.name),
                ));
            }
            Ok(i.counters.clone())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn admin() -> MemAdmin {
        MemAdmin::new(
            "SYSTEM.EXAMPLE",
            1_800_000_000,
            Box::new(|u, _| u == "QSECOFR"),
        )
    }

    #[test]
    fn authority_and_instances() {
        let a = admin();
        assert_eq!(a.instances("WEBUSER").unwrap_err().msgid, "CPF222E");
        let l = a.instances("QSECOFR").unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].state, InstanceState::Running);
        assert_eq!((l[1].port, l[1].secure_port), (1389, 1636));
        a.start("QSECOFR", "devdir").unwrap();
        assert_eq!(
            a.instances("QSECOFR").unwrap()[1].job,
            "123501/QDIRSRV/DEVDIR"
        );
        assert!(a.counters("QSECOFR", "DEVDIR").is_ok());
        a.end("QSECOFR", "DEVDIR").unwrap();
        assert_eq!(
            a.counters("QSECOFR", "DEVDIR").unwrap_err().kind,
            ErrorKind::Unavailable
        );
        assert_eq!(a.start("QSECOFR", "NONE").unwrap_err().msgid, "GLD0203");
        assert_eq!(
            a.properties("QSECOFR", "NONE").unwrap_err().msgid,
            "GLD0201"
        );
    }

    #[test]
    fn schema_pages_and_changes() {
        let a = admin();
        let q = SchemaQuery {
            kind: SchemaKind::Attribute,
            contains: "telephone".into(),
            ..SchemaQuery::default()
        };
        let p = a.schema("QSECOFR", "QUSRDIR", &q).unwrap();
        assert!(p.total >= 1 && p.defs.iter().any(|d| d.names[0] == "telephoneNumber"));
        a.add_schema(
            "QSECOFR",
            "QUSRDIR",
            SchemaKind::Attribute,
            "( 1.3.6.1.4.1.99999.1 NAME 'qsBadge' SYNTAX 1.3.6.1.4.1.1466.115.121.1.15 SINGLE-VALUE )",
        )
        .unwrap();
        let mine = a
            .schema(
                "QSECOFR",
                "QUSRDIR",
                &SchemaQuery {
                    custom_only: true,
                    ..SchemaQuery::default()
                },
            )
            .unwrap();
        assert_eq!(mine.total, 1);
        assert!(mine.defs[0].single_value && mine.defs[0].custom);
        assert_eq!(
            a.add_schema("QSECOFR", "QUSRDIR", SchemaKind::Attribute, "nonsense")
                .unwrap_err()
                .kind,
            ErrorKind::Invalid
        );
        a.remove_schema("QSECOFR", "QUSRDIR", SchemaKind::Attribute, "qsbadge")
            .unwrap();
        assert!(
            a.remove_schema("QSECOFR", "QUSRDIR", SchemaKind::Class, "top")
                .is_err()
        );
    }

    #[test]
    fn access_inherits_and_changes() {
        let a = admin();
        let x = a
            .access(
                "QSECOFR",
                "QUSRDIR",
                "uid=jsmith,ou=People,dc=quartz,dc=example",
            )
            .unwrap();
        assert!(!x.explicit_acl && !x.explicit_owner);
        assert_eq!(x.acl_source, "dc=quartz,dc=example");
        assert_eq!(x.owner_source, "ou=People,dc=quartz,dc=example");
        let c = a.access("QSECOFR", "QUSRDIR", "cn=localhost").unwrap();
        assert_eq!(
            (c.acl.clone(), c.acl_source.as_str()),
            (vec![DEFAULT_ACL.to_string()], "default")
        );
        let mut own = x.clone();
        own.explicit_acl = true;
        own.acl = vec!["access-id:uid=jsmith,ou=People,dc=quartz,dc=example:normal:rwsc".into()];
        a.set_access("QSECOFR", "QUSRDIR", &own).unwrap();
        let y = a
            .access(
                "QSECOFR",
                "QUSRDIR",
                "UID=JSMITH, OU=PEOPLE, DC=QUARTZ, DC=EXAMPLE",
            )
            .unwrap();
        assert!(y.explicit_acl);
        assert_eq!(y.acl, own.acl);
        own.acl = vec!["bad".into()];
        assert!(a.set_access("QSECOFR", "QUSRDIR", &own).is_err());
        assert!(a.access("QSECOFR", "QUSRDIR", "o=none").is_err());
    }

    #[test]
    fn publishing_and_configuration() {
        let a = admin();
        let mut p = a.publishing("QSECOFR").unwrap();
        assert_eq!(p.len(), 2);
        p[1].enabled = true;
        p[1].server = "ldap.example".into();
        a.set_publishing("QSECOFR", &p[1], Some("pw")).unwrap();
        assert!(a.publishing("QSECOFR").unwrap()[1].password_set);
        p[1].parent_dn = "junk".into();
        assert_eq!(
            a.set_publishing("QSECOFR", &p[1], None).unwrap_err().msgid,
            "GLD0224"
        );
        assert_eq!(a.publishing_status("QSECOFR").unwrap()[0].msgid, "GLD010B");
        let mut pr = a.properties("QSECOFR", "QUSRDIR").unwrap();
        pr.time_limit = 60;
        assert_eq!(
            a.set_properties("QSECOFR", "QUSRDIR", &pr).unwrap(),
            ["ibm-slapdTimeLimit"]
        );
        assert!(
            a.conf_text("QUSRDIR")
                .unwrap()
                .contains("ibm-slapdTimeLimit: 60")
        );
        let s = a.suffixes("QSECOFR", "QUSRDIR").unwrap();
        let user = s.iter().find(|s| s.kind == SuffixKind::User).unwrap();
        assert_eq!(user.entries, Some(5));
        a.put_index(
            "QSECOFR",
            "QUSRDIR",
            &Index {
                attr: "GIVENNAME".into(),
                equality: true,
                ..Index::default()
            },
        )
        .unwrap();
        assert_eq!(
            a.indexes("QSECOFR", "QUSRDIR")
                .unwrap()
                .last()
                .unwrap()
                .attr,
            "givenName"
        );
        a.set_admin("QSECOFR", "QUSRDIR", &AdminChange::Password, "pw")
            .unwrap();
        assert!(a.admin("QSECOFR", "QUSRDIR").unwrap().password_set);
    }
}
