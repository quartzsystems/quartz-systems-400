// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Version 11 (draft): the `Nfs` service's bodies, values and gates.

use std::fmt::Debug;

use ipc::{Wire, decode, encode};
use qap_proto::ops::nfs::{
    DaemonJob, DaemonType, Daemons, EndDaemons, Export, ExportOptions, ExportRef, Exports,
    HostOption, ListExports, MountNfs, MountedFs, Mounts, NfsOptions, PutExport, Registration,
    RemoveExport, StartDaemons, Unmount, UnmountKind,
};
use qap_proto::types::JobId;
use qap_proto::{Envelope, Op, Service, Status, VERSION};

fn same<T: Wire + PartialEq + Debug>(v: &T) {
    assert_eq!(&decode::<T>(&encode(v)).unwrap(), v);
}

const OPS: [Op; 13] = [
    Op::ListExports,
    Op::GetExport,
    Op::PutExport,
    Op::RemoveExport,
    Op::ExportAll,
    Op::ListDaemons,
    Op::StartDaemons,
    Op::EndDaemons,
    Op::ListMounts,
    Op::MountNfs,
    Op::Unmount,
    Op::GetNfsOptions,
    Op::SetNfsOptions,
];

#[test]
fn values_and_gates() {
    const { assert!(VERSION >= 11) };
    assert_eq!(encode(&Service::Nfs), [13]);
    assert_eq!(Service::Nfs.since(), 11);
    for (code, op) in (1..=13).zip(OPS) {
        assert_eq!(Op::of(Service::Nfs, code), Some(op));
        assert_eq!(op.code(), (Service::Nfs, code));
        assert_eq!(op.since(), 11, "{op:?}");
        assert!(!op.before_signon() && !op.in_lane(), "{op:?}");
    }
    assert_eq!(Op::of(Service::Nfs, 14), None);
    for v in 1..=10 {
        assert!(!Service::of_version(v).contains(&Service::Nfs));
    }
    assert!(Service::of_version(11).contains(&Service::Nfs));
    let req = Envelope::request(9, Op::ListExports, &ListExports::default());
    let reply = Envelope::reply(&req, Status::Ok, None, encode(&Exports::default()));
    assert!(!reply.defined_in(10) && reply.defined_in(11));
    for (i, d) in DaemonType::ALL.into_iter().enumerate() {
        assert_eq!(encode(&d), [u8::try_from(i).unwrap()]);
    }
    assert_eq!(DaemonType::Nlm.keyword(), "*NLM");
}

fn export() -> Export {
    Export {
        id: 3,
        path: "/home/shared".into(),
        options: ExportOptions {
            read_only: true,
            rw_hosts: vec!["admin01".into()],
            access_hosts: Some(vec!["@10.0.0.0/8".into(), "[fd00::1]".into()]),
            root_hosts: vec!["admin01".into()],
            anon: Some(-1),
            nosuid: true,
            v3: false,
            v4: true,
            public: true,
            pseudo_root: true,
            nfsroot: Some("/".into()),
            exname: Some("/shared".into()),
        },
        hosts: vec![HostOption {
            host: "pc1".into(),
            data_ccsid: 819,
            path_ccsid: 1208,
            sync: false,
        }],
        anon_uid: None,
        anon_user: None,
        exported_by: Some(JobId {
            name: "QPADEV0001".into(),
            user: "QSECOFR".into(),
            number: "000123".into(),
        }),
        since: 1_000,
    }
}

#[test]
fn bodies_round_trip() {
    same(&ListExports { start: 1, max: 5 });
    same(&Exports {
        total: 2,
        exports: vec![export(), Export::default()],
    });
    same(&ExportRef {
        path: "/home".into(),
    });
    same(&PutExport {
        export: export(),
        update_file: true,
    });
    same(&RemoveExport {
        path: "/home".into(),
        update_file: false,
    });
    same(&Daemons {
        jobs: vec![DaemonJob {
            daemon: DaemonType::Svr,
            job: JobId {
                name: "QNFSNFSD".into(),
                user: "QSECOFR".into(),
                number: "000200".into(),
            },
            ready: true,
            ending: false,
            registrations: vec![Registration {
                program: 100_003,
                version: 4,
                proto: "tcp".into(),
                port: 2049,
            }],
            since: 5,
        }],
    });
    same(&StartDaemons {
        daemon: Some(DaemonType::Svr),
        servers: 4,
        block_io: 0,
        retrieve_registrations: true,
        timeout: None,
    });
    same(&EndDaemons {
        daemon: None,
        timeout: Some(30),
    });
    same(&Mounts {
        mounts: vec![MountedFs {
            id: 101,
            kind: "nfs4".into(),
            over: "/mnt/a".into(),
            source: "srv:/x".into(),
            options: "ro".into(),
            read_only: true,
            responding: Some(false),
        }],
    });
    same(&MountNfs {
        host: "srv".into(),
        path: "/x".into(),
        over: "/mnt/a".into(),
        options: Some("vers=4".into()),
    });
    same(&Unmount {
        kind: UnmountKind::Udfs,
        over: "/u".into(),
    });
    same(&NfsOptions {
        flavors: vec![1],
        domain: "example.com".into(),
        max_threads: 8,
        handle_cache: 1024,
        utf8_validation: true,
    });
    assert!(ExportOptions::default().v3 && ExportOptions::default().v4);
}
