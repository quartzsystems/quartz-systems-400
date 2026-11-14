// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Request 1568 `dir_admin` (docs/DIRSRV.md §4 "Administration"):
//! [`DirAdmin`] over the system, as the job's current profile.
//!
//! The configuration's questions read and write the instance's
//! `ibmslapd.conf` through `admin::conf`, and an open instance applies
//! what takes effect at once (`serve::config_changed`); the instance
//! list's autostart is edited in `idsinstances.ldif`. Start, end and
//! restart are `STRTCPSVR`/`ENDTCPSVR SERVER(*DIRSRV)`'s: a start is a
//! batch job running `STRTCPSVR` as the profile (`QSYS/QDIRJOBD`), an
//! end the instance job's `ENDJOB *CNTRLD`, a restart both with the start
//! held 15 seconds, while the old job ends. An entry's access and the
//! counters are the running instance's (`GLD0122` otherwise); the
//! schema is the running instance's, or the shipped one with the
//! instance's additions. Publishing is `publish.conf` through
//! `admin::publish`. Every refusal's text is its message's.

use std::cell::{RefCell, RefMut};

use dirsrv::admin::{
    Access, AdminChange, AdminError, AdminInfo, AgentStatus, Counters, DirAdmin, ErrorKind, Index,
    InstanceInfo, InstanceState, Properties, PublishAgent, Result, SchemaKind, SchemaPage,
    SchemaQuery, Suffix, access as acl_check, conf, publish as pubmap, schema as defs,
};
use dirsrv::config::{self, ConfigError, Instance as Listed, InstanceList, SlapdConf};
use dirsrv::ops::{Budget, Cursors, PageState, Session, Started};
use dirsrv::publish::config::{Kind, PublishConfig};
use dirsrv::schema::Schema;
use ldap::msg::{DerefAliases, ModOp, Modification, Scope, SearchRequest};
use ldap::{Attribute, Dn, Filter, LdapMessage, ProtocolOp, ResultCode};
use sec::auth::principal::Principal;
use sec::profile::SpecialAuth;
use stg::dir::DirError;
use stg::diradmin::{DirAdminOp, DirAdminReply};
use wm::table::JobInfo;

use super::instance::{Host as _, Instance, USER, load_schema, schema_path};
use super::serve::{self, NOT_ACTIVE, SystemHost};
use crate::ipl::system::{System, now_unix};
use crate::stgsvc::dirconf;

/// The batch job that runs a start.
const START_JOB: &str = "QGLDSTRSVR";
/// Its job description (`QCMDB`, `QSYSNOMAX`).
const START_JOBD: &str = "QSYS/QDIRJOBD";
/// A restart's start waits this long for the old job to end (it takes
/// up to 10 seconds).
const RESTART_DELAY: u64 = 15;

/// `GLD0166`: no such entry.
const NO_ENTRY: &str = "GLD0166";
/// `GLD0168`: an attribute type the schema lacks.
const NO_ATTR: &str = "GLD0168";
/// `GLD016E`: an own ACL or owner list with no value.
const EMPTY_LIST: &str = "GLD016E";

/// The attributes of an entry's access control.
const ACCESS_ATTRS: [&str; 6] = [
    "aclEntry",
    "aclPropagate",
    "aclSource",
    "entryOwner",
    "ownerPropagate",
    "ownerSource",
];

/// Answer `op` as `me`.
pub fn request(system: &mut System, me: &Principal, op: DirAdminOp) -> DirAdminReply {
    let reply = sec::guard::as_system(|| {
        let a = SvcAdmin {
            system: RefCell::new(&mut *system),
            me,
        };
        stg::diradmin::serve(&a, &me.user, op)
    });
    match reply {
        DirAdminReply::Refused { mut error } => {
            let data: Vec<&str> = error.data.iter().map(String::as_str).collect();
            let text = crate::stgsvc::msgs::text(system, &error.msgid, &data);
            if text != error.msgid {
                error.text = text;
            }
            DirAdminReply::Refused { error }
        }
        r => r,
    }
}

/// The service as a [`DirAdmin`] for one request.
struct SvcAdmin<'a> {
    system: RefCell<&'a mut System>,
    me: &'a Principal,
}

