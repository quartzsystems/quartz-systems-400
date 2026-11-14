// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The directory server's administration over the link (docs/DIRSRV.md
//! §4, request 1568 `dir_admin`): one [`DirAdminOp`] answered by a
//! [`DirAdminReply`], each a method of `dirsrv::admin::DirAdmin`. The
//! job sets the signed-on profile before it asks, so no request names
//! a user; the service answers as the job's current user.
//!
//! [`Remote`] is a `DirAdmin` that sends each call through a [`Sender`]
//! (the job's side); [`serve`] answers one request from a `DirAdmin`
//! (the service's side) as request 1568 `dir_admin` of [`crate::proto`].
//! A sender on the job's main thread is `Send + Sync` (Navigator's);
//! one over a thread's own link need not be (QAP's).

use dirsrv::admin::{
    Access, AdminChange, AdminError, AdminInfo, AgentStatus, Counters, DirAdmin, ErrorKind, Index,
    InstanceInfo, Properties, PublishAgent, Result, SchemaKind, SchemaPage, SchemaQuery, Suffix,
};
use ipc::wire_enum;

/// One request; the fields are the method's.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum DirAdminOp {
    Instances,
    SetAutostart {
        instance: String,
        on: bool,
    },
    Start {
        instance: String,
    },
    End {
        instance: String,
    },
    Restart {
        instance: String,
    },
    Properties {
        instance: String,
    },
    SetProperties {
        instance: String,
        to: Properties,
    },
    Suffixes {
        instance: String,
    },
    AddSuffix {
        instance: String,
        dn: String,
    },
    RemoveSuffix {
        instance: String,
        dn: String,
    },
    Indexes {
        instance: String,
    },
    PutIndex {
        instance: String,
        index: Index,
    },
    RemoveIndex {
        instance: String,
        attr: String,
    },
    Admin {
        instance: String,
    },
    SetAdmin {
        instance: String,
        change: AdminChange,
        password: String,
    },
    Schema {
        instance: String,
        query: SchemaQuery,
    },
    AddSchema {
        instance: String,
        kind: SchemaKind,
        text: String,
    },
    RemoveSchema {
        instance: String,
        kind: SchemaKind,
        id: String,
    },
    Access {
        instance: String,
        dn: String,
    },
    SetAccess {
        instance: String,
        access: Access,
    },
    Publishing,
    SetPublishing {
        agent: PublishAgent,
        password: Option<String>,
    },
    PublishingStatus,
    Counters {
        instance: String,
    },
}

wire_enum!(DirAdminOp {
    0 => Instances,
    1 => SetAutostart { instance, on },
    2 => Start { instance },
    3 => End { instance },
    4 => Restart { instance },
    5 => Properties { instance },
    6 => SetProperties { instance, to },
    7 => Suffixes { instance },
    8 => AddSuffix { instance, dn },
    9 => RemoveSuffix { instance, dn },
    10 => Indexes { instance },
    11 => PutIndex { instance, index },
    12 => RemoveIndex { instance, attr },
    13 => Admin { instance },
    14 => SetAdmin { instance, change, password },
    15 => Schema { instance, query },
    16 => AddSchema { instance, kind, text },
    17 => RemoveSchema { instance, kind, id },
    18 => Access { instance, dn },
    19 => SetAccess { instance, access },
    20 => Publishing,
    21 => SetPublishing { agent, password },
    22 => PublishingStatus,
    23 => Counters { instance },
});

/// The answer to a [`DirAdminOp`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum DirAdminReply {
    /// Done.
    Done,
    /// Refused.
    Refused {
        error: AdminError,
    },
    Instances {
        list: Vec<InstanceInfo>,
    },
    Properties {
        properties: Properties,
    },
    /// The attributes written.
    Changed {
        attrs: Vec<String>,
    },
    Suffixes {
        list: Vec<Suffix>,
    },
    Indexes {
        list: Vec<Index>,
    },
    Admin {
        info: AdminInfo,
    },
    Schema {
        page: SchemaPage,
    },
    Access {
        access: Access,
    },
    Publishing {
        agents: Vec<PublishAgent>,
    },
    PublishingStatus {
        list: Vec<AgentStatus>,
    },
    Counters {
        counters: Counters,
    },
}

