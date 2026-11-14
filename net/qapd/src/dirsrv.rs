// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `Dirsrv` service (version 12, draft; docs/QAP.md "Service 14
//! `Dirsrv`") on a secondary thread: each request passes the exit
//! point `QS400_QAP_MGMT` (server `*QAPDIRSRV`), then runs as the
//! signed-on profile through the directory server's administration
//! interface (`dirsrv::admin::DirAdmin`, the storage service's
//! `dir_admin`), which checks the authority, validates and words the
//! refusal. The host gives the interface ([`ThreadHost::dirsrv`]).

use dirsrv::admin as a;
use qap_proto::ops::dirsrv::{
    AddDirSchema, AdminTarget, AgentKind, AgentStatus, Changed, DirAdmin, DirCounters, DirIndex,
    DirIndexRef, DirIndexes, DirProperties, DnRef, EntryAccess, Instance, InstanceRef,
    InstanceState, Instances, ListDirSchema, PublishAgent, Publishing, PublishingStatus,
    PutDirIndex, SchemaDefinition, SchemaDefinitions, SchemaKind, SchemaRef, SetDirAccess,
    SetDirAdmin, SetDirAutostart, SetDirProperties, SetPublishing, Suffix, SuffixKind, Suffixes,
};
use qap_proto::types::JobId;
use qap_proto::{Envelope, Op, ServerMessage, Status};

use crate::conn::Answer;
use crate::host::{Refusal, ThreadHost, no_database};
use crate::manage::{EXIT_FORMAT, EXIT_POINT, ExitRequest, NO_MANAGE};

/// The server field of this service's exit requests.
pub const SERVER: &str = "*QAPDIRSRV";

/// The status a refusal of the interface stands for.
#[must_use]
pub fn status_of(k: a::ErrorKind) -> Status {
    match k {
        a::ErrorKind::NotAuthorized => Status::NotAuthorized,
        a::ErrorKind::NotFound => Status::NotFound,
        a::ErrorKind::Exists => Status::Exists,
        a::ErrorKind::Invalid | a::ErrorKind::Unavailable | a::ErrorKind::Failed => Status::Error,
    }
}

/// A refusal of the interface, its message as the job sends it.
fn refusal(host: &mut dyn ThreadHost, e: &a::AdminError) -> Refusal {
    let data: Vec<&str> = e.data.iter().map(String::as_str).collect();
    let mut message = host.message(&e.msgid, &data);
    if message.text.is_empty() {
        message = ServerMessage {
            id: e.msgid.clone(),
            severity: 40,
            text: e.text.clone(),
            ..message
        };
    }
    Refusal {
        status: status_of(e.kind),
        message,
    }
}

fn ms(secs: u64) -> i64 {
    i64::try_from(secs).unwrap_or(i64::MAX).saturating_mul(1000)
}

fn job_id(q: &str) -> Option<JobId> {
    let mut p = q.splitn(3, '/');
    let (number, user, name) = (p.next()?, p.next()?, p.next()?);
    Some(JobId {
        name: name.into(),
        user: user.into(),
        number: number.into(),
    })
}

/// An instance as the protocol carries it.
#[must_use]
pub fn instance_out(i: &a::InstanceInfo) -> Instance {
    Instance {
        name: i.name.clone(),
        description: i.desc.clone(),
        autostart: i.autostart,
        state: match i.state {
            a::InstanceState::Stopped => InstanceState::Stopped,
            a::InstanceState::Starting => InstanceState::Starting,
            a::InstanceState::Running => InstanceState::Running,
            a::InstanceState::Ending => InstanceState::Ending,
        },
        job: job_id(&i.job),
        port: i.port,
        secure_port: i.secure_port,
        security: i.security.clone(),
        certificate: (!i.certificate.is_empty()).then(|| i.certificate.clone()),
        version: i.version.clone(),
        since: ms(i.since),
    }
}