fn refusal(kind: ErrorKind, msgid: &str, data: Vec<String>) -> AdminError {
    AdminError {
        kind,
        msgid: msgid.into(),
        data,
        text: String::new(),
    }
}

fn cfg(e: ConfigError) -> AdminError {
    e.into()
}

fn file(path: &str, why: String) -> AdminError {
    cfg(ConfigError::File {
        file: path.into(),
        why,
    })
}

fn from_dir(e: DirError) -> AdminError {
    AdminError {
        kind: ErrorKind::Failed,
        msgid: e.msgid,
        data: e.vars,
        text: e.text,
    }
}

/// The service's `MSGID text` form of an error.
fn coded(e: &str) -> AdminError {
    let t = e.trim();
    match t.split_once(' ') {
        Some((id, rest)) if id.len() == 7 && id.chars().all(|c| c.is_ascii_alphanumeric()) => {
            refusal(ErrorKind::Failed, id, vec![rest.into()])
        }
        _ => refusal(ErrorKind::Failed, "CPF9898", vec![t.into()]),
    }
}

fn not_active(name: &str) -> AdminError {
    refusal(ErrorKind::Unavailable, NOT_ACTIVE, vec![name.into()])
}

fn qualified(j: &JobInfo) -> String {
    format!("{:06}/{}/{}", j.number, j.user.trim(), j.name.trim())
}

impl<'a> SvcAdmin<'a> {
    fn sys(&self) -> RefMut<'_, &'a mut System> {
        self.system.borrow_mut()
    }

    /// `CPF222E` naming the first of `what` the profile lacks.
    fn need(&self, what: &[(SpecialAuth, &str)]) -> Result<()> {
        match what.iter().find(|(a, _)| !self.me.has(*a)) {
            Some((_, n)) => Err(AdminError::special(n)),
            None => Ok(()),
        }
    }

    fn read(&self) -> Result<()> {
        self.need(&[(SpecialAuth::IOSYSCFG, "*IOSYSCFG")])
    }

    fn change(&self) -> Result<()> {
        self.need(&[
            (SpecialAuth::ALLOBJ, "*ALLOBJ"),
            (SpecialAuth::IOSYSCFG, "*IOSYSCFG"),
        ])
    }

    fn list(&self) -> Result<InstanceList> {
        dirconf::instances(&mut self.sys()).map_err(cfg)
    }

    /// The instance `name`: `GLD0201` when the list has none.
    fn listed(&self, name: &str) -> Result<Listed> {
        self.list()?
            .find(name)
            .cloned()
            .ok_or_else(|| cfg(ConfigError::NoInstance(name.trim().to_ascii_uppercase())))
    }

    /// As [`Self::listed`] with `GLD0203`, as the start and end have it.
    fn listed_server(&self, name: &str) -> Result<Listed> {
        self.list()?.find(name).cloned().ok_or_else(|| {
            cfg(ConfigError::NoServerInstance(
                name.trim().to_ascii_uppercase(),
            ))
        })
    }

    fn conf(&self, inst: &Listed) -> Result<SlapdConf> {
        let path = inst.conf_path();
        match dirconf::read(&mut self.sys(), &path) {
            Ok(Some(t)) => SlapdConf::parse(&path, &t).map_err(cfg),
            Ok(None) => Err(file(&path, ifs::Errno::ENOENT.text().into())),
            Err(why) => Err(file(&path, why)),
        }
    }

    /// Write `c` and have an open instance apply it.
    fn save(&self, inst: &Listed, c: &SlapdConf) -> Result<()> {
        dirconf::write(&mut self.sys(), c.path(), c.text()).map_err(|why| file(c.path(), why))?;
        serve::config_changed(&mut self.sys(), &inst.name).map_err(from_dir)
    }

    /// A change of instance `name`'s file.
    fn edit<T>(
        &self,
        name: &str,
        f: impl FnOnce(&mut SlapdConf, &Listed) -> Result<T>,
    ) -> Result<T> {
        self.change()?;
        let inst = self.listed(name)?;
        let mut c = self.conf(&inst)?;
        let t = f(&mut c, &inst)?;
        self.save(&inst, &c)?;
        Ok(t)
    }

