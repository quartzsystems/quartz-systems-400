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
    // Extended attributes through the open file.
    assert_eq!(f.xattr_get("trusted.qs400.t").unwrap(), None);
    f.xattr_set("trusted.qs400.t", b"fd").unwrap();
    assert_eq!(
        f.xattr_get("trusted.qs400.t").unwrap(),
        Some(b"fd".to_vec())
    );
    assert_eq!(
        b.xattr_get("t/a", "trusted.qs400.t").unwrap(),
        Some(b"fd".to_vec())
    );
    b.xattr_remove("t/a", "trusted.qs400.t").unwrap();
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
    let made = ifs.fstat(&cx(&p), fd).unwrap().change;
    ifs.write(&cx(&p), fd, b"x").unwrap();
    // The change counter, through the open file and on the backend's top.
    assert!(ifs.fstat(&cx(&p), fd).unwrap().change > made);
    assert!(ifs::meta::change::mark(ifs.backend()).unwrap() > made);
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

/// File IDs over a backend: an inode number the backend reuses never
/// meets its old generation, a restart keeps generations rising, and an
/// ID is found again by walking once the remembered paths are gone.
fn ids_over<B: Backend + Clone>(b: B) {
    use ifs::ops::audit::FileId;
    use ifs::ops::{Config, Ifs};
    let env = common::env();
    let q = || Box::new(ifs::vfs::qsys::MemQsys::new(sec::auth::bits::Auth::EXCLUDE));
    let ifs = Ifs::new(b.clone(), q(), Config::default());
    ifs.ensure_layout(&env).unwrap();
    let sec = common::user("QSECOFR");
    let audit = ifs::ops::audit::NoAudit;
    let cx = ifs::Ctx {
        owner: ifs::Owner(1),
        principal: &sec,
        env: &env,
        audit: &audit,
        job_ccsid: 37,
    };
    let id_of = |ifs: &Ifs<B>, p: &str| {
        let s = ifs.stat(&cx, p).unwrap();
        FileId {
            fs: s.vfs,
            ino: s.ino,
            generation: s.ino_gen,
        }
    };
    ifs.mkdir(&cx, "/home/ids", 0o755).unwrap();
    let fd = ifs.creat(&cx, "/home/ids/first", 0o644).unwrap();
    ifs.close(&cx, fd).unwrap();
    let old = id_of(&ifs, "/home/ids/first");
    ifs.unlink(&cx, "/home/ids/first").unwrap();
    // Make files until the backend hands the number out again (xfs does
    // soon); the new object has another generation.
    for i in 0..200 {
        let p = format!("/home/ids/f{i}");
        let fd = ifs.creat(&cx, &p, 0o644).unwrap();
        ifs.close(&cx, fd).unwrap();
        let id = id_of(&ifs, &p);
        if id.ino == old.ino {
            assert_ne!(id.generation, old.generation);
            break;
        }
    }
    assert_eq!(ifs.stat_by_id(&cx, old).err(), Some(ifs::Errno::ESTALE));
    // After a restart.
    let again = Ifs::new(b, q(), Config::default());
    let kept = id_of(&again, "/home/ids/f0");
    again.mkdir(&cx, "/home/ids/later", 0o755).unwrap();
    let later = id_of(&again, "/home/ids/later");
    // The clock's generations rise; a backend with its own (xfs) has random ones.
    if again.backend().generation("").ok().flatten().is_none() {
        assert!(later.generation > kept.generation);
        assert!(later.generation > ifs::meta::generation::BLOCK);
    }
    again.forget_ids();
    assert_eq!(again.stat_by_id(&cx, kept).unwrap().0, "/home/ids/f0");
    again.remove_tree(&cx, "/home/ids").unwrap();
}

#[test]
fn ids_over_mem() {
    ids_over(ifs::backend::mem::MemBackend::new());
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "needs root and a file system with trusted xattrs (tests/linux-xfs.sh)"]
fn ids_over_linux() {
    let dir = linux_dir();
    let b = ifs::backend::linux::LinuxBackend::open(&dir).unwrap();
    ids_over(Shared(std::sync::Arc::new(b)));
}

/// A backend shared by two `Ifs` (a restart over the same volume).
#[cfg(target_os = "linux")]
#[derive(Clone)]
struct Shared(std::sync::Arc<ifs::backend::linux::LinuxBackend>);

