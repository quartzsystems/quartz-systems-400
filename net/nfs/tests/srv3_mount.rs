// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! MOUNT versions 3 and 1: `MNT` and its errors, the mount list
//! (`DUMP`, `UMNT`, `UMNTALL`), `EXPORT`.

#![allow(clippy::many_single_char_names)]
mod srv3_common;

use ifs::client::IfsClient;
use nfs::server::HostPat;
use nfs::types::mount::MOUNT_PROGRAM::{MOUNT_V1, MOUNT_V3};
use nfs::types::mount::{dirpath, fhstatus, mountres3, mountstat3};
use srv3_common::{Client, World};

const PEER: &str = "10.0.0.5:900";

fn mnt(
    c: &Client,
    p: &dyn nfs::rpc::LocalProgram,
    path: &str,
) -> Result<(Vec<u8>, Vec<i32>), mountstat3> {
    match c.call(
        p,
        &MOUNT_V3::Args::MOUNTPROC3_MNT(dirpath(path.as_bytes().to_vec())),
    ) {
        MOUNT_V3::Res::MOUNTPROC3_MNT(mountres3::MNT3_OK(ok)) => {
            Ok((ok.fhandle.0, ok.auth_flavors))
        }
        MOUNT_V3::Res::MOUNTPROC3_MNT(mountres3::default(s)) => Err(s),
        r => panic!("{r:?}"),
    }
}

fn dump(c: &Client, p: &dyn nfs::rpc::LocalProgram) -> Vec<(String, String)> {
    match c.call(p, &MOUNT_V3::Args::MOUNTPROC3_DUMP) {
        MOUNT_V3::Res::MOUNTPROC3_DUMP(l) => {
            l.0.into_iter()
                .map(|m| {
                    (
                        String::from_utf8(m.ml_hostname.0).unwrap(),
                        String::from_utf8(m.ml_directory.0).unwrap(),
                    )
                })
                .collect()
        }
        r => panic!("{r:?}"),
    }
}

#[test]
fn mnt_answers_the_handle_and_its_errors() {
    let w = World::new();
    let mut e = w.export(1, "/home/exp");
    w.write_file("/home/exp/file", b"x");
    w.job(9).mkdir("/home/exp/sub", 0o777).unwrap();
    e.access = Some(vec![HostPat::Net("10.0.0.0".parse().unwrap(), 24)]);
    w.put(e);
    let p = w.mountd();
    let c = Client::new(PEER, 100);
    let (fh, flavors) = mnt(&c, &p, "/home/exp").unwrap();
    assert_eq!(fh, w.handle(1, "/home/exp"));
    assert_eq!(flavors, vec![1]);
    // A directory under the export.
    assert_eq!(
        mnt(&c, &p, "/home/exp/sub").unwrap().0,
        w.handle(1, "/home/exp/sub")
    );
    assert_eq!(
        mnt(&c, &p, "/home/exp/nope"),
        Err(mountstat3::MNT3ERR_NOENT)
    );
    assert_eq!(
        mnt(&c, &p, "/home/exp/file"),
        Err(mountstat3::MNT3ERR_NOTDIR)
    );
    assert_eq!(mnt(&c, &p, "/home"), Err(mountstat3::MNT3ERR_ACCES));
    assert_eq!(mnt(&c, &p, "/tmp"), Err(mountstat3::MNT3ERR_ACCES));
    let far = Client::new("10.9.0.5:900", 100);
    assert_eq!(mnt(&far, &p, "/home/exp"), Err(mountstat3::MNT3ERR_ACCES));
    // The handle works with NFS.
    let n = w.nfs3();
    assert!(srv3_common::V3 { p: &n, c: &c }.getattr(&fh).is_ok());
}

#[test]
fn the_mount_list_and_the_exports() {
    let w = World::new();
    let mut e = w.export(1, "/home/exp");
    e.access = Some(vec![HostPat::Addr("10.0.0.5".parse().unwrap())]);
    w.put(e);
    let e2 = w.export(2, "/home/two");
    w.put(e2);
    let p = w.mountd();
    let c = Client::new(PEER, 100);
    mnt(&c, &p, "/home/exp").unwrap();
    mnt(&c, &p, "/home/two").unwrap();
    assert_eq!(
        dump(&c, &p),
        vec![
            ("client1".into(), "/home/exp".into()),
            ("client1".into(), "/home/two".into())
        ]
    );
    assert_eq!(
        c.call::<_, MOUNT_V3::Res>(
            &p,
            &MOUNT_V3::Args::MOUNTPROC3_UMNT(dirpath(b"/home/exp".to_vec()))
        ),
        MOUNT_V3::Res::MOUNTPROC3_UMNT
    );
    assert_eq!(dump(&c, &p), vec![("client1".into(), "/home/two".into())]);
    let _: MOUNT_V3::Res = c.call(&p, &MOUNT_V3::Args::MOUNTPROC3_UMNTALL);
    assert!(dump(&c, &p).is_empty());
    match c.call(&p, &MOUNT_V3::Args::MOUNTPROC3_EXPORT) {
        MOUNT_V3::Res::MOUNTPROC3_EXPORT(x) => {
            let got: Vec<(Vec<u8>, Vec<Vec<u8>>)> =
                x.0.into_iter()
                    .map(|n| {
                        (
                            n.ex_dir.0,
                            n.ex_groups.0.into_iter().map(|g| g.gr_name.0).collect(),
                        )
                    })
                    .collect();
            assert_eq!(
                got,
                vec![
                    (b"/home/exp".to_vec(), vec![b"10.0.0.5".to_vec()]),
                    (b"/home/two".to_vec(), vec![])
                ]
            );
        }
        r => panic!("{r:?}"),
    }
}

#[test]
fn version_1_for_showmount() {
    let w = World::new();
    let e = w.export(1, "/home/exp");
    w.put(e);
    let p = w.mountd();
    let c = Client::new(PEER, 100);
    match c.call(
        &p,
        &MOUNT_V1::Args::MOUNTPROC_MNT(dirpath(b"/home/exp".to_vec())),
    ) {
        MOUNT_V1::Res::MOUNTPROC_MNT(fhstatus::V0(h)) => {
            assert_eq!(h.to_vec(), w.handle(1, "/home/exp"));
        }
        r => panic!("{r:?}"),
    }
    match c.call(
        &p,
        &MOUNT_V1::Args::MOUNTPROC_MNT(dirpath(b"/nowhere".to_vec())),
    ) {
        MOUNT_V1::Res::MOUNTPROC_MNT(fhstatus::default(13)) => {}
        r => panic!("{r:?}"),
    }
    match c.call(&p, &MOUNT_V1::Args::MOUNTPROC_DUMP) {
        MOUNT_V1::Res::MOUNTPROC_DUMP(l) => assert_eq!(l.0.len(), 1),
        r => panic!("{r:?}"),
    }
    match c.call(&p, &MOUNT_V1::Args::MOUNTPROC_EXPORTALL) {
        MOUNT_V1::Res::MOUNTPROC_EXPORTALL(x) => assert_eq!(x.0.len(), 1),
        r => panic!("{r:?}"),
    }
}
