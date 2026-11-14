// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Version 12 (draft): the `Dirsrv` service's bodies, values and gates.

use std::fmt::Debug;

use ipc::{Wire, decode, encode};
use qap_proto::ops::dirsrv::{
    AddDirSchema, AdminTarget, AgentKind, AgentStatus, Changed, DirAdmin, DirCounters, DirIndex,
    DirIndexRef, DirIndexes, DirProperties, DnRef, EntryAccess, Instance, InstanceRef,
    InstanceState, Instances, ListDirSchema, PublishAgent, Publishing, PublishingStatus,
    PutDirIndex, SchemaDefinition, SchemaDefinitions, SchemaKind, SchemaRef, SetDirAccess,
    SetDirAdmin, SetDirAutostart, SetDirProperties, SetPublishing, Suffix, SuffixKind, Suffixes,
};
use qap_proto::types::JobId;
use qap_proto::{Envelope, Op, Service, Status, VERSION};

fn same<T: Wire + PartialEq + Debug>(v: &T) {
    assert_eq!(&decode::<T>(&encode(v)).unwrap(), v);
}

const OPS: [Op; 24] = [
    Op::ListDirInstances,
    Op::SetDirAutostart,
    Op::StartDirInstance,
    Op::EndDirInstance,
    Op::RestartDirInstance,
    Op::GetDirProperties,
    Op::SetDirProperties,
    Op::ListDirSuffixes,
    Op::AddDirSuffix,
    Op::RemoveDirSuffix,
    Op::ListDirIndexes,
    Op::PutDirIndex,
    Op::RemoveDirIndex,
    Op::GetDirAdmin,
    Op::SetDirAdmin,
    Op::ListDirSchema,
    Op::AddDirSchema,
    Op::RemoveDirSchema,
    Op::GetDirAccess,
    Op::SetDirAccess,
    Op::GetPublishing,
    Op::SetPublishing,
    Op::GetPublishingStatus,
    Op::GetDirCounters,
];

#[test]
fn values_and_gates() {
    const { assert!(VERSION >= 12) };
    assert_eq!(encode(&Service::Dirsrv), [14]);
    assert_eq!(Service::Dirsrv.since(), 12);
    for (code, op) in (1..=24).zip(OPS) {
        assert_eq!(Op::of(Service::Dirsrv, code), Some(op));
        assert_eq!(op.code(), (Service::Dirsrv, code));
        assert_eq!(op.since(), 12, "{op:?}");
        assert!(!op.before_signon() && !op.in_lane(), "{op:?}");
    }
    assert_eq!(Op::of(Service::Dirsrv, 25), None);
    for v in 1..=11 {
        assert!(!Service::of_version(v).contains(&Service::Dirsrv));
    }
    assert!(Service::of_version(12).contains(&Service::Dirsrv));
    let req = Envelope::request(9, Op::ListDirInstances, &());
    let reply = Envelope::reply(&req, Status::Ok, None, encode(&Instances::default()));
    assert!(!reply.defined_in(11) && reply.defined_in(12));
    assert_eq!(encode(&SuffixKind::Config), [4]);
    assert_eq!(encode(&AdminTarget::UpdatePassword), [2]);
    assert_eq!(encode(&InstanceState::Ending), [3]);
}