wire_enum!(DirAdminReply {
    0 => Done,
    1 => Refused { error },
    2 => Instances { list },
    3 => Properties { properties },
    4 => Changed { attrs },
    5 => Suffixes { list },
    6 => Indexes { list },
    7 => Admin { info },
    8 => Schema { page },
    9 => Access { access },
    10 => Publishing { agents },
    11 => PublishingStatus { list },
    12 => Counters { counters },
});

/// Answer `op` from `a` as `user` (the job's current profile).
pub fn serve(a: &dyn DirAdmin, user: &str, op: DirAdminOp) -> DirAdminReply {
    use DirAdminOp as O;
    use DirAdminReply as R;
    fn done(r: Result<()>) -> DirAdminReply {
        r.map_or_else(|error| R::Refused { error }, |()| R::Done)
    }
    fn or<T>(r: Result<T>, f: impl FnOnce(T) -> DirAdminReply) -> DirAdminReply {
        r.map_or_else(|error| R::Refused { error }, f)
    }
    match op {
        O::Instances => or(a.instances(user), |list| R::Instances { list }),
        O::SetAutostart { instance, on } => done(a.set_autostart(user, &instance, on)),
        O::Start { instance } => done(a.start(user, &instance)),
        O::End { instance } => done(a.end(user, &instance)),
        O::Restart { instance } => done(a.restart(user, &instance)),
        O::Properties { instance } => or(a.properties(user, &instance), |properties| {
            R::Properties { properties }
        }),
        O::SetProperties { instance, to } => or(a.set_properties(user, &instance, &to), |attrs| {
            R::Changed { attrs }
        }),
        O::Suffixes { instance } => or(a.suffixes(user, &instance), |list| R::Suffixes { list }),
        O::AddSuffix { instance, dn } => done(a.add_suffix(user, &instance, &dn)),
        O::RemoveSuffix { instance, dn } => done(a.remove_suffix(user, &instance, &dn)),
        O::Indexes { instance } => or(a.indexes(user, &instance), |list| R::Indexes { list }),
        O::PutIndex { instance, index } => done(a.put_index(user, &instance, &index)),
        O::RemoveIndex { instance, attr } => done(a.remove_index(user, &instance, &attr)),
        O::Admin { instance } => or(a.admin(user, &instance), |info| R::Admin { info }),
        O::SetAdmin {
            instance,
            change,
            password,
        } => done(a.set_admin(user, &instance, &change, &password)),
        O::Schema { instance, query } => {
            or(a.schema(user, &instance, &query), |page| R::Schema { page })
        }
        O::AddSchema {
            instance,
            kind,
            text,
        } => done(a.add_schema(user, &instance, kind, &text)),
        O::RemoveSchema { instance, kind, id } => done(a.remove_schema(user, &instance, kind, &id)),
        O::Access { instance, dn } => or(a.access(user, &instance, &dn), |access| R::Access {
            access,
        }),
        O::SetAccess { instance, access } => done(a.set_access(user, &instance, &access)),
        O::Publishing => or(a.publishing(user), |agents| R::Publishing { agents }),
        O::SetPublishing { agent, password } => {
            done(a.set_publishing(user, &agent, password.as_deref()))
        }
        O::PublishingStatus => or(a.publishing_status(user), |list| R::PublishingStatus {
            list,
        }),
        O::Counters { instance } => or(a.counters(user, &instance), |counters| R::Counters {
            counters,
        }),
    }
}

/// What carries a request to the service as `user` and brings back its
/// answer.
pub trait Sender {
    /// Send `op` as `user`.
    fn send(&self, user: &str, op: DirAdminOp) -> DirAdminReply;
}

/// A `DirAdmin` whose every call is a request through `S`.
pub struct Remote<S>(pub S);

fn unexpected() -> AdminError {
    AdminError::plain(
        ErrorKind::Failed,
        "The storage service gave an answer of another kind.",
    )
}

macro_rules! ask {
    ($self:ident, $user:ident, $op:expr, $pat:pat => $val:expr) => {
        match $self.0.send($user, $op) {
            $pat => Ok($val),
            DirAdminReply::Refused { error } => Err(error),
            _ => Err(unexpected()),
        }
    };
}

impl<S: Sender> DirAdmin for Remote<S> {
    fn instances(&self, user: &str) -> Result<Vec<InstanceInfo>> {
        ask!(self, user, DirAdminOp::Instances, DirAdminReply::Instances { list } => list)
    }