    /// The active job of instance `name`.
    fn job_of(&self, name: &str) -> Option<JobInfo> {
        self.sys().jobs.all().into_iter().find(|j| {
            j.name.trim().eq_ignore_ascii_case(name.trim())
                && j.user.trim().eq_ignore_ascii_case(USER)
        })
    }

    /// The instance's schema: the open instance's, or the shipped one
    /// with its additions.
    fn schema_of(&self, inst: &Listed, c: &SlapdConf) -> Result<Schema> {
        if let Some(s) = serve::with_instance(&inst.name, |i| i.dir.schema.clone()) {
            return Ok(s);
        }
        let mut sys = self.sys();
        load_schema(&mut SystemHost(&mut sys), &schema_path(c, inst)).map_err(from_dir)
    }

    /// Submit `STRTCPSVR` of instance `name`, to run at `at` (0:
    /// now), as the profile.
    fn submit_start(&self, name: &str, at: u64) -> Result<()> {
        let mut sys = self.sys();
        let me = sys.current_job;
        let r = wm::obj::jobq::SbmJob {
            cmd: format!("STRTCPSVR SERVER(*DIRSRV) INSTANCE({name})"),
            job: START_JOB.into(),
            jobd: START_JOBD.into(),
            user: self.me.user.clone(),
            msgq: "*NONE".into(),
            scheduled: at,
            inllibl: Some(Vec::new()),
            curlib: "*CRTDFT".into(),
            ..wm::obj::jobq::SbmJob::default()
        };
        crate::stgsvc::jobqs::sbmjob(&mut sys, me, &r)
            .map(|_| ())
            .map_err(|e| coded(&e))
    }

    /// End instance `name`'s job, controlled; whether one was active.
    fn end_job(&self, name: &str) -> Result<bool> {
        let Some(j) = self.job_of(name) else {
            return Ok(false);
        };
        let mut sys = self.sys();
        let me = sys.current_job;
        crate::stgsvc::jobqs::endjob(&mut sys, me, &qualified(&j), false)
            .map(|_| true)
            .map_err(|e| coded(&e))
    }

    fn info(&self, inst: &Listed, certificate: &str) -> InstanceInfo {
        let p = self
            .conf(inst)
            .map(|c| conf::properties(&c))
            .unwrap_or_default();
        let job = self.job_of(&inst.name);
        let ending = job
            .as_ref()
            .is_some_and(|j| self.sys().end_requests.contains_key(&j.number));
        InstanceInfo {
            name: inst.name.clone(),
            desc: inst.desc.clone(),
            autostart: inst.autostart,
            state: match &job {
                None => InstanceState::Stopped,
                Some(_) if ending => InstanceState::Ending,
                Some(_) => InstanceState::Running,
            },
            job: job.as_ref().map(qualified).unwrap_or_default(),
            port: p.port,
            secure_port: p.secure_port,
            security: p.security,
            certificate: certificate.into(),
            version: inst.version.clone(),
            location: inst.location.clone(),
            since: job.map_or(0, |j| j.started),
        }
    }

    /// The label of the certificate assigned to the server's
    /// application ID; empty for none.
    fn certificate(&self) -> String {
        let mut sys = self.sys();
        let p = crate::stgsvc::certs::system_principal();
        crate::stgsvc::certs::cert_app(&mut sys, &p, dirsrv::admin::APP_ID)
            .ok()
            .filter(|t| t.assigned)
            .map(|t| t.label)
            .unwrap_or_default()
    }

    fn publish_config(&self) -> Result<PublishConfig> {
        super::publish::read_config(&mut self.sys())
            .map(Option::unwrap_or_default)
            .map_err(|e| coded(&e))
    }

