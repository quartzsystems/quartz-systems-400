// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Version 11 (draft): the `Nfs` service against the test system, whose
//! NFS is its tables in memory changed by the commands: each group of
//! operations, `*IOSYSCFG`, the exit point, and version 10 without it.

#![allow(clippy::many_single_char_names)]

mod common;

use qap_proto::ops::connection::HelloReply;
use qap_proto::ops::nfs::{
    DaemonType, Daemons, EndDaemons, Export, ExportOptions, ExportRef, Exports, HostOption,
    ListExports, MountNfs, Mounts, NfsOptions, PutExport, RemoveExport, StartDaemons, Unmount,
    UnmountKind,
};
use qap_proto::{Envelope, Op, Service, Status};
use qapd::manage::ExitRequest;

use common::{Run, cfg, start};

fn signed_on(user: &str, password: &str) -> Run {
    let mut r = start(cfg());
    let h: HelloReply = r.client.hello_range(1, 11).body_as().unwrap();
    assert!(h.services.contains(&Service::Nfs));
    r.client.sign_on(user, password);
    r
}

fn id(e: &Envelope) -> String {
    e.message.as_ref().map(|m| m.id.clone()).unwrap_or_default()
}

fn exports(r: &mut Run) -> Exports {
    r.client
        .call(Op::ListExports, &ListExports::default())
        .body_as()
        .unwrap()
}

fn ran(r: &Run) -> Vec<String> {
    std::mem::take(&mut r.c.nfs.lock().unwrap().ran)
}

#[test]
fn exports_listed_put_and_removed() {
    let mut r = signed_on("SECOFR", "SECPW01");
    let all = exports(&mut r);
    assert_eq!(all.total, 1);
    let home = &all.exports[0];
    assert_eq!(home.path, "/home");
    assert!(home.options.read_only);
    assert_eq!(home.options.rw_hosts, ["admin"]);
    assert_eq!(home.anon_user.as_deref(), Some("QNFSANON"));
    assert_eq!(
        home.hosts,
        [HostOption {
            host: "pc1".into(),
            data_ccsid: 819,
            path_ccsid: 819,
            sync: false,
        }]
    );
    // Put: a new export, and into /etc/exports.
    let e = Export {
        path: "/QOpenSys/data".into(),
        options: ExportOptions {
            v3: false,
            root_hosts: vec!["admin".into()],
            ..ExportOptions::default()
        },
        ..Export::default()
    };
    let ok = r.client.call(
        Op::PutExport,
        &PutExport {
            export: e,
            update_file: true,
        },
    );
    assert_eq!(ok.status, Status::Ok, "{:?}", ok.message);
    assert_eq!(
        ran(&r),
        ["EXPORTFS OPTIONS('-I -F -O root=admin,vers=4') DIR('/QOpenSys/data')"]
    );
    let got: Export = r
        .client
        .call(
            Op::GetExport,
            &ExportRef {
                path: "/qopensys/data/".into(),
            },
        )
        .body_as()
        .unwrap();
    assert!(!got.options.v3 && got.options.v4);
    assert_eq!(got.exported_by.map(|j| j.user), Some("SECOFR".into()));
    // Remove it; a second time it is not exported.
    let rm = RemoveExport {
        path: "/QOpenSys/data".into(),
        update_file: false,
    };
    assert_eq!(r.client.call(Op::RemoveExport, &rm).status, Status::Ok);
    let no = r.client.call(Op::RemoveExport, &rm);
    assert_eq!((no.status, id(&no).as_str()), (Status::Error, "CPFA1C7"));
    let nf = r.client.call(
        Op::GetExport,
        &ExportRef {
            path: "/QOpenSys/data".into(),
        },
    );
    assert_eq!((nf.status, id(&nf).as_str()), (Status::NotFound, "CPDA1C5"));
    // Export all: /etc/exports' two entries and the one put with -F.
    assert_eq!(r.client.call(Op::ExportAll, &()).status, Status::Ok);
    assert_eq!(exports(&mut r).total, 3);
}