    fn set_autostart(&self, user: &str, instance: &str, on: bool) -> Result<()> {
        let op = DirAdminOp::SetAutostart {
            instance: instance.into(),
            on,
        };
        ask!(self, user, op, DirAdminReply::Done => ())
    }

    fn start(&self, user: &str, instance: &str) -> Result<()> {
        let op = DirAdminOp::Start {
            instance: instance.into(),
        };
        ask!(self, user, op, DirAdminReply::Done => ())
    }

    fn end(&self, user: &str, instance: &str) -> Result<()> {
        let op = DirAdminOp::End {
            instance: instance.into(),
        };
        ask!(self, user, op, DirAdminReply::Done => ())
    }

    fn restart(&self, user: &str, instance: &str) -> Result<()> {
        let op = DirAdminOp::Restart {
            instance: instance.into(),
        };
        ask!(self, user, op, DirAdminReply::Done => ())
    }

    fn properties(&self, user: &str, instance: &str) -> Result<Properties> {
        let op = DirAdminOp::Properties {
            instance: instance.into(),
        };
        ask!(self, user, op, DirAdminReply::Properties { properties } => properties)
    }

    fn set_properties(&self, user: &str, instance: &str, to: &Properties) -> Result<Vec<String>> {
        let op = DirAdminOp::SetProperties {
            instance: instance.into(),
            to: to.clone(),
        };
        ask!(self, user, op, DirAdminReply::Changed { attrs } => attrs)
    }

    fn suffixes(&self, user: &str, instance: &str) -> Result<Vec<Suffix>> {
        let op = DirAdminOp::Suffixes {
            instance: instance.into(),
        };
        ask!(self, user, op, DirAdminReply::Suffixes { list } => list)
    }

    fn add_suffix(&self, user: &str, instance: &str, dn: &str) -> Result<()> {
        let op = DirAdminOp::AddSuffix {
            instance: instance.into(),
            dn: dn.into(),
        };
        ask!(self, user, op, DirAdminReply::Done => ())
    }

    fn remove_suffix(&self, user: &str, instance: &str, dn: &str) -> Result<()> {
        let op = DirAdminOp::RemoveSuffix {
            instance: instance.into(),
            dn: dn.into(),
        };
        ask!(self, user, op, DirAdminReply::Done => ())
    }

    fn indexes(&self, user: &str, instance: &str) -> Result<Vec<Index>> {
        let op = DirAdminOp::Indexes {
            instance: instance.into(),
        };
        ask!(self, user, op, DirAdminReply::Indexes { list } => list)
    }

    fn put_index(&self, user: &str, instance: &str, index: &Index) -> Result<()> {
        let op = DirAdminOp::PutIndex {
            instance: instance.into(),
            index: index.clone(),
        };
        ask!(self, user, op, DirAdminReply::Done => ())
    }

    fn remove_index(&self, user: &str, instance: &str, attr: &str) -> Result<()> {
        let op = DirAdminOp::RemoveIndex {
            instance: instance.into(),
            attr: attr.into(),
        };
        ask!(self, user, op, DirAdminReply::Done => ())
    }

    fn admin(&self, user: &str, instance: &str) -> Result<AdminInfo> {
        let op = DirAdminOp::Admin {
            instance: instance.into(),
        };
        ask!(self, user, op, DirAdminReply::Admin { info } => info)
    }

    fn set_admin(
        &self,
        user: &str,
        instance: &str,
        change: &AdminChange,
        password: &str,
    ) -> Result<()> {
        let op = DirAdminOp::SetAdmin {
            instance: instance.into(),
            change: change.clone(),
            password: password.into(),
        };
        ask!(self, user, op, DirAdminReply::Done => ())
    }

    fn schema(&self, user: &str, instance: &str, q: &SchemaQuery) -> Result<SchemaPage> {
        let op = DirAdminOp::Schema {
            instance: instance.into(),
            query: q.clone(),
        };
        ask!(self, user, op, DirAdminReply::Schema { page } => page)
    }

    fn add_schema(&self, user: &str, instance: &str, kind: SchemaKind, text: &str) -> Result<()> {
        let op = DirAdminOp::AddSchema {
            instance: instance.into(),
            kind,
            text: text.into(),
        };
        ask!(self, user, op, DirAdminReply::Done => ())
    }