#[cfg(target_os = "linux")]
impl Backend for Shared {
    type File = ifs::backend::linux::LinuxFile;
    fn stat(&self, p: &str) -> ifs::Result<ifs::backend::BStat> {
        self.0.stat(p)
    }
    fn readdir(&self, p: &str) -> ifs::Result<Vec<ifs::backend::DirEntry>> {
        self.0.readdir(p)
    }
    fn mkdir(&self, p: &str, mode: u32) -> ifs::Result<()> {
        self.0.mkdir(p, mode)
    }
    fn rmdir(&self, p: &str) -> ifs::Result<()> {
        self.0.rmdir(p)
    }
    fn create(&self, p: &str, mode: u32) -> ifs::Result<Self::File> {
        self.0.create(p, mode)
    }
    fn open(&self, p: &str, write: bool) -> ifs::Result<Self::File> {
        self.0.open(p, write)
    }
    fn mknod(&self, p: &str, kind: ifs::backend::Kind, mode: u32) -> ifs::Result<()> {
        self.0.mknod(p, kind, mode)
    }
    fn unlink(&self, p: &str) -> ifs::Result<()> {
        self.0.unlink(p)
    }
    fn rename(&self, from: &str, to: &str, replace: bool) -> ifs::Result<()> {
        self.0.rename(from, to, replace)
    }
    fn link(&self, existing: &str, new: &str) -> ifs::Result<()> {
        self.0.link(existing, new)
    }
    fn symlink(&self, target: &str, p: &str) -> ifs::Result<()> {
        self.0.symlink(target, p)
    }
    fn readlink(&self, p: &str) -> ifs::Result<String> {
        self.0.readlink(p)
    }
    fn set_times(
        &self,
        p: &str,
        atime: Option<ifs::backend::Nanos>,
        mtime: Option<ifs::backend::Nanos>,
    ) -> ifs::Result<()> {
        self.0.set_times(p, atime, mtime)
    }
    fn set_mode(&self, p: &str, mode: u32) -> ifs::Result<()> {
        self.0.set_mode(p, mode)
    }
    fn set_owner(&self, p: &str, uid: u32, gid: u32) -> ifs::Result<()> {
        self.0.set_owner(p, uid, gid)
    }
    fn xattr_get(&self, p: &str, name: &str) -> ifs::Result<Option<Vec<u8>>> {
        self.0.xattr_get(p, name)
    }
    fn xattr_set(&self, p: &str, name: &str, value: &[u8]) -> ifs::Result<()> {
        self.0.xattr_set(p, name, value)
    }
    fn xattr_list(&self, p: &str) -> ifs::Result<Vec<String>> {
        self.0.xattr_list(p)
    }
    fn xattr_remove(&self, p: &str, name: &str) -> ifs::Result<()> {
        self.0.xattr_remove(p, name)
    }
    fn statfs(&self) -> ifs::Result<ifs::backend::StatFs> {
        self.0.statfs()
    }
    fn generation(&self, p: &str) -> ifs::Result<Option<u32>> {
        self.0.generation(p)
    }
    fn path_of(&self, ino: u64, generation: u32) -> ifs::Result<Option<String>> {
        self.0.path_of(ino, generation)
    }
}

/// File IDs open by xfs handle: a new object's generation is xfs's, and
/// a directory and a file are found after the remembered paths are gone
/// without a walk; one removed is stale.
#[cfg(target_os = "linux")]
#[test]
#[ignore = "needs root and a file system with trusted xattrs (tests/linux-xfs.sh)"]
fn handles_over_linux() {
    use ifs::ops::audit::FileId;
    use ifs::ops::{Config, Ifs};
    let b = ifs::backend::linux::LinuxBackend::open(&linux_dir()).unwrap();
    assert!(b.handles(), "xfs gives handles of the expected form");
    let env = common::env();
    let ifs = Ifs::new(
        b,
        Box::new(ifs::vfs::qsys::MemQsys::new(sec::auth::bits::Auth::EXCLUDE)),
        Config::default(),
    );
    ifs.ensure_layout(&env).unwrap();
    let sec = common::user("QSECOFR");
    let audit = ifs::ops::audit::NoAudit;
    let cx = ifs::Ctx {
        owner: ifs::Owner(1),
        principal: &sec,
        env: &env,
        audit: &audit,
        job_ccsid: 37,
    };
    ifs.mkdir(&cx, "/home/fh", 0o755).unwrap();
    ifs.mkdir(&cx, "/home/fh/d", 0o755).unwrap();
    let fd = ifs.creat(&cx, "/home/fh/d/f", 0o644).unwrap();
    ifs.close(&cx, fd).unwrap();
    for (p, rel) in [
        ("/home/fh/d", "root/home/fh/d"),
        ("/home/fh/d/f", "root/home/fh/d/f"),
    ] {
        let s = ifs.stat(&cx, p).unwrap();
        assert_eq!(
            Some(s.ino_gen),
            ifs.backend().generation(rel).unwrap(),
            "{p}: the record holds xfs's generation"
        );
        let id = FileId {
            fs: s.vfs,
            ino: s.ino,
            generation: s.ino_gen,
        };
        assert_eq!(
            ifs.backend()
                .path_of(id.ino, id.generation)
                .unwrap()
                .as_deref(),
            Some(rel)
        );
        ifs.forget_ids();
        assert_eq!(ifs.stat_by_id(&cx, id).unwrap().0, p);
        assert_eq!(ifs.id_cache_len(), 1, "{p}: found without a walk");
    }
    let s = ifs.stat(&cx, "/home/fh/d/f").unwrap();
    let id = FileId {
        fs: s.vfs,
        ino: s.ino,
        generation: s.ino_gen,
    };
    ifs.unlink(&cx, "/home/fh/d/f").unwrap();
    assert_eq!(ifs.backend().path_of(id.ino, id.generation).unwrap(), None);
    assert_eq!(ifs.stat_by_id(&cx, id).err(), Some(ifs::Errno::ESTALE));
    ifs.remove_tree(&cx, "/home/fh").unwrap();
}
