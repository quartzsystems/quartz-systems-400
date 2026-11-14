// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Version 12 (draft): the `Dirsrv` service against the test system,
//! whose directory server is the administration interface's sample in
//! memory: each group of operations, the authority, the exit point, and
//! version 11 without it.

#![allow(clippy::many_single_char_names)]

mod common;

use dirsrv::admin::DirAdmin as _;
use qap_proto::ops::connection::HelloReply;
use qap_proto::ops::dirsrv::{
    AddDirSchema, AdminTarget, AgentKind, Changed, DirAdmin, DirCounters, DirIndex, DirIndexes,
    DirProperties, DnRef, EntryAccess, InstanceRef, InstanceState, Instances, ListDirSchema,
    Publishing, PublishingStatus, PutDirIndex, SchemaDefinitions, SchemaKind, SchemaRef,
    SetDirAccess, SetDirAdmin, SetDirAutostart, SetDirProperties, SetPublishing, SuffixKind,
    Suffixes,
};
use qap_proto::{Envelope, Op, Service, Status};
use qapd::manage::ExitRequest;

use common::{Run, cfg, start};

fn signed_on(user: &str, password: &str) -> Run {
    let mut r = start(cfg());
    let h: HelloReply = r.client.hello_range(1, 12).body_as().unwrap();
    assert!(h.services.contains(&Service::Dirsrv));
    r.client.sign_on(user, password);
    r
}

fn id(e: &Envelope) -> String {
    e.message.as_ref().map(|m| m.id.clone()).unwrap_or_default()
}

fn q() -> InstanceRef {
    InstanceRef {
        instance: "QUSRDIR".into(),
    }
}

#[test]
fn instances_start_end_and_autostart() {
    let mut r = signed_on("SECOFR", "SECPW01");
    let l: Instances = r.client.call(Op::ListDirInstances, &()).body_as().unwrap();
    assert_eq!(l.instances.len(), 2);
    assert_eq!(l.instances[0].state, InstanceState::Running);
    assert_eq!(
        l.instances[0].job.as_ref().map(|j| j.user.as_str()),
        Some("QDIRSRV")
    );
    let dev = InstanceRef {
        instance: "devdir".into(),
    };
    assert_eq!(r.client.call(Op::StartDirInstance, &dev).status, Status::Ok);
    assert_eq!(
        r.client.call(Op::RestartDirInstance, &dev).status,
        Status::Ok
    );
    let c: DirCounters = r.client.call(Op::GetDirCounters, &dev).body_as().unwrap();
    assert_eq!(c.binds, 0);
    assert_eq!(r.client.call(Op::EndDirInstance, &dev).status, Status::Ok);
    let e = r.client.call(Op::GetDirCounters, &dev);
    assert_eq!((e.status, id(&e).as_str()), (Status::Error, "CPF9898"));
    let set = SetDirAutostart {
        instance: "DEVDIR".into(),
        autostart: true,
    };
    assert_eq!(r.client.call(Op::SetDirAutostart, &set).status, Status::Ok);
    let l: Instances = r.client.call(Op::ListDirInstances, &()).body_as().unwrap();
    assert!(l.instances[1].autostart);
    let no = r.client.call(
        Op::StartDirInstance,
        &InstanceRef {
            instance: "NONE".into(),
        },
    );
    assert_eq!((no.status, id(&no).as_str()), (Status::NotFound, "GLD0203"));
}

#[test]
fn properties_suffixes_and_indexes() {
    let mut r = signed_on("SECOFR", "SECPW01");
    let p: DirProperties = r.client.call(Op::GetDirProperties, &q()).body_as().unwrap();
    assert_eq!((p.port, p.size_limit), (389, 500));
    let to = DirProperties {
        size_limit: 1000,
        allow_anonymous: false,
        ..p.clone()
    };
    let c: Changed = r
        .client
        .call(
            Op::SetDirProperties,
            &SetDirProperties {
                instance: "QUSRDIR".into(),
                properties: to,
            },
        )
        .body_as()
        .unwrap();
    assert_eq!(c.attributes, ["ibm-slapdSizeLimit", "ibm-slapdAllowAnon"]);
    let bad = r.client.call(
        Op::SetDirProperties,
        &SetDirProperties {
            instance: "QUSRDIR".into(),
            properties: DirProperties {
                password_encryption: "aes256".into(),
                ..p
            },
        },
    );
    assert_eq!((bad.status, id(&bad).as_str()), (Status::Error, "GLD0232"));
    let add = DnRef {
        instance: "QUSRDIR".into(),
        dn: "o=branch".into(),
    };
    assert_eq!(r.client.call(Op::AddDirSuffix, &add).status, Status::Ok);
    let again = r.client.call(Op::AddDirSuffix, &add);
    assert_eq!(again.status, Status::Exists);
    let s: Suffixes = r.client.call(Op::ListDirSuffixes, &q()).body_as().unwrap();
    assert!(
        s.suffixes
            .iter()
            .any(|s| s.dn == "o=branch" && s.kind == SuffixKind::User)
    );
    assert_eq!(r.client.call(Op::RemoveDirSuffix, &add).status, Status::Ok);
    let ix = PutDirIndex {
        instance: "QUSRDIR".into(),
        index: DirIndex {
            attribute: "givenName".into(),
            equality: true,
            ..DirIndex::default()
        },
    };
    assert_eq!(r.client.call(Op::PutDirIndex, &ix).status, Status::Ok);
    let l: DirIndexes = r.client.call(Op::ListDirIndexes, &q()).body_as().unwrap();
    assert_eq!(l.indexes.len(), 8);
    let a: DirAdmin = r.client.call(Op::GetDirAdmin, &q()).body_as().unwrap();
    assert!(!a.password_set);
    let set = SetDirAdmin {
        instance: "QUSRDIR".into(),
        target: AdminTarget::Password,
        dn: String::new(),
        password: "secret".into(),
    };
    assert_eq!(r.client.call(Op::SetDirAdmin, &set).status, Status::Ok);
    let a: DirAdmin = r.client.call(Op::GetDirAdmin, &q()).body_as().unwrap();
    assert!(a.password_set);
    let empty = SetDirAdmin {
        password: String::new(),
        ..set
    };
    let e = r.client.call(Op::SetDirAdmin, &empty);
    assert_eq!((e.status, id(&e).as_str()), (Status::Error, "GLD0202"));
}

