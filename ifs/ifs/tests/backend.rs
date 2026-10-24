// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One conformance run for every backend: `MemBackend` always, and
//! `LinuxBackend` (ignored: it needs root for `trusted.` attributes and
//! a file system that has them) over the empty directory named by
//! `QS400_IFS_TEST_DIR`. `ifs/ifs/tests/linux-xfs.sh` runs it as root
//! over an xfs loop image in WSL.

mod common;

use ifs::Errno;
use ifs::backend::{BFile, Backend, Kind};

/// The backend contract, as the IFS relies on it.
fn exercise<B: Backend>(b: &B) {
    b.mkdir("t", 0o755).unwrap();
    assert_eq!(b.mkdir("t", 0o755), Err(Errno::EEXIST));
    let f = b.create("t/a", 0o640).unwrap();
    assert_eq!(b.create("t/a", 0o640).err(), Some(Errno::EEXIST));
    assert_eq!(f.write_at(b"hello", 0).unwrap(), 5);
    f.write_at(b"!", 10).unwrap();
    assert_eq!(f.stat().unwrap().size, 11);
    let mut buf = [0u8; 16];
    assert_eq!(f.read_at(&mut buf, 0).unwrap(), 11);
    assert_eq!(&buf[..6], b"hello\0");
    f.set_len(5).unwrap();
    f.sync(true).unwrap();
    f.sync(false).unwrap();
    drop(f);
    let st = b.stat("t/a").unwrap();
    assert_eq!(
        (st.kind, st.size, st.nlink, st.mode & 0o777),
        (Kind::File, 5, 1, 0o640)
    );
    let g = b.open("t/a", true).unwrap();
    g.write_at(b"J", 0).unwrap();
    drop(g);
    assert_eq!(b.open("t", false).err(), Some(Errno::EISDIR));
    // Links.
    b.link("t/a", "t/b").unwrap();
    assert_eq!(b.stat("t/a").unwrap().nlink, 2);
    assert_eq!(b.stat("t/a").unwrap().ino, b.stat("t/b").unwrap().ino);
    b.symlink("../t/a", "t/l").unwrap();
    assert_eq!(b.readlink("t/l").unwrap(), "../t/a");
    assert_eq!(b.stat("t/l").unwrap().kind, Kind::Symlink);
    // Paths never go through a link.
    b.symlink(".", "t/self").unwrap();
    assert!(b.stat("t/self/a").is_err());
    b.mknod("t/fifo", Kind::Fifo, 0o600).unwrap();
    assert_eq!(b.stat("t/fifo").unwrap().kind, Kind::Fifo);
    assert_eq!(b.open("t/fifo", false).err(), Some(Errno::ENOTSUP));
    // Rename with and without replacing.
    assert_eq!(b.rename("t/a", "t/b", false), Err(Errno::EEXIST));
    b.rename("t/b", "t/c", false).unwrap();
    // Two links to one file: rename does nothing (POSIX).
    b.rename("t/c", "t/a", true).unwrap();
    assert_eq!(b.stat("t/a").unwrap().nlink, 2);
    b.unlink("t/c").unwrap();
    assert_eq!(b.stat("t/a").unwrap().nlink, 1);
    // Directory listing.
    let mut names: Vec<String> = b
        .readdir("t")
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    names.sort();
    assert_eq!(names, vec!["a", "fifo", "l", "self"]);
    assert_eq!(b.rmdir("t"), Err(Errno::ENOTEMPTY));
    // Extended attributes, on a file, a directory, a link and a FIFO.
    for p in ["t/a", "t", "t/l", "t/fifo"] {
        assert_eq!(b.xattr_get(p, "trusted.qs400.meta").unwrap(), None, "{p}");
        let big = vec![7u8; 3000];
        b.xattr_set(p, "trusted.qs400.meta", &big).unwrap();
        assert_eq!(
            b.xattr_get(p, "trusted.qs400.meta").unwrap(),
            Some(big),
            "{p}"
        );
        assert!(
            b.xattr_list(p)
                .unwrap()
                .contains(&"trusted.qs400.meta".to_string())
        );
        b.xattr_remove(p, "trusted.qs400.meta").unwrap();
        b.xattr_remove(p, "trusted.qs400.meta").unwrap();
    }
    // The mirrors.
    b.set_mode("t/a", 0o604).unwrap();
    b.set_owner("t/a", 100, 500).unwrap();
    b.set_owner("t/l", 100, 500).unwrap();
    let st = b.stat("t/a").unwrap();
    assert_eq!((st.mode & 0o7777, st.uid, st.gid), (0o604, 100, 500));
    b.set_times("t/a", Some(1_000_000_000), Some(2_000_000_123))
        .unwrap();
    let st = b.stat("t/a").unwrap();
    assert_eq!((st.atime, st.mtime), (1_000_000_000, 2_000_000_123));
    b.set_times("t/a", None, Some(5_000_000_000)).unwrap();
    assert_eq!(b.stat("t/a").unwrap().atime, 1_000_000_000);
    // Removal.
    for p in ["t/a", "t/l", "t/self", "t/fifo"] {
        b.unlink(p).unwrap();
    }
    assert_eq!(b.unlink("t"), Err(Errno::EISDIR));
    b.rmdir("t").unwrap();
    assert!(b.statfs().unwrap().blocks > 0);
}

