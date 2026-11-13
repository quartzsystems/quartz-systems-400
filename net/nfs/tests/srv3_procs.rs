// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every NFS version 3 procedure against `IfsVfs` over a `MemBackend`,
//! through real RPC messages: what each answers, and its errors.

#![allow(clippy::many_single_char_names)]
#![allow(clippy::too_many_lines)]

mod srv3_common;

use nfs::types::nfs3::NFS_PROGRAM::NFS_V3::{Args, Res};
use nfs::types::nfs3::{self as t3, nfsstat3 as S};
use srv3_common::{Client, V3, World, dirop, fh, mode, no_sattr};

fn setup() -> (World, Vec<u8>) {
    let w = World::new();
    let e = w.export(1, "/home/exp");
    w.put(e);
    let root = w.handle(1, "/home/exp");
    (w, root)
}

#[test]
fn null_getattr_lookup_access_readlink() {
    let (w, root) = setup();
    let p = w.nfs3();
    let c = Client::new("10.0.0.5:900", 100);
    let v = V3 { p: &p, c: &c };
    assert_eq!(c.v3(&p, &Args::NFSPROC3_NULL), Res::NFSPROC3_NULL);
    let a = v.getattr(&root).unwrap();
    assert_eq!(a.r#type, t3::ftype3::NF3DIR);
    // No primary group: its bits are 0, as `stat` shows them.
    assert_eq!(a.mode & 0o777, 0o707);
    let f = v.create(&root, "a.txt").unwrap();
    assert_eq!(v.lookup(&root, "a.txt").unwrap(), f);
    assert_eq!(v.lookup(&root, "."), Ok(root.clone()));
    // `..` of the export's root stays at the root.
    assert_eq!(v.lookup(&root, ".."), Ok(root.clone()));
    assert_eq!(v.lookup(&root, "nope"), Err(S::NFS3ERR_NOENT));
    assert_eq!(v.lookup(&f, "x"), Err(S::NFS3ERR_NOTDIR));
    assert_eq!(v.lookup(&root, "a/b"), Err(S::NFS3ERR_INVAL));
    assert_eq!(
        v.lookup(&root, &"n".repeat(256)),
        Err(S::NFS3ERR_NAMETOOLONG)
    );
    let fa = v.getattr(&f).unwrap();
    assert_eq!(
        (fa.r#type, fa.mode, fa.uid, fa.size),
        (t3::ftype3::NF3REG, 0o604, 100, 0)
    );
    // ACCESS: BOB owns it (read, write); ZED only reads.
    let all = t3::ACCESS3_READ
        | t3::ACCESS3_MODIFY
        | t3::ACCESS3_EXTEND
        | t3::ACCESS3_EXECUTE
        | t3::ACCESS3_DELETE;
    let acc = |c: &Client, h: &[u8]| match c.v3(
        &p,
        &Args::NFSPROC3_ACCESS(t3::ACCESS3args {
            object: fh(h),
            access: all,
        }),
    ) {
        Res::NFSPROC3_ACCESS(t3::ACCESS3res::NFS3_OK(ok)) => ok.access,
        r => panic!("{r:?}"),
    };
    assert_eq!(
        acc(&c, &f),
        t3::ACCESS3_READ | t3::ACCESS3_MODIFY | t3::ACCESS3_EXTEND
    );
    let zed = Client::new("10.0.0.5:901", 102);
    assert_eq!(acc(&zed, &f), t3::ACCESS3_READ);
    assert_eq!(
        acc(&zed, &root),
        t3::ACCESS3_READ | t3::ACCESS3_MODIFY | t3::ACCESS3_EXTEND | t3::ACCESS3_DELETE,
        "a directory everyone may change; EXECUTE is LOOKUP's bit there"
    );
    // SYMLINK and READLINK.
    let a = t3::SYMLINK3args {
        r#where: dirop(&root, "l"),
        symlink: t3::symlinkdata3 {
            symlink_attributes: no_sattr(),
            symlink_data: t3::nfspath3(b"a.txt".to_vec()),
        },
    };
    let l = match c.v3(&p, &Args::NFSPROC3_SYMLINK(a)) {
        Res::NFSPROC3_SYMLINK(t3::SYMLINK3res::NFS3_OK(ok)) => ok.obj.unwrap().data,
        r => panic!("{r:?}"),
    };
    assert_eq!(v.getattr(&l).unwrap().r#type, t3::ftype3::NF3LNK);
    match c.v3(
        &p,
        &Args::NFSPROC3_READLINK(t3::READLINK3args { symlink: fh(&l) }),
    ) {
        Res::NFSPROC3_READLINK(t3::READLINK3res::NFS3_OK(ok)) => assert_eq!(ok.data.0, b"a.txt"),
        r => panic!("{r:?}"),
    }
    match c.v3(
        &p,
        &Args::NFSPROC3_READLINK(t3::READLINK3args { symlink: fh(&f) }),
    ) {
        Res::NFSPROC3_READLINK(t3::READLINK3res::default(s, _)) => assert_eq!(s, S::NFS3ERR_INVAL),
        r => panic!("{r:?}"),
    }
}

#[test]
fn read_write_commit_setattr() {
    let (w, root) = setup();
    let p = w.nfs3();
    let c = Client::new("10.0.0.5:900", 100);
    let v = V3 { p: &p, c: &c };
    let f = v.create(&root, "data").unwrap();
    let r = v
        .write(&f, 0, b"hello world", t3::stable_how::UNSTABLE)
        .unwrap();
    assert_eq!((r.count, r.committed), (11, t3::stable_how::UNSTABLE));
    assert_eq!(r.verf, w.instance.verf());
    assert_eq!(r.file_wcc.before.unwrap().size, 0);
    assert_eq!(r.file_wcc.after.unwrap().size, 11);
    let s = v.write(&f, 6, b"WORLD", t3::stable_how::FILE_SYNC).unwrap();
    assert_eq!(s.committed, t3::stable_how::FILE_SYNC);
    assert_eq!(v.read(&f, 0, 100).unwrap(), (b"hello WORLD".to_vec(), true));
    assert_eq!(v.read(&f, 0, 5).unwrap(), (b"hello".to_vec(), false));
    assert_eq!(v.read(&f, 50, 5).unwrap(), (Vec::new(), true));
    assert_eq!(w.read_file("/home/exp/data"), b"hello WORLD");
    assert_eq!(v.read(&root, 0, 5), Err(S::NFS3ERR_ISDIR));
    assert_eq!(
        v.write(&root, 0, b"x", t3::stable_how::UNSTABLE).err(),
        Some(S::NFS3ERR_ISDIR)
    );
    match c.v3(
        &p,
        &Args::NFSPROC3_COMMIT(t3::COMMIT3args {
            file: fh(&f),
            offset: 0,
            count: 0,
        }),
    ) {
        Res::NFSPROC3_COMMIT(t3::COMMIT3res::NFS3_OK(ok)) => assert_eq!(ok.verf, w.instance.verf()),
        r => panic!("{r:?}"),
    }
    // A WRITE whose count is more than its data.
    let a = t3::WRITE3args {
        file: fh(&f),
        offset: 0,
        count: 9,
        stable: t3::stable_how::UNSTABLE,
        data: b"abc".to_vec(),
    };
    assert!(matches!(
        c.v3(&p, &Args::NFSPROC3_WRITE(a)),
        Res::NFSPROC3_WRITE(t3::WRITE3res::default(S::NFS3ERR_INVAL, _))
    ));
    // SETATTR: size, mode, times; a guard that does not match.
    v.setattr(
        &f,
        t3::sattr3 {
            size: Some(5),
            ..mode(0o600)
        },
    )
    .unwrap();
    let a = v.getattr(&f).unwrap();
    assert_eq!((a.size, a.mode), (5, 0o600));
    let t = t3::nfstime3 {
        seconds: 1_000_000,
        nseconds: 5,
    };
    v.setattr(
        &f,
        t3::sattr3 {
            mtime: t3::set_mtime::SET_TO_CLIENT_TIME(t.clone()),
            ..no_sattr()
        },
    )
    .unwrap();
    assert_eq!(v.getattr(&f).unwrap().mtime, t);
    let guard = t3::SETATTR3args {
        object: fh(&f),
        new_attributes: mode(0o644),
        guard: Some(t3::nfstime3 {
            seconds: 1,
            nseconds: 0,
        }),
    };
    match c.v3(&p, &Args::NFSPROC3_SETATTR(guard)) {
        Res::NFSPROC3_SETATTR(t3::SETATTR3res::default(s, f)) => {
            assert_eq!(s, S::NFS3ERR_NOT_SYNC);
            assert!(f.obj_wcc.after.is_some());
        }
        r => panic!("{r:?}"),
    }
    let ctime = v.getattr(&f).unwrap().ctime;
    let guard = t3::SETATTR3args {
        object: fh(&f),
        new_attributes: mode(0o640),
        guard: Some(ctime),
    };
    assert!(matches!(
        c.v3(&p, &Args::NFSPROC3_SETATTR(guard)),
        Res::NFSPROC3_SETATTR(t3::SETATTR3res::NFS3_OK(_))
    ));
    // Another user may not chmod BOB's file.
    let zed = Client::new("10.0.0.5:901", 102);
    let vz = V3 { p: &p, c: &zed };
    assert_eq!(vz.setattr(&f, mode(0o666)), Err(S::NFS3ERR_PERM));
    assert_eq!(
        vz.write(&f, 0, b"x", t3::stable_how::UNSTABLE).err(),
        Some(S::NFS3ERR_ACCES)
    );
}

#[test]
fn create_mkdir_mknod_remove_rmdir_rename_link() {
    let (w, root) = setup();
    let p = w.nfs3();
    let c = Client::new("10.0.0.5:900", 100);
    let v = V3 { p: &p, c: &c };
    let f = v.create(&root, "f").unwrap();
    // Unchecked again: the same file; guarded: exists.
    assert_eq!(v.create(&root, "f").unwrap(), f);
    assert_eq!(
        v.create_how(&root, "f", t3::createhow3::GUARDED(mode(0o600))),
        Err(S::NFS3ERR_EXIST)
    );
    assert_eq!(v.create(&root, "."), Err(S::NFS3ERR_EXIST));
    let d = v.mkdir(&root, "d").unwrap();
    assert_eq!(v.getattr(&d).unwrap().mode, 0o705);
    assert_eq!(v.mkdir(&root, "d"), Err(S::NFS3ERR_EXIST));
    assert_eq!(v.create(&f, "x"), Err(S::NFS3ERR_NOTDIR));
    // MKNOD: a FIFO; devices and sockets are not supported.
    let fifo = t3::MKNOD3args {
        r#where: dirop(&root, "pipe"),
        what: t3::mknoddata3::NF3FIFO(mode(0o600)),
    };
    let pipe = match c.v3(&p, &Args::NFSPROC3_MKNOD(fifo)) {
        Res::NFSPROC3_MKNOD(t3::MKNOD3res::NFS3_OK(ok)) => ok.obj.unwrap().data,
        r => panic!("{r:?}"),
    };
    assert_eq!(v.getattr(&pipe).unwrap().r#type, t3::ftype3::NF3FIFO);
    for what in [
        t3::mknoddata3::NF3SOCK(no_sattr()),
        t3::mknoddata3::NF3CHR(t3::devicedata3 {
            dev_attributes: no_sattr(),
            spec: t3::specdata3 {
                specdata1: 1,
                specdata2: 3,
            },
        }),
    ] {
        let a = t3::MKNOD3args {
            r#where: dirop(&root, "dev"),
            what,
        };
        assert!(matches!(
            c.v3(&p, &Args::NFSPROC3_MKNOD(a)),
            Res::NFSPROC3_MKNOD(t3::MKNOD3res::default(S::NFS3ERR_NOTSUPP, _))
        ));
    }
    let a = t3::MKNOD3args {
        r#where: dirop(&root, "dev"),
        what: t3::mknoddata3::default(t3::ftype3::NF3REG),
    };
    assert!(matches!(
        c.v3(&p, &Args::NFSPROC3_MKNOD(a)),
        Res::NFSPROC3_MKNOD(t3::MKNOD3res::default(S::NFS3ERR_BADTYPE, _))
    ));
    // LINK, RENAME.
    let a = t3::LINK3args {
        file: fh(&f),
        link: dirop(&d, "f2"),
    };
    match c.v3(&p, &Args::NFSPROC3_LINK(a)) {
        Res::NFSPROC3_LINK(t3::LINK3res::NFS3_OK(ok)) => {
            assert_eq!(ok.file_attributes.unwrap().nlink, 2);
        }
        r => panic!("{r:?}"),
    }
    let a = t3::RENAME3args {
        from: dirop(&root, "f"),
        to: dirop(&d, "g"),
    };
    match c.v3(&p, &Args::NFSPROC3_RENAME(a)) {
        Res::NFSPROC3_RENAME(t3::RENAME3res::NFS3_OK(ok)) => {
            assert!(ok.fromdir_wcc.before.is_some() && ok.todir_wcc.after.is_some());
        }
        r => panic!("{r:?}"),
    }
    assert_eq!(v.lookup(&d, "g"), Ok(f.clone()));
    assert_eq!(v.lookup(&root, "f"), Err(S::NFS3ERR_NOENT));
    // REMOVE, RMDIR and their errors.
    assert_eq!(v.remove(&root, "d"), Err(S::NFS3ERR_ISDIR));
    let rmdir = |name: &str| match c.v3(
        &p,
        &Args::NFSPROC3_RMDIR(t3::RMDIR3args {
            object: dirop(&root, name),
        }),
    ) {
        Res::NFSPROC3_RMDIR(t3::RMDIR3res::NFS3_OK(_)) => Ok(()),
        Res::NFSPROC3_RMDIR(t3::RMDIR3res::default(s, _)) => Err(s),
        r => panic!("{r:?}"),
    };
    assert_eq!(rmdir("d"), Err(S::NFS3ERR_NOTEMPTY));
    assert_eq!(rmdir("pipe"), Err(S::NFS3ERR_NOTDIR));
    assert_eq!(rmdir("."), Err(S::NFS3ERR_INVAL));
    v.remove(&d, "g").unwrap();
    v.remove(&d, "f2").unwrap();
    assert_eq!(v.remove(&d, "f2"), Err(S::NFS3ERR_NOENT));
    rmdir("d").unwrap();
    // The removed file's handle is stale.
    assert_eq!(v.getattr(&f), Err(S::NFS3ERR_STALE));
}

#[test]
fn fsstat_fsinfo_pathconf() {
    let (w, root) = setup();
    let p = w.nfs3();
    let c = Client::new("10.0.0.5:900", 100);
    match c.v3(
        &p,
        &Args::NFSPROC3_FSSTAT(t3::FSSTAT3args { fsroot: fh(&root) }),
    ) {
        Res::NFSPROC3_FSSTAT(t3::FSSTAT3res::NFS3_OK(ok)) => {
            assert!(ok.tbytes >= ok.fbytes && ok.fbytes >= ok.abytes);
            assert!(ok.obj_attributes.is_some());
        }
        r => panic!("{r:?}"),
    }
    match c.v3(
        &p,
        &Args::NFSPROC3_FSINFO(t3::FSINFO3args { fsroot: fh(&root) }),
    ) {
        Res::NFSPROC3_FSINFO(t3::FSINFO3res::NFS3_OK(ok)) => {
            assert_eq!(
                (ok.rtmax, ok.wtmax, ok.rtpref, ok.wtpref),
                (1 << 20, 1 << 20, 65536, 65536)
            );
            assert_eq!(
                ok.properties,
                t3::FSF3_LINK | t3::FSF3_SYMLINK | t3::FSF3_HOMOGENEOUS | t3::FSF3_CANSETTIME
            );
        }
        r => panic!("{r:?}"),
    }
    match c.v3(
        &p,
        &Args::NFSPROC3_PATHCONF(t3::PATHCONF3args { object: fh(&root) }),
    ) {
        Res::NFSPROC3_PATHCONF(t3::PATHCONF3res::NFS3_OK(ok)) => {
            assert_eq!(ok.name_max, 255);
            assert!(ok.no_trunc && ok.case_preserving);
            assert!(
                ok.case_insensitive,
                "the root file system matches names without case"
            );
        }
        r => panic!("{r:?}"),
    }
    // A handle that is not ours.
    match c.v3(
        &p,
        &Args::NFSPROC3_FSINFO(t3::FSINFO3args {
            fsroot: fh(b"junk"),
        }),
    ) {
        Res::NFSPROC3_FSINFO(t3::FSINFO3res::default(s, _)) => assert_eq!(s, S::NFS3ERR_BADHANDLE),
        r => panic!("{r:?}"),
    }
}

#[test]
fn a_version_and_procedures_outside_nfs3() {
    let (w, _) = setup();
    let p = w.nfs3();
    let c = Client::new("10.0.0.5:900", 100);
    // Procedure 22 does not exist: PROC_UNAVAIL.
    let mut m = c.message(77, &Args::NFSPROC3_NULL);
    // The procedure number is the sixth word of the call.
    m[20..24].copy_from_slice(&22u32.to_be_bytes());
    let r = c.send(&p, &m).unwrap();
    assert!(matches!(
        nfs::rpc::client::results(&r),
        Err(nfs::rpc::RpcError::Refused(
            nfs::rpc::AcceptError::ProcUnavail
        ))
    ));
    // Version 2: PROG_MISMATCH 3 to 3.
    m[16..20].copy_from_slice(&2u32.to_be_bytes());
    m[20..24].copy_from_slice(&0u32.to_be_bytes());
    let r = c.send(&p, &m).unwrap();
    assert_eq!(
        nfs::rpc::client::results(&r).unwrap_err().mismatch(),
        Some((3, 3))
    );
}