    /// Run `f` over open instance `name` with the service as its host.
    fn open<T>(
        &self,
        name: &str,
        f: impl FnOnce(&mut Instance, &mut SystemHost<'_>) -> Result<T>,
    ) -> Result<T> {
        let mut sys = self.sys();
        serve::with_instance(name, |i| f(i, &mut SystemHost(&mut sys)))
            .unwrap_or_else(|| Err(not_active(&name.trim().to_ascii_uppercase())))
    }
}

/// The administrator's session of an open instance.
fn admin_session(inst: &Instance) -> Result<Session> {
    let a = &inst.dir.settings.admin_dn;
    let dn = Dn::parse(a).map_err(|_| cfg(ConfigError::BadDn(a.clone())))?;
    Ok(Session::admin(&inst.dir.schema, &dn))
}

fn text_of(attrs: &[Attribute], ty: &str) -> Vec<String> {
    attrs
        .iter()
        .filter(|a| a.ty.eq_ignore_ascii_case(ty))
        .flat_map(|a| a.vals.iter())
        .map(|v| String::from_utf8_lossy(v).into_owned())
        .collect()
}

fn is_true(attrs: &[Attribute], ty: &str) -> bool {
    text_of(attrs, ty)
        .first()
        .is_some_and(|v| v.eq_ignore_ascii_case("TRUE"))
}

/// Entry `dn`'s access control, as the administrator reads it.
fn read_access(inst: &mut Instance, host: &mut SystemHost<'_>, dn: &str) -> Result<Access> {
    let session = admin_session(inst)?;
    let msg = LdapMessage {
        id: 1,
        op: ProtocolOp::SearchRequest(SearchRequest {
            base: dn.trim().into(),
            scope: Scope::Base,
            deref: DerefAliases::Never,
            size_limit: 1,
            time_limit: 0,
            types_only: false,
            filter: Filter::Present("objectClass".into()),
            attrs: ACCESS_ATTRS.iter().map(ToString::to_string).collect(),
        }),
        controls: Vec::new(),
    };
    let replies = host
        .with_store(|mut t, _| {
            let mut cursors = Cursors::default();
            let out = match inst.dir.search(&mut t, &session, &msg, &mut cursors) {
                Started::Done(v) => v,
                Started::Cursor(c) => {
                    let mut all = Vec::new();
                    loop {
                        let b = Budget {
                            entries: 10,
                            bytes: 1 << 20,
                        };
                        let p = inst.dir.next(&mut t, &mut cursors, c, b);
                        all.extend(p.replies);
                        if !matches!(p.state, PageState::More) {
                            break;
                        }
                    }
                    cursors.close(c);
                    all
                }
            };
            t.abort();
            out
        })
        .ok_or_else(|| from_dir(super::instance::no_store()))?;
    let Some((found, attrs)) = replies.into_iter().find_map(|r| match r.op {
        ProtocolOp::SearchResultEntry { dn, attrs } => Some((dn, attrs)),
        _ => None,
    }) else {
        return Err(refusal(
            ErrorKind::NotFound,
            NO_ENTRY,
            vec![dn.trim().into()],
        ));
    };
    let own = |source: &[String]| {
        source
            .first()
            .is_some_and(|s| config::dn_key(s) == config::dn_key(&found))
    };
    let acl_source = text_of(&attrs, "aclSource");
    let owner_source = text_of(&attrs, "ownerSource");
    Ok(Access {
        explicit_acl: own(&acl_source),
        acl: text_of(&attrs, "aclEntry"),
        acl_propagate: is_true(&attrs, "aclPropagate"),
        acl_source: acl_source.into_iter().next().unwrap_or_default(),
        explicit_owner: own(&owner_source),
        owners: text_of(&attrs, "entryOwner"),
        owner_propagate: is_true(&attrs, "ownerPropagate"),
        owner_source: owner_source.into_iter().next().unwrap_or_default(),
        dn: found,
    })
}

/// A replace of `ty` with `vals` (none removes it).
fn replace(ty: &str, vals: &[String]) -> Modification {
    Modification {
        op: ModOp::Replace,
        attr: Attribute {
            ty: ty.into(),
            vals: vals.iter().map(|v| v.clone().into_bytes()).collect(),
        },
    }
}

fn flag(on: bool) -> Vec<String> {
    vec![if on { "TRUE" } else { "FALSE" }.into()]
}

/// Entry `a.dn`'s own ACL and owners as `a` has them, as the
/// administrator changes them; its changes reach the persistent
/// searches.
fn write_access(inst: &mut Instance, host: &mut SystemHost<'_>, a: &Access) -> Result<()> {
    let session = admin_session(inst)?;
    let (acl, acl_prop) = if a.explicit_acl {
        (a.acl.clone(), flag(a.acl_propagate))
    } else {
        (Vec::new(), Vec::new())
    };
    let (own, own_prop) = if a.explicit_owner {
        (a.owners.clone(), flag(a.owner_propagate))
    } else {
        (Vec::new(), Vec::new())
    };
    let msg = LdapMessage {
        id: 1,
        op: ProtocolOp::ModifyRequest {
            dn: a.dn.trim().into(),
            changes: vec![
                replace("aclEntry", &acl),
                replace("aclPropagate", &acl_prop),
                replace("entryOwner", &own),
                replace("ownerPropagate", &own_prop),
            ],
        },
        controls: Vec::new(),
    };
    let o = host
        .with_store(|mut t, _| {
            let o = inst.dir.op(&mut t, &session, &msg);
            if o.succeeded() {
                if let Err(e) = t.commit() {
                    eprintln!("qsstg: directory access change: {e}");
                }
            } else {
                t.abort();
            }
            o
        })
        .ok_or_else(|| from_dir(super::instance::no_store()))?;
    if !o.succeeded() {
        let r = o.replies.iter().find_map(|r| r.op.result().cloned());
        return Err(match r {
            Some(r) if r.code == ResultCode::NoSuchObject => refusal(
                ErrorKind::NotFound,
                NO_ENTRY,
                vec![a.dn.trim().into()],
            ),
            Some(r) => refusal(ErrorKind::Invalid, "CPF9898", vec![r.message]),
            None => refusal(ErrorKind::Failed, "CPF9898", Vec::new()),
        });
    }
    if !o.changes.is_empty() {
        host.with_store(|mut t, _| {
            inst.dir.publish(&mut t, &o.changes);
            t.abort();
        });
    }
    Ok(())
}

fn counters(i: &Instance) -> Counters {
    let c = &i.dir.counters;
    Counters {
        started: c.start_time / 1_000_000,
        connections: u32::try_from(c.current_connections).unwrap_or(u32::MAX),
        total_connections: c.total_connections,
        ops_initiated: c.ops_initiated,
        ops_completed: c.ops_completed,
        binds: c.binds_requested,
        searches: c.searches_requested,
        compares: c.compares_requested,
        adds: c.adds_requested,
        modifies: c.modifies_requested,
        deletes: c.deletes_requested,
        modrdns: c.modrdns_requested,
        extended: c.extops_requested,
        abandons: c.abandons_requested,
        entries_sent: c.entries_sent,
        persistent: u32::try_from(c.current_psearches).unwrap_or(u32::MAX),
    }
}

impl DirAdmin for SvcAdmin<'_> {
    fn instances(&self, _user: &str) -> Result<Vec<InstanceInfo>> {
        self.read()?;
        let list = self.list()?;
        let cert = self.certificate();
        Ok(list.instances.iter().map(|i| self.info(i, &cert)).collect())
    }