#[test]
fn daemons_started_and_ended() {
    let mut r = signed_on("SECOFR", "SECPW01");
    let d: Daemons = r.client.call(Op::ListDaemons, &()).body_as().unwrap();
    assert_eq!(d.jobs.len(), 2);
    assert_eq!(d.jobs[1].job.name, "QNFSNFSD");
    assert_eq!(d.jobs[1].registrations[0].port, 2049);
    let ok = r.client.call(
        Op::StartDaemons,
        &StartDaemons {
            daemon: None,
            servers: 2,
            ..StartDaemons::default()
        },
    );
    assert_eq!(ok.status, Status::Ok);
    assert_eq!(
        ran(&r),
        ["STRNFSSVR SERVER(*ALL) NBRSVR(2) STRJOBTIMO(*NOMAX)"]
    );
    let d: Daemons = r.client.call(Op::ListDaemons, &()).body_as().unwrap();
    assert_eq!(d.jobs.len(), 7);
    let ok = r.client.call(
        Op::EndDaemons,
        &EndDaemons {
            daemon: Some(DaemonType::Nlm),
            timeout: Some(30),
        },
    );
    assert_eq!(ok.status, Status::Ok);
    assert_eq!(ran(&r), ["ENDNFSSVR SERVER(*NLM) ENDJOBTIMO(30)"]);
}

#[test]
fn mounts_and_options() {
    let mut r = signed_on("SECOFR", "SECPW01");
    let ok = r.client.call(
        Op::MountNfs,
        &MountNfs {
            host: "srv".into(),
            path: "/x".into(),
            over: "/mnt/x".into(),
            options: Some("ro".into()),
        },
    );
    assert_eq!(ok.status, Status::Ok);
    let m: Mounts = r.client.call(Op::ListMounts, &()).body_as().unwrap();
    assert_eq!(m.mounts.len(), 3);
    assert_eq!(m.mounts[2].source, "srv:/x");
    let u = Unmount {
        kind: UnmountKind::Nfs,
        over: "/mnt/x".into(),
    };
    assert_eq!(r.client.call(Op::Unmount, &u).status, Status::Ok);
    let o: NfsOptions = r.client.call(Op::GetNfsOptions, &()).body_as().unwrap();
    assert_eq!(o.flavors, [1]);
    let want = NfsOptions {
        domain: "example.com".into(),
        ..o
    };
    let back: NfsOptions = r.client.call(Op::SetNfsOptions, &want).body_as().unwrap();
    assert_eq!(back, want);
    let bad = NfsOptions {
        flavors: vec![6],
        ..want
    };
    let e = r.client.call(Op::SetNfsOptions, &bad);
    assert_eq!((e.status, id(&e).as_str()), (Status::Error, "CPFA1BB"));
}

#[test]
fn without_iosyscfg_nothing_changes() {
    let mut r = signed_on("BOB", "SECRET1");
    // Reading is open; changing needs *IOSYSCFG.
    assert_eq!(exports(&mut r).total, 1);
    let no = r.client.call(Op::ExportAll, &());
    assert_eq!(
        (no.status, id(&no).as_str()),
        (Status::NotAuthorized, "CPFA1B8")
    );
    let no = r.client.call(Op::SetNfsOptions, &NfsOptions::default());
    assert_eq!(
        (no.status, id(&no).as_str()),
        (Status::NotAuthorized, "CPFA1B8")
    );
    assert!(ran(&r).is_empty());
}

#[test]
fn the_exit_point_sees_each_request() {
    let mut r = signed_on("SECOFR", "SECPW01");
    r.c.manage.lock().unwrap().exit_refuses = Some(Box::new(|x: &ExitRequest| {
        x.server == qapd::nfs::SERVER && x.op == 10
    }));
    let no = r.client.call(
        Op::MountNfs,
        &MountNfs {
            host: "srv".into(),
            path: "/x".into(),
            over: "/mnt/x".into(),
            options: None,
        },
    );
    assert_eq!(
        (no.status, id(&no).as_str()),
        (Status::NotAuthorized, "TCP1A5E")
    );
    assert!(ran(&r).is_empty());
    assert_eq!(r.client.call(Op::ListMounts, &()).status, Status::Ok);
}

#[test]
fn version_10_has_no_nfs() {
    let mut r = start(cfg());
    let h: HelloReply = r.client.hello_range(1, 10).body_as().unwrap();
    assert!(!h.services.contains(&Service::Nfs));
}