#[test]
fn schema_access_and_publishing() {
    let mut r = signed_on("SECOFR", "SECPW01");
    let add = AddDirSchema {
        instance: "QUSRDIR".into(),
        kind: SchemaKind::AttributeType,
        description:
            "( 1.3.6.1.4.1.99999.1.9 NAME 'roomNumber2' SYNTAX 1.3.6.1.4.1.1466.115.121.1.15 )"
                .into(),
    };
    assert_eq!(r.client.call(Op::AddDirSchema, &add).status, Status::Ok);
    let l: SchemaDefinitions = r
        .client
        .call(
            Op::ListDirSchema,
            &ListDirSchema {
                instance: "QUSRDIR".into(),
                added_only: true,
                ..ListDirSchema::default()
            },
        )
        .body_as()
        .unwrap();
    assert_eq!(l.total, 1);
    assert_eq!(l.definitions[0].names, ["roomNumber2"]);
    let dup = r.client.call(Op::AddDirSchema, &add);
    assert_eq!(dup.status, Status::Exists);
    let rm = SchemaRef {
        instance: "QUSRDIR".into(),
        kind: SchemaKind::AttributeType,
        id: "roomNumber2".into(),
    };
    assert_eq!(r.client.call(Op::RemoveDirSchema, &rm).status, Status::Ok);

    let dn = DnRef {
        instance: "QUSRDIR".into(),
        dn: "ou=People,dc=quartz,dc=example".into(),
    };
    let a: EntryAccess = r.client.call(Op::GetDirAccess, &dn).body_as().unwrap();
    assert!(a.own_owners && !a.own_acl);
    assert_eq!(a.acl_source, "dc=quartz,dc=example");
    let set = SetDirAccess {
        instance: "QUSRDIR".into(),
        access: EntryAccess {
            own_acl: true,
            acl: vec!["group:cn=authenticated:normal:rsc".into()],
            ..a
        },
    };
    assert_eq!(r.client.call(Op::SetDirAccess, &set).status, Status::Ok);
    let a: EntryAccess = r.client.call(Op::GetDirAccess, &dn).body_as().unwrap();
    assert_eq!(a.acl, ["group:cn=authenticated:normal:rsc"]);
    let nf = r.client.call(
        Op::GetDirAccess,
        &DnRef {
            instance: "QUSRDIR".into(),
            dn: "o=none".into(),
        },
    );
    assert_eq!(nf.status, Status::NotFound);

    let p: Publishing = r.client.call(Op::GetPublishing, &()).body_as().unwrap();
    assert_eq!(p.agents.len(), 2);
    let mut printers = p.agents[1].clone();
    assert_eq!(printers.kind, AgentKind::Printers);
    printers.enabled = true;
    let ok = r.client.call(
        Op::SetPublishing,
        &SetPublishing {
            agent: printers,
            password: Some("pw".into()),
        },
    );
    assert_eq!(ok.status, Status::Ok);
    let p: Publishing = r.client.call(Op::GetPublishing, &()).body_as().unwrap();
    assert!(p.agents[1].enabled && p.agents[1].password_set);
    let s: PublishingStatus = r
        .client
        .call(Op::GetPublishingStatus, &())
        .body_as()
        .unwrap();
    assert_eq!(s.agents[0].message_id.as_deref(), Some("GLD010B"));
}

#[test]
fn without_the_authority_nothing_is_read_or_changed() {
    let mut r = signed_on("BOB", "SECRET1");
    let no = r.client.call(Op::ListDirInstances, &());
    assert_eq!(
        (no.status, id(&no).as_str()),
        (Status::NotAuthorized, "CPF222E")
    );
    let no = r.client.call(Op::StartDirInstance, &q());
    assert_eq!(no.status, Status::NotAuthorized);
    assert_eq!(
        r.c.dirsrv.instances("SECOFR").unwrap()[1].state,
        dirsrv::admin::InstanceState::Stopped
    );
}

#[test]
fn the_exit_point_sees_each_request() {
    let mut r = signed_on("SECOFR", "SECPW01");
    r.c.manage.lock().unwrap().exit_refuses = Some(Box::new(|x: &ExitRequest| {
        x.server == qapd::dirsrv::SERVER && x.op == 9 && x.detail == "o=branch"
    }));
    let no = r.client.call(
        Op::AddDirSuffix,
        &DnRef {
            instance: "QUSRDIR".into(),
            dn: "o=branch".into(),
        },
    );
    assert_eq!(
        (no.status, id(&no).as_str()),
        (Status::NotAuthorized, "TCP1A5E")
    );
    let s: Suffixes = r.client.call(Op::ListDirSuffixes, &q()).body_as().unwrap();
    assert!(!s.suffixes.iter().any(|s| s.dn == "o=branch"));
}

#[test]
fn version_11_has_no_dirsrv() {
    let mut r = start(cfg());
    let h: HelloReply = r.client.hello_range(1, 11).body_as().unwrap();
    assert!(!h.services.contains(&Service::Dirsrv));
}