    fn set_autostart(&self, _user: &str, instance: &str, on: bool) -> Result<()> {
        self.change()?;
        let inst = self.listed(instance)?;
        let text = dirconf::read(&mut self.sys(), config::INSTANCES)
            .map_err(|why| file(config::INSTANCES, why))?
            .unwrap_or_else(|| config::shipped::INSTANCES.to_string());
        let text = config::instances::with_autostart(&text, &inst.name, on)
            .ok_or_else(|| cfg(ConfigError::NoInstance(inst.name.clone())))?;
        dirconf::write_instances(&mut self.sys(), &text)
            .map_err(|why| file(config::INSTANCES, why))
    }

    fn start(&self, _user: &str, instance: &str) -> Result<()> {
        self.need(&[(SpecialAuth::IOSYSCFG, "*IOSYSCFG")])?;
        let inst = self.listed_server(instance)?;
        self.conf(&inst)?.validate(&inst.name).map_err(cfg)?;
        if self.job_of(&inst.name).is_some() {
            return Ok(());
        }
        self.submit_start(&inst.name, 0)
    }

    fn end(&self, _user: &str, instance: &str) -> Result<()> {
        self.need(&[(SpecialAuth::IOSYSCFG, "*IOSYSCFG")])?;
        let inst = self.listed_server(instance)?;
        self.end_job(&inst.name).map(|_| ())
    }