/// The properties as the protocol carries them.
#[must_use]
pub fn properties_out(p: &a::Properties) -> DirProperties {
    DirProperties {
        port: p.port,
        secure_port: p.secure_port,
        security: p.security.clone(),
        size_limit: p.size_limit,
        time_limit: p.time_limit,
        idle_timeout: p.idle_timeout,
        write_timeout: p.write_timeout,
        allow_anonymous: p.allow_anon,
        deref_aliases: p.deref_aliases.clone(),
        schema_check: p.schema_check.clone(),
        password_encryption: p.pw_encryption.clone(),
        read_only: p.read_only,
        log_options: p.log_options.clone(),
        audit: p.audit,
        persistent_search: p.persistent_search,
        max_persistent_searches: p.max_persistent,
        paged_for_all: p.paged_non_admin,
        paged_limit: p.paged_limit,
        sorted_for_all: p.sort_non_admin,
        sort_key_limit: p.sort_key_limit,
        changelog_max: p.changelog_max,
        password_policy: p.pwd_policy,
        profiles_read_only: p.projected_read_only,
        referrals: p.referrals.clone(),
    }
}

/// The properties of a request, for the interface.
#[must_use]
pub fn properties_in(p: &DirProperties) -> a::Properties {
    a::Properties {
        port: p.port,
        secure_port: p.secure_port,
        security: p.security.clone(),
        size_limit: p.size_limit,
        time_limit: p.time_limit,
        idle_timeout: p.idle_timeout,
        write_timeout: p.write_timeout,
        allow_anon: p.allow_anonymous,
        deref_aliases: p.deref_aliases.clone(),
        schema_check: p.schema_check.clone(),
        pw_encryption: p.password_encryption.clone(),
        read_only: p.read_only,
        log_options: p.log_options.clone(),
        audit: p.audit,
        persistent_search: p.persistent_search,
        max_persistent: p.max_persistent_searches,
        paged_non_admin: p.paged_for_all,
        paged_limit: p.paged_limit,
        sort_non_admin: p.sorted_for_all,
        sort_key_limit: p.sort_key_limit,
        changelog_max: p.changelog_max,
        pwd_policy: p.password_policy,
        projected_read_only: p.profiles_read_only,
        referrals: p.referrals.clone(),
    }
}

fn suffix_out(s: a::Suffix) -> Suffix {
    Suffix {
        dn: s.dn,
        kind: match s.kind {
            a::SuffixKind::User => SuffixKind::User,
            a::SuffixKind::Local => SuffixKind::Local,
            a::SuffixKind::Projected => SuffixKind::Projected,
            a::SuffixKind::Schema => SuffixKind::Schema,
            a::SuffixKind::Config => SuffixKind::Config,
        },
        entries: s.entries,
    }
}

fn index_out(i: a::Index) -> DirIndex {
    DirIndex {
        attribute: i.attr,
        presence: i.presence,
        equality: i.equality,
        substring: i.substring,
        always: i.always,
    }
}

fn kind_in(k: SchemaKind) -> a::SchemaKind {
    match k {
        SchemaKind::AttributeType => a::SchemaKind::Attribute,
        SchemaKind::ObjectClass => a::SchemaKind::Class,
    }
}

fn def_out(d: a::SchemaDef) -> SchemaDefinition {
    SchemaDefinition {
        kind: match d.kind {
            a::SchemaKind::Attribute => SchemaKind::AttributeType,
            a::SchemaKind::Class => SchemaKind::ObjectClass,
        },
        oid: d.oid,
        names: d.names,
        description: d.desc,
        superior: d.sup,
        syntax: d.syntax,
        single_value: d.single_value,
        access_class: d.access,
        class_kind: d.class_kind,
        must: d.must,
        may: d.may,
        added: d.custom,
        protected: d.protected,
        text: d.text,
    }
}

fn access_out(x: a::Access) -> EntryAccess {
    EntryAccess {
        dn: x.dn,
        own_acl: x.explicit_acl,
        acl: x.acl,
        acl_propagate: x.acl_propagate,
        acl_source: x.acl_source,
        own_owners: x.explicit_owner,
        owners: x.owners,
        owner_propagate: x.owner_propagate,
        owner_source: x.owner_source,
    }
}

fn access_in(x: &EntryAccess) -> a::Access {
    a::Access {
        dn: x.dn.clone(),
        explicit_acl: x.own_acl,
        acl: x.acl.clone(),
        acl_propagate: x.acl_propagate,
        acl_source: String::new(),
        explicit_owner: x.own_owners,
        owners: x.owners.clone(),
        owner_propagate: x.owner_propagate,
        owner_source: String::new(),
    }
}

fn agent_kind_out(k: a::AgentKind) -> AgentKind {
    match k {
        a::AgentKind::Users => AgentKind::Users,
        a::AgentKind::Printers => AgentKind::Printers,
    }
}

