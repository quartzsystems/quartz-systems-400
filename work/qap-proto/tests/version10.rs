// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Version 10 (draft): the `NetServer` service's bodies, values and
//! gates.

use std::fmt::Debug;

use ipc::{Wire, decode, encode};
use qap_proto::types::{JobId, ObjectName};
use qap_proto::{Envelope, Op, Service, Status, VERSION};

fn same<T: Wire + PartialEq + Debug>(v: &T) {
    assert_eq!(&decode::<T>(&encode(v)).unwrap(), v);
}

fn assets() -> ObjectName {
    ObjectName {
        library: "QUSRSYS".into(),
        name: "PRT01".into(),
    }
}

fn job() -> JobId {
    JobId {
        name: "QZLSFILE".into(),
        user: "QUSER".into(),
        number: "004711".into(),
    }
}

use qap_proto::ops::netserver::{
    Attributes, ChangeAttributes, Connection, ConnectionKind, Dialect, DisabledUser, DisabledUsers,
    EnableUser, FileShare, ListSessions, ListShares, NetSession, NetSessions, PrintShare,
    ServerAttributes, SessionRef, Share, ShareBody, ShareDetail, ShareKind, ShareRef, Shares,
    SpoolType, StartServer,
};

const OPS: [Op; 13] = [
    Op::ListShares,
    Op::GetShare,
    Op::AddShare,
    Op::ChangeShare,
    Op::RemoveShare,
    Op::ListSessions,
    Op::EndSession,
    Op::GetServerAttributes,
    Op::ChangeServerAttributes,
    Op::ListDisabledUsers,
    Op::EnableUser,
    Op::StartServer,
    Op::EndServer,
];

#[test]
fn values_and_gates() {
    const { assert!(VERSION >= 10) };
    assert_eq!(encode(&Service::NetServer), [12]);
    assert_eq!(Service::NetServer.since(), 10);
    for (code, op) in (1..=13).zip(OPS) {
        assert_eq!(Op::of(Service::NetServer, code), Some(op));
        assert_eq!(op.code(), (Service::NetServer, code));
        assert_eq!(op.since(), 10, "{op:?}");
        assert!(!op.before_signon() && !op.in_lane(), "{op:?}");
    }
    assert_eq!(Op::of(Service::NetServer, 14), None);
    for v in 1..=9 {
        assert!(!Service::of_version(v).contains(&Service::NetServer));
    }
    assert!(Service::of_version(10).contains(&Service::NetServer));
    let req = Envelope::request(9, Op::ListShares, &ListShares::default());
    let reply = Envelope::reply(&req, Status::Ok, None, encode(&Shares::default()));
    assert!(!reply.defined_in(9) && reply.defined_in(10));
}

fn attrs() -> ServerAttributes {
    ServerAttributes {
        server_name: "QSHQ400".into(),
        domain: "QSHDOM".into(),
        text: "Head office".into(),
        allow_system_name: true,
        guest: Some("QSHGUEST".into()),
        code_page: 850,
        idle_timeout: -1,
        oplock_timeout: 30,
        browse_interval: 720_000,
        message_auth: 2,
        encryption_required: true,
        authorization_list: None,
        allow_netbios: false,
        min_dialect: Dialect::Smb311,
        log_signon: true,
    }
}

#[test]
fn bodies_round_trip() {
    let file = Share {
        name: "DOCS".into(),
        text: "Documents".into(),
        encryption_required: true,
        authorization_list: Some("NSUSERS".into()),
        current_users: 2,
        detail: ShareDetail::File {
            file: FileShare {
                path: "/home/docs".into(),
                read_only: true,
                max_users: Some(20),
                text_conversion: true,
                text_ccsid: 819,
                extensions: vec!["TXT".into()],
            },
        },
    };
    let print = Share {
        name: "PRT01".into(),
        detail: ShareDetail::Print {
            print: PrintShare {
                output_queue: assets(),
                driver: "Generic PCL 6".into(),
                spool_type: SpoolType::UserAscii,
                printer_file: None,
            },
        },
        ..Share::default()
    };
    same(&ListShares {
        kind: Some(ShareKind::Print),
        start: 1,
        max: 5,
    });
    same(&Shares {
        total: 2,
        shares: vec![file.clone(), print.clone()],
    });
    same(&ShareRef {
        name: "DOCS".into(),
    });
    same(&ShareBody { share: print });
    same(&ListSessions { start: 0, max: 0 });
    same(&NetSessions {
        total: 1,
        sessions: vec![NetSession {
            id: 7,
            workstation: "PC1".into(),
            expanded: "pc1.example.com".into(),
            user: "BOB".into(),
            guest: false,
            job: Some(job()),
            since: 1_000,
            last_active: 2_000,
            connections: vec![Connection {
                share: "DOCS".into(),
                kind: ConnectionKind::Ipc,
                open_files: 3,
                since: 1_500,
            }],
        }],
    });
    same(&SessionRef { id: 7 });
    same(&Attributes {
        active: true,
        job: Some(job()),
        current: attrs(),
        pending: ServerAttributes::default(),
    });
    same(&ChangeAttributes { pending: attrs() });
    same(&DisabledUsers {
        users: vec![DisabledUser {
            user: "BOB".into(),
            since: 5,
        }],
    });
    same(&EnableUser { user: "BOB".into() });
    same(&StartServer { reset: true });
}

#[test]
fn enums_by_place() {
    for (i, t) in SpoolType::ALL.iter().enumerate() {
        assert_eq!(encode(t), [u8::try_from(i).unwrap()]);
    }
    for (i, d) in Dialect::ALL.iter().enumerate() {
        assert_eq!(encode(d), [u8::try_from(i).unwrap()]);
    }
    assert_eq!(encode(&ConnectionKind::Ipc), [2]);
    assert_eq!(encode(&ShareKind::Print), [1]);
}