    fn restart(&self, _user: &str, instance: &str) -> Result<()> {
        self.need(&[(SpecialAuth::IOSYSCFG, "*IOSYSCFG")])?;
        let inst = self.listed_server(instance)?;
        self.conf(&inst)?.validate(&inst.name).map_err(cfg)?;
        let at = if self.end_job(&inst.name)? {
            now_unix() + RESTART_DELAY
        } else {
            0
        };
        self.submit_start(&inst.name, at)
    }

    fn properties(&self, _user: &str, instance: &str) -> Result<Properties> {
        self.read()?;
        let inst = self.listed(instance)?;
        Ok(conf::properties(&self.conf(&inst)?))
    }

    fn set_properties(&self, _user: &str, instance: &str, to: &Properties) -> Result<Vec<String>> {
        self.edit(instance, |c, _| conf::set_properties(c, to))
    }

    fn suffixes(&self, _user: &str, instance: &str) -> Result<Vec<Suffix>> {
        self.read()?;
        let inst = self.listed(instance)?;
        Ok(conf::suffixes(&self.conf(&inst)?))
    }

    fn add_suffix(&self, _user: &str, instance: &str, dn: &str) -> Result<()> {
        self.edit(instance, |c, _| conf::add_suffix(c, dn))
    }

    fn remove_suffix(&self, _user: &str, instance: &str, dn: &str) -> Result<()> {
        self.edit(instance, |c, _| conf::remove_suffix(c, dn))
    }

    fn indexes(&self, _user: &str, instance: &str) -> Result<Vec<Index>> {
        self.read()?;
        let inst = self.listed(instance)?;
        Ok(conf::indexes(&self.conf(&inst)?))
    }

    fn put_index(&self, _user: &str, instance: &str, index: &Index) -> Result<()> {
        self.edit(instance, |c, inst| {
            let schema = self.schema_of(inst, c)?;
            let attr = index.attr.trim();
            let Some(a) = schema.attribute(attr) else {
                return Err(refusal(ErrorKind::NotFound, NO_ATTR, vec![attr.into()]));
            };
            let named = Index {
                attr: a.name().to_string(),
                ..index.clone()
            };
            conf::put_index(c, &named)
        })
    }

    fn remove_index(&self, _user: &str, instance: &str, attr: &str) -> Result<()> {
        self.edit(instance, |c, _| conf::remove_index(c, attr))
    }

    fn admin(&self, _user: &str, instance: &str) -> Result<AdminInfo> {
        self.read()?;
        let inst = self.listed(instance)?;
        Ok(conf::admin(&self.conf(&inst)?))
    }

    fn set_admin(
        &self,
        _user: &str,
        instance: &str,
        change: &AdminChange,
        password: &str,
    ) -> Result<()> {
        self.edit(instance, |c, inst| {
            conf::set_admin(c, &inst.name, change, password)
        })
    }

    fn schema(&self, _user: &str, instance: &str, q: &SchemaQuery) -> Result<SchemaPage> {
        self.read()?;
        let inst = self.listed(instance)?;
        let c = self.conf(&inst)?;
        Ok(defs::page(&self.schema_of(&inst, &c)?, q))
    }

    fn add_schema(&self, _user: &str, instance: &str, kind: SchemaKind, text: &str) -> Result<()> {
        self.change_schema(instance, |s| defs::add(s, kind, text))
    }

    fn remove_schema(&self, _user: &str, instance: &str, kind: SchemaKind, id: &str) -> Result<()> {
        self.change_schema(instance, |s| defs::remove(s, kind, id))
    }

    fn access(&self, _user: &str, instance: &str, dn: &str) -> Result<Access> {
        self.read()?;
        let inst = self.listed(instance)?;
        self.open(&inst.name, |i, h| read_access(i, h, dn))
    }