fn agent_out(x: a::PublishAgent) -> PublishAgent {
    PublishAgent {
        kind: agent_kind_out(x.kind),
        enabled: x.enabled,
        server: x.server,
        port: x.port,
        tls: x.tls,
        bind_dn: x.bind_dn,
        password_set: x.password_set,
        parent_dn: x.parent_dn,
    }
}

fn agent_in(x: &PublishAgent) -> a::PublishAgent {
    a::PublishAgent {
        kind: match x.kind {
            AgentKind::Users => a::AgentKind::Users,
            AgentKind::Printers => a::AgentKind::Printers,
        },
        enabled: x.enabled,
        server: x.server.clone(),
        port: x.port,
        tls: x.tls,
        bind_dn: x.bind_dn.clone(),
        password_set: false,
        parent_dn: x.parent_dn.clone(),
    }
}

fn status_out(s: a::AgentStatus) -> AgentStatus {
    AgentStatus {
        kind: agent_kind_out(s.kind),
        job: job_id(&s.job),
        active: s.active,
        published: s.published,
        failed: s.failed,
        last: ms(s.last),
        message_id: (!s.msgid.is_empty()).then_some(s.msgid),
        message_text: s.text,
    }
}

fn counters_out(c: &a::Counters) -> DirCounters {
    DirCounters {
        started: ms(c.started),
        connections: c.connections,
        total_connections: c.total_connections,
        operations_started: c.ops_initiated,
        operations_completed: c.ops_completed,
        binds: c.binds,
        searches: c.searches,
        compares: c.compares,
        adds: c.adds,
        modifies: c.modifies,
        deletes: c.deletes,
        modify_dns: c.modrdns,
        extended: c.extended,
        abandons: c.abandons,
        entries_sent: c.entries_sent,
        persistent_searches: c.persistent,
    }
}

/// The exit request for `op` with body `req`; `None` when the body
/// does not decode.
fn exit_request(user: &str, op: Op, req: &Envelope) -> Option<ExitRequest> {
    let inst = |r: &Envelope| r.body_as::<InstanceRef>().ok().map(|b| b.instance);
    let (target, detail) = match op {
        Op::ListDirInstances | Op::GetPublishing | Op::GetPublishingStatus => {
            ("*ALL".into(), String::new())
        }
        Op::StartDirInstance
        | Op::EndDirInstance
        | Op::RestartDirInstance
        | Op::GetDirProperties
        | Op::ListDirSuffixes
        | Op::ListDirIndexes
        | Op::GetDirAdmin
        | Op::GetDirCounters => (inst(req)?, String::new()),
        Op::SetDirAutostart => {
            let b = req.body_as::<SetDirAutostart>().ok()?;
            (b.instance, format!("AUTOSTART({})", u8::from(b.autostart)))
        }
        Op::SetDirProperties => (
            req.body_as::<SetDirProperties>().ok()?.instance,
            String::new(),
        ),
        Op::AddDirSuffix | Op::RemoveDirSuffix | Op::GetDirAccess => {
            let b = req.body_as::<DnRef>().ok()?;
            (b.instance, b.dn)
        }
        Op::PutDirIndex => {
            let b = req.body_as::<PutDirIndex>().ok()?;
            (b.instance, b.index.attribute)
        }
        Op::RemoveDirIndex => {
            let b = req.body_as::<DirIndexRef>().ok()?;
            (b.instance, b.attribute)
        }
        Op::SetDirAdmin => {
            let b = req.body_as::<SetDirAdmin>().ok()?;
            let d = match b.target {
                AdminTarget::Password => "*ADMINDN".to_string(),
                AdminTarget::NewDn => b.dn,
                AdminTarget::UpdatePassword => "*UPDATEDN".into(),
            };
            (b.instance, d)
        }
        Op::ListDirSchema => {
            let b = req.body_as::<ListDirSchema>().ok()?;
            (b.instance, b.contains)
        }
        Op::AddDirSchema => {
            let b = req.body_as::<AddDirSchema>().ok()?;
            (b.instance, b.description)
        }
        Op::RemoveDirSchema => {
            let b = req.body_as::<SchemaRef>().ok()?;
            (b.instance, b.id)
        }
        Op::SetDirAccess => {
            let b = req.body_as::<SetDirAccess>().ok()?;
            (b.instance, b.access.dn)
        }
        Op::SetPublishing => {
            let b = req.body_as::<SetPublishing>().ok()?;
            (
                format!("{:?}", b.agent.kind).to_ascii_uppercase(),
                b.agent.parent_dn,
            )
        }
        _ => return None,
    };
    let (_, code) = op.code();
    Some(ExitRequest {
        user: user.to_string(),
        server: SERVER.to_string(),
        op: code,
        name: format!("{op:?}"),
        target,
        detail,
    })
}