#[test]
fn mem_backend_conformance() {
    exercise(&ifs::backend::mem::MemBackend::new());
}

/// The whole IFS over a backend: layout, records in the attribute,
/// mirrors, reclaim.
fn ifs_over<B: Backend>(b: B) {
    use ifs::ops::{Config, Ifs};
    let env = common::env();
    let ifs = Ifs::new(
        b,
        Box::new(ifs::vfs::qsys::MemQsys::new(sec::auth::bits::Auth::EXCLUDE)),
        Config::default(),
    );
    ifs.ensure_layout(&env).unwrap();
    let p = common::user("BOB");
    let sec = common::user("QSECOFR");
    let audit = ifs::ops::audit::NoAudit;
    let cx = |p| ifs::Ctx {
        owner: ifs::Owner(1),
        principal: p,
        env: &env,
        audit: &audit,
        job_ccsid: 37,
    };
    ifs.mkdir(&cx(&sec), "/home/bob", 0o755).unwrap();
    ifs.chgown(&cx(&sec), "/home/bob", "BOB", true).unwrap();
    let fd = ifs.creat(&cx(&p), "/home/BOB/Data.txt", 0o640).unwrap();
    ifs.write(&cx(&p), fd, b"x").unwrap();
    ifs.close(&cx(&p), fd).unwrap();
    let st = ifs.backend().stat("root/home/bob/Data.txt").unwrap();
    // No primary group: the group bits show as none.
    assert_eq!((st.uid, st.mode & 0o777), (100, 0o600));
    assert!(
        ifs.backend()
            .xattr_get("root/home/bob/Data.txt", ifs::meta::XATTR)
            .unwrap()
            .is_some()
    );
    assert_eq!(
        ifs.stat(&cx(&p), "/home/bob/data.TXT").unwrap().owner,
        "BOB"
    );
    let rep = ifs.rcllnk(&cx(&sec), false).unwrap();
    assert!(rep.repaired.is_empty(), "{rep:?}");
    ifs.remove_tree(&cx(&sec), "/home/bob").unwrap();
}

#[test]
fn ifs_over_mem() {
    ifs_over(ifs::backend::mem::MemBackend::new());
}

#[cfg(target_os = "linux")]
fn linux_dir() -> String {
    std::env::var("QS400_IFS_TEST_DIR").expect("QS400_IFS_TEST_DIR names an empty directory on xfs")
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "needs root and a file system with trusted xattrs (tests/linux-xfs.sh)"]
fn linux_backend_conformance() {
    exercise(&ifs::backend::linux::LinuxBackend::open(&linux_dir()).unwrap());
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "needs root and a file system with trusted xattrs (tests/linux-xfs.sh)"]
fn ifs_over_linux() {
    ifs_over(ifs::backend::linux::LinuxBackend::open(&linux_dir()).unwrap());
}