    fn set_access(&self, _user: &str, instance: &str, a: &Access) -> Result<()> {
        self.change()?;
        if (a.explicit_acl && a.acl.is_empty()) || (a.explicit_owner && a.owners.is_empty()) {
            return Err(refusal(ErrorKind::Invalid, EMPTY_LIST, Vec::new()));
        }
        if a.explicit_acl {
            a.acl.iter().try_for_each(|v| acl_check::check_acl(v))?;
        }
        if a.explicit_owner {
            a.owners.iter().try_for_each(|v| acl_check::check_owner(v))?;
        }
        let inst = self.listed(instance)?;
        self.open(&inst.name, |i, h| write_access(i, h, a))
    }

    fn publishing(&self, _user: &str) -> Result<Vec<PublishAgent>> {
        self.read()?;
        Ok(pubmap::agents(&self.publish_config()?))
    }

    fn set_publishing(
        &self,
        _user: &str,
        agent: &PublishAgent,
        password: Option<&str>,
    ) -> Result<()> {
        self.change()?;
        let mut c = self.publish_config()?;
        pubmap::apply(&mut c, agent)?;
        let kind = pubmap::kind(agent.kind);
        // An anonymous bind keeps no password.
        let passwords: Vec<(Kind, Option<String>)> = match password {
            Some(p) if c.agent(kind).bind_dn.is_some() => {
                vec![(kind, Some(p.to_string()).filter(|p| !p.is_empty()))]
            }
            _ => Vec::new(),
        };
        super::publish::write_config(&mut self.sys(), c, &passwords).map_err(|e| coded(&e))
    }

    fn publishing_status(&self, _user: &str) -> Result<Vec<AgentStatus>> {
        self.read()?;
        let c = self.publish_config()?;
        Ok(Kind::ALL
            .into_iter()
            .map(|kind| {
                let job = self.job_of(kind.job());
                let s = super::publish::status(kind).map(|(_, s)| s);
                let (msgid, text) = match s.as_ref().and_then(|s| s.last_error.clone()) {
                    Some((_, e)) => ("CPF9898".to_string(), e),
                    None => (String::new(), String::new()),
                };
                AgentStatus {
                    kind: pubmap::agent_kind(kind),
                    active: job.is_some() && c.agent(kind).enabled,
                    job: job.as_ref().map(qualified).unwrap_or_default(),
                    published: s.as_ref().map_or(0, |s| u64::from(s.published)),
                    failed: s.as_ref().map_or(0, |s| u64::from(s.errors)),
                    last: s.as_ref().and_then(|s| s.last_success).unwrap_or(0),
                    msgid,
                    text,
                }
            })
            .collect())
    }

    fn counters(&self, _user: &str, instance: &str) -> Result<Counters> {
        self.read()?;
        let inst = self.listed(instance)?;
        self.open(&inst.name, |i, _| Ok(counters(i)))
    }
}

impl SvcAdmin<'_> {
    /// Change instance `name`'s schema with `f` and write its additions:
    /// the open instance's schema, or the shipped one with them.
    fn change_schema(
        &self,
        name: &str,
        f: impl FnOnce(&mut Schema) -> Result<()>,
    ) -> Result<()> {
        self.change()?;
        let inst = self.listed(name)?;
        let c = self.conf(&inst)?;
        let path = schema_path(&c, &inst);
        let mut f = Some(f);
        let open = {
            let mut sys = self.sys();
            serve::with_instance(&inst.name, |i| -> Result<()> {
                (f.take().expect("once"))(&mut i.dir.schema)?;
                let text = i.dir.schema.modified_text();
                SystemHost(&mut sys)
                    .write(&i.schema_path, &text)
                    .map_err(|why| file(&i.schema_path, why))
            })
        };
        if let Some(r) = open {
            return r;
        }
        let mut s = self.schema_of(&inst, &c)?;
        (f.take().expect("once"))(&mut s)?;
        SystemHost(&mut self.sys())
            .write(&path, &s.modified_text())
            .map_err(|why| file(&path, why))
    }
}