/// Answer one `Dirsrv` operation as `user`, on a secondary thread.
#[allow(clippy::too_many_lines, clippy::many_single_char_names)]
pub(crate) fn serve(host: &mut dyn ThreadHost, user: &str, op: Op, req: &Envelope) -> Answer {
    let Some(exit) = exit_request(user, op, req) else {
        return Answer::status(Status::InvalidRequest);
    };
    if let Err(r) = host.exit_allows(EXIT_POINT, EXIT_FORMAT, &exit) {
        return Answer::refused(r);
    }
    let Some(d) = host.dirsrv() else {
        let m = host.message(NO_MANAGE, &[]);
        return Answer::refused(no_database(m));
    };
    // The answer, or the refusal turned into a message on the host.
    let r: Result<Answer, a::AdminError> = (|| {
        Ok(match op {
            Op::ListDirInstances => Answer::ok(&Instances {
                instances: d.instances(user)?.iter().map(instance_out).collect(),
            }),
            Op::SetDirAutostart => match req.body_as::<SetDirAutostart>() {
                Ok(b) => {
                    d.set_autostart(user, &b.instance, b.autostart)?;
                    Answer::status(Status::Ok)
                }
                Err(_) => Answer::status(Status::InvalidRequest),
            },
            Op::StartDirInstance | Op::EndDirInstance | Op::RestartDirInstance => {
                let Ok(b) = req.body_as::<InstanceRef>() else {
                    return Ok(Answer::status(Status::InvalidRequest));
                };
                match op {
                    Op::StartDirInstance => d.start(user, &b.instance)?,
                    Op::EndDirInstance => d.end(user, &b.instance)?,
                    _ => d.restart(user, &b.instance)?,
                }
                Answer::status(Status::Ok)
            }
            Op::GetDirProperties
            | Op::ListDirSuffixes
            | Op::ListDirIndexes
            | Op::GetDirAdmin
            | Op::GetDirCounters => {
                let Ok(b) = req.body_as::<InstanceRef>() else {
                    return Ok(Answer::status(Status::InvalidRequest));
                };
                let i = &b.instance;
                match op {
                    Op::GetDirProperties => Answer::ok(&properties_out(&d.properties(user, i)?)),
                    Op::ListDirSuffixes => Answer::ok(&Suffixes {
                        suffixes: d.suffixes(user, i)?.into_iter().map(suffix_out).collect(),
                    }),
                    Op::ListDirIndexes => Answer::ok(&DirIndexes {
                        indexes: d.indexes(user, i)?.into_iter().map(index_out).collect(),
                    }),
                    Op::GetDirAdmin => {
                        let x = d.admin(user, i)?;
                        Answer::ok(&DirAdmin {
                            dn: x.dn,
                            password_set: x.password_set,
                            update_dn: x.update_dn,
                        })
                    }
                    _ => Answer::ok(&counters_out(&d.counters(user, i)?)),
                }
            }
            Op::SetDirProperties => match req.body_as::<SetDirProperties>() {
                Ok(b) => Answer::ok(&Changed {
                    attributes: d.set_properties(
                        user,
                        &b.instance,
                        &properties_in(&b.properties),
                    )?,
                }),
                Err(_) => Answer::status(Status::InvalidRequest),
            },
            Op::AddDirSuffix | Op::RemoveDirSuffix | Op::GetDirAccess => {
                let Ok(b) = req.body_as::<DnRef>() else {
                    return Ok(Answer::status(Status::InvalidRequest));
                };
                match op {
                    Op::AddDirSuffix => d.add_suffix(user, &b.instance, &b.dn)?,
                    Op::RemoveDirSuffix => d.remove_suffix(user, &b.instance, &b.dn)?,
                    _ => {
                        return Ok(Answer::ok(&access_out(d.access(
                            user,
                            &b.instance,
                            &b.dn,
                        )?)));
                    }
                }
                Answer::status(Status::Ok)
            }
            Op::PutDirIndex => match req.body_as::<PutDirIndex>() {
                Ok(b) => {
                    let x = a::Index {
                        attr: b.index.attribute,
                        presence: b.index.presence,
                        equality: b.index.equality,
                        substring: b.index.substring,
                        always: false,
                    };
                    d.put_index(user, &b.instance, &x)?;
                    Answer::status(Status::Ok)
                }
                Err(_) => Answer::status(Status::InvalidRequest),
            },
            Op::RemoveDirIndex => match req.body_as::<DirIndexRef>() {
                Ok(b) => {
                    d.remove_index(user, &b.instance, &b.attribute)?;
                    Answer::status(Status::Ok)
                }
                Err(_) => Answer::status(Status::InvalidRequest),
            },
            Op::SetDirAdmin => match req.body_as::<SetDirAdmin>() {
                Ok(b) => {
                    let change = match b.target {
                        AdminTarget::Password => a::AdminChange::Password,
                        AdminTarget::NewDn => a::AdminChange::Dn { dn: b.dn },
                        AdminTarget::UpdatePassword => a::AdminChange::UpdatePassword,
                    };
                    d.set_admin(user, &b.instance, &change, &b.password)?;
                    Answer::status(Status::Ok)
                }
                Err(_) => Answer::status(Status::InvalidRequest),
            },
            Op::ListDirSchema => match req.body_as::<ListDirSchema>() {
                Ok(b) => {
                    let p = d.schema(
                        user,
                        &b.instance,
                        &a::SchemaQuery {
                            kind: kind_in(b.kind),
                            contains: b.contains,
                            custom_only: b.added_only,
                            start: b.start,
                            max: b.max,
                        },
                    )?;
                    Answer::ok(&SchemaDefinitions {
                        total: p.total,
                        definitions: p.defs.into_iter().map(def_out).collect(),
                    })
                }
                Err(_) => Answer::status(Status::InvalidRequest),
            },
            Op::AddDirSchema => match req.body_as::<AddDirSchema>() {
                Ok(b) => {
                    d.add_schema(user, &b.instance, kind_in(b.kind), &b.description)?;
                    Answer::status(Status::Ok)
                }
                Err(_) => Answer::status(Status::InvalidRequest),
            },
            Op::RemoveDirSchema => match req.body_as::<SchemaRef>() {
                Ok(b) => {
                    d.remove_schema(user, &b.instance, kind_in(b.kind), &b.id)?;
                    Answer::status(Status::Ok)
                }
                Err(_) => Answer::status(Status::InvalidRequest),
            },
            Op::SetDirAccess => match req.body_as::<SetDirAccess>() {
                Ok(b) => {
                    d.set_access(user, &b.instance, &access_in(&b.access))?;
                    Answer::status(Status::Ok)
                }
                Err(_) => Answer::status(Status::InvalidRequest),
            },
            Op::GetPublishing => Answer::ok(&Publishing {
                agents: d.publishing(user)?.into_iter().map(agent_out).collect(),
            }),
            Op::SetPublishing => match req.body_as::<SetPublishing>() {
                Ok(b) => {
                    d.set_publishing(user, &agent_in(&b.agent), b.password.as_deref())?;
                    Answer::status(Status::Ok)
                }
                Err(_) => Answer::status(Status::InvalidRequest),
            },
            Op::GetPublishingStatus => Answer::ok(&PublishingStatus {
                agents: d
                    .publishing_status(user)?
                    .into_iter()
                    .map(status_out)
                    .collect(),
            }),
            _ => Answer::status(Status::InvalidRequest),
        })
    })();
    match r {
        Ok(a) => a,
        Err(e) => {
            let r = refusal(host, &e);
            Answer::refused(r)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn properties_cross_both_ways() {
        let p = a::Properties {
            size_limit: 7,
            referrals: vec!["ldap://x".into()],
            ..a::Properties::default()
        };
        assert_eq!(properties_in(&properties_out(&p)), p);
        assert_eq!(status_of(a::ErrorKind::Exists), Status::Exists);
        assert_eq!(
            job_id("123470/QDIRSRV/QUSRDIR").map(|j| j.name),
            Some("QUSRDIR".into())
        );
        assert_eq!(job_id(""), None);
    }
}