fn job() -> JobId {
    JobId {
        name: "QUSRDIR".into(),
        user: "QDIRSRV".into(),
        number: "123470".into(),
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn bodies_round_trip() {
    same(&Instances {
        instances: vec![Instance {
            name: "QUSRDIR".into(),
            description: "QUSRDIR".into(),
            autostart: true,
            state: InstanceState::Running,
            job: Some(job()),
            port: 389,
            secure_port: 636,
            security: "TLS".into(),
            certificate: Some("DIRCERT".into()),
            version: "6.1".into(),
            since: 1_000,
        }],
    });
    same(&InstanceRef {
        instance: "QUSRDIR".into(),
    });
    same(&SetDirAutostart {
        instance: "DEVDIR".into(),
        autostart: false,
    });
    same(&SetDirProperties {
        instance: "QUSRDIR".into(),
        properties: DirProperties {
            port: 389,
            security: "none".into(),
            size_limit: 500,
            allow_anonymous: true,
            referrals: vec!["ldap://a.example".into()],
            ..DirProperties::default()
        },
    });
    same(&Changed {
        attributes: vec!["ibm-slapdSizeLimit".into()],
    });
    same(&Suffixes {
        suffixes: vec![Suffix {
            dn: "dc=example".into(),
            kind: SuffixKind::User,
            entries: Some(5),
        }],
    });
    same(&DnRef {
        instance: "QUSRDIR".into(),
        dn: "dc=example".into(),
    });
    let ix = DirIndex {
        attribute: "telephoneNumber".into(),
        presence: true,
        equality: true,
        substring: false,
        always: false,
    };
    same(&DirIndexes {
        indexes: vec![ix.clone()],
    });
    same(&PutDirIndex {
        instance: "QUSRDIR".into(),
        index: ix,
    });
    same(&DirIndexRef {
        instance: "QUSRDIR".into(),
        attribute: "sn".into(),
    });
    same(&DirAdmin {
        dn: "cn=Administrator".into(),
        password_set: false,
        update_dn: None,
    });
    same(&SetDirAdmin {
        instance: "QUSRDIR".into(),
        target: AdminTarget::NewDn,
        dn: "cn=Boss".into(),
        password: "pw".into(),
    });
    same(&ListDirSchema {
        instance: "QUSRDIR".into(),
        kind: SchemaKind::ObjectClass,
        contains: "person".into(),
        added_only: false,
        start: 0,
        max: 25,
    });
    same(&SchemaDefinitions {
        total: 1,
        definitions: vec![SchemaDefinition {
            kind: SchemaKind::ObjectClass,
            oid: "2.5.6.6".into(),
            names: vec!["person".into()],
            class_kind: "STRUCTURAL".into(),
            must: vec!["cn".into(), "sn".into()],
            text: "( 2.5.6.6 NAME 'person' )".into(),
            ..SchemaDefinition::default()
        }],
    });
    same(&AddDirSchema {
        instance: "QUSRDIR".into(),
        kind: SchemaKind::AttributeType,
        description: "( 1.2.3 NAME 'x' )".into(),
    });
    same(&SchemaRef {
        instance: "QUSRDIR".into(),
        kind: SchemaKind::AttributeType,
        id: "x".into(),
    });
    same(&SetDirAccess {
        instance: "QUSRDIR".into(),
        access: EntryAccess {
            dn: "dc=example".into(),
            own_acl: true,
            acl: vec!["group:cn=anybody:normal:rsc".into()],
            acl_propagate: true,
            acl_source: "dc=example".into(),
            own_owners: false,
            owners: vec!["access-id:cn=Administrator".into()],
            owner_propagate: true,
            owner_source: "default".into(),
        },
    });
    let agent = PublishAgent {
        kind: AgentKind::Printers,
        enabled: true,
        server: "ldap.example".into(),
        port: 636,
        tls: true,
        bind_dn: "cn=Publisher".into(),
        password_set: true,
        parent_dn: "ou=Printers,dc=example".into(),
        start_tls: false,
        create_parent: true,
        retry: 300,
        resync: 0,
        publisher: "QS400 HQ".into(),
    };
    same(&Publishing {
        agents: vec![agent.clone()],
    });
    same(&SetPublishing {
        agent,
        password: Some(String::new()),
    });
    same(&PublishingStatus {
        agents: vec![AgentStatus {
            kind: AgentKind::Users,
            job: None,
            active: false,
            published: 0,
            failed: 0,
            last: 0,
            message_id: Some("GLD010B".into()),
            message_text: "ended".into(),
        }],
    });
    same(&DirCounters {
        started: 5,
        connections: 2,
        searches: 90,
        ..DirCounters::default()
    });
}