    fn remove_schema(&self, user: &str, instance: &str, kind: SchemaKind, id: &str) -> Result<()> {
        let op = DirAdminOp::RemoveSchema {
            instance: instance.into(),
            kind,
            id: id.into(),
        };
        ask!(self, user, op, DirAdminReply::Done => ())
    }

    fn access(&self, user: &str, instance: &str, dn: &str) -> Result<Access> {
        let op = DirAdminOp::Access {
            instance: instance.into(),
            dn: dn.into(),
        };
        ask!(self, user, op, DirAdminReply::Access { access } => access)
    }

    fn set_access(&self, user: &str, instance: &str, a: &Access) -> Result<()> {
        let op = DirAdminOp::SetAccess {
            instance: instance.into(),
            access: a.clone(),
        };
        ask!(self, user, op, DirAdminReply::Done => ())
    }

    fn publishing(&self, user: &str) -> Result<Vec<PublishAgent>> {
        ask!(self, user, DirAdminOp::Publishing, DirAdminReply::Publishing { agents } => agents)
    }

    fn set_publishing(
        &self,
        user: &str,
        agent: &PublishAgent,
        password: Option<&str>,
    ) -> Result<()> {
        let op = DirAdminOp::SetPublishing {
            agent: agent.clone(),
            password: password.map(str::to_string),
        };
        ask!(self, user, op, DirAdminReply::Done => ())
    }

    fn publishing_status(&self, user: &str) -> Result<Vec<AgentStatus>> {
        ask!(self, user, DirAdminOp::PublishingStatus, DirAdminReply::PublishingStatus { list } => list)
    }

    fn counters(&self, user: &str, instance: &str) -> Result<Counters> {
        let op = DirAdminOp::Counters {
            instance: instance.into(),
        };
        ask!(self, user, op, DirAdminReply::Counters { counters } => counters)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dirsrv::admin::{InstanceState, MemAdmin};

    /// Through the wire both ways, then into a directory in memory.
    struct Loop(MemAdmin);

    impl Sender for Loop {
        fn send(&self, user: &str, op: DirAdminOp) -> DirAdminReply {
            let op: DirAdminOp = ipc::decode(&ipc::encode(&op)).expect("the request decodes");
            let r = serve(&self.0, user, op);
            ipc::decode(&ipc::encode(&r)).expect("the answer decodes")
        }
    }

    #[test]
    #[allow(clippy::many_single_char_names)]
    fn calls_cross_the_link_whole() {
        let r = Remote(Loop(MemAdmin::new(
            "S",
            1_800_000_000,
            Box::new(|u, _| u == "QSECOFR"),
        )));
        let l = r.instances("QSECOFR").unwrap();
        assert_eq!(l[0].state, InstanceState::Running);
        assert_eq!(r.instances("X").unwrap_err().msgid, "CPF222E");
        let mut p = r.properties("QSECOFR", "QUSRDIR").unwrap();
        p.size_limit = 9;
        assert_eq!(
            r.set_properties("QSECOFR", "QUSRDIR", &p).unwrap(),
            ["ibm-slapdSizeLimit"]
        );
        r.add_suffix("QSECOFR", "QUSRDIR", "o=more").unwrap();
        assert!(
            r.suffixes("QSECOFR", "QUSRDIR")
                .unwrap()
                .iter()
                .any(|s| s.dn == "o=more")
        );
        let page = r
            .schema(
                "QSECOFR",
                "QUSRDIR",
                &SchemaQuery {
                    contains: "inetorgperson".into(),
                    kind: SchemaKind::Class,
                    ..SchemaQuery::default()
                },
            )
            .unwrap();
        assert_eq!(page.defs[0].class_kind, "STRUCTURAL");
        let a = r
            .access("QSECOFR", "QUSRDIR", "dc=quartz,dc=example")
            .unwrap();
        r.set_access("QSECOFR", "QUSRDIR", &a).unwrap();
        r.set_admin("QSECOFR", "QUSRDIR", &AdminChange::Password, "pw")
            .unwrap();
        assert!(r.counters("QSECOFR", "QUSRDIR").is_ok());
        assert_eq!(r.publishing("QSECOFR").unwrap().len(), 2);
    }
}
