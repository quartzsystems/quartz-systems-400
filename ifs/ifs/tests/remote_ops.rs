// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every IFS operation on a remote mount, over a remote file system in
//! memory: names, data, attributes, authority (the server's), links,
//! copies and moves across file systems, walks. No call reaches the
//! remote file system from the thread that calls the IFS.

mod remote_common;

use ifs::Errno;
use ifs::client::{IfsClient, read_all, write_all};
use ifs::ops::flags::{
    F_OK, O_APPEND, O_CREAT, O_EXCL, O_RDONLY, O_RDWR, O_TRUNC, O_WRONLY, R_OK, SEEK_END, W_OK,
    X_OK,
};
use ifs::ops::{Attr, AttrId, ChgAut, CopyOpts, OpenArgs, RenameMode, WalkOpts, Who};
use remote_common::mounted;

fn put(c: &dyn IfsClient, path: &str, data: &[u8]) {
    let fd = c
        .open(
            path,
            OpenArgs::new(O_WRONLY | O_CREAT | O_TRUNC).mode(0o644),
        )
        .unwrap();
    write_all(c, fd, data).unwrap();
    c.close(fd).unwrap();
}

fn get(c: &dyn IfsClient, path: &str) -> Vec<u8> {
    let fd = c.open(path, OpenArgs::new(O_RDONLY)).unwrap();
    let d = read_all(c, fd).unwrap();
    c.close(fd).unwrap();
    d
}

#[test]
fn names_and_data() {
    let r = mounted();
    let c = r.client("BOB", 1);
    c.mkdir("/mnt/d", 0o755).unwrap();
    assert_eq!(c.mkdir("/mnt/d", 0o755), Err(Errno::EEXIST));
    let st = c.stat("/mnt/d", true).unwrap();
    assert_eq!(st.objtype, "*DIR");
    assert_eq!(st.uid, 100);
    assert_eq!(st.owner, "BOB");
    assert_eq!(st.mode & 0o777, 0o755);
    put(&c, "/mnt/d/f.txt", b"hello, world");
    assert_eq!(
        r.server.file("d/f.txt").as_deref(),
        Some(&b"hello, world"[..])
    );
    assert_eq!(get(&c, "/mnt/d/f.txt"), b"hello, world");
    // Names keep their case and compare exactly.
    assert_eq!(c.stat("/mnt/D/f.txt", true), Err(Errno::ENOENT));
    let names: Vec<String> = c
        .read_dir("/mnt/d")
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(names, vec!["f.txt"]);
    c.rename("/mnt/d/f.txt", "/mnt/d/g.txt", RenameMode::Keep)
        .unwrap();
    assert!(r.server.exists("d/g.txt") && !r.server.exists("d/f.txt"));
    put(&c, "/mnt/d/h.txt", b"h");
    assert_eq!(
        c.rename("/mnt/d/g.txt", "/mnt/d/h.txt", RenameMode::Keep),
        Err(Errno::EEXIST)
    );
    c.rename("/mnt/d/g.txt", "/mnt/d/h.txt", RenameMode::Unlink)
        .unwrap();
    assert_eq!(get(&c, "/mnt/d/h.txt"), b"hello, world");
    // Across file systems: EXDEV.
    assert_eq!(
        c.rename("/mnt/d/h.txt", "/tmp/h.txt", RenameMode::Keep),
        Err(Errno::EXDEV)
    );
    assert_eq!(c.link("/mnt/d/h.txt", "/tmp/h2"), Err(Errno::EXDEV));
    c.link("/mnt/d/h.txt", "/mnt/d/hard").unwrap();
    assert_eq!(c.stat("/mnt/d/hard", true).unwrap().nlink, 2);
    c.unlink("/mnt/d/hard").unwrap();
    assert_eq!(c.stat("/mnt/d/hard", true), Err(Errno::ENOENT));
    assert_eq!(c.rmdir("/mnt/d"), Err(Errno::ENOTEMPTY));
    assert_eq!(c.unlink("/mnt/d"), Err(Errno::EISDIR));
    c.unlink("/mnt/d/h.txt").unwrap();
    c.rmdir("/mnt/d").unwrap();
    assert!(!r.server.exists("d"));
    // FIFOs are made; sockets are not.
    c.mkfifo("/mnt/fifo", 0o644).unwrap();
    assert_eq!(c.stat("/mnt/fifo", true).unwrap().objtype, "*FIFO");
    // No call came from this thread.
    assert!(!r.server.threads().contains(&std::thread::current().id()));
}

#[test]
fn links_resolve_on_the_client() {
    let r = mounted();
    let c = r.client("BOB", 1);
    c.mkdir("/mnt/a", 0o777).unwrap();
    put(&c, "/mnt/a/x", b"remote");
    c.symlink("a/x", "/mnt/rel").unwrap();
    assert_eq!(c.readlink("/mnt/rel").unwrap(), "a/x");
    assert_eq!(get(&c, "/mnt/rel"), b"remote");
    // An absolute target is the local root's.
    put(&r.client("QSECOFR", 2), "/tmp/local", b"local");
    c.symlink("/tmp/local", "/mnt/abs").unwrap();
    assert_eq!(get(&c, "/mnt/abs"), b"local");
    assert_eq!(c.stat("/mnt/abs", false).unwrap().objtype, "*SYMLNK");
    // `..` from the mount's root leaves it.
    c.chdir("/mnt/a").unwrap();
    assert_eq!(c.getcwd().unwrap(), "/mnt/a");
    assert_eq!(get(&c, "../a/x"), b"remote");
    assert_eq!(c.stat("../../tmp/local", true).unwrap().size, 5);
    c.chdir("/").unwrap();
}

#[test]
fn descriptors() {
    let r = mounted();
    let c = r.client("BOB", 1);
    let fd = c
        .open(
            "/mnt/f",
            OpenArgs::new(O_RDWR | O_CREAT | O_EXCL).mode(0o600),
        )
        .unwrap();
    assert_eq!(
        c.open(
            "/mnt/f",
            OpenArgs::new(O_RDWR | O_CREAT | O_EXCL).mode(0o600)
        ),
        Err(Errno::EEXIST)
    );
    assert_eq!(c.stat("/mnt/f", true).unwrap().mode & 0o777, 0o600);
    write_all(&c, fd, &[7u8; 3000]).unwrap();
    // The size is the client's while the data is not yet written back.
    assert_eq!(c.fstat(fd).unwrap().size, 3000);
    assert_eq!(c.stat("/mnt/f", true).unwrap().size, 3000);
    assert_eq!(c.lseek(fd, 0, SEEK_END).unwrap(), 3000);
    assert_eq!(c.pread(fd, 4, 2998).unwrap(), vec![7, 7]);
    c.ftruncate(fd, 10).unwrap();
    assert_eq!(c.fstat(fd).unwrap().size, 10);
    assert_eq!(c.fd_path(fd).unwrap(), "/mnt/f");
    c.fsync(fd, false).unwrap();
    assert_eq!(r.server.file("f").unwrap(), vec![7u8; 10]);
    c.close(fd).unwrap();
    let fd = c
        .open("/mnt/f", OpenArgs::new(O_WRONLY | O_APPEND))
        .unwrap();
    c.write(fd, b"++").unwrap();
    c.close(fd).unwrap();
    assert_eq!(r.server.file("f").unwrap().len(), 12);
    let fd = c.open("/mnt/f", OpenArgs::new(O_WRONLY | O_TRUNC)).unwrap();
    c.close(fd).unwrap();
    assert_eq!(r.server.file("f").unwrap().len(), 0);
    // A directory opens for reading only.
    let d = c.open("/mnt", OpenArgs::new(O_RDONLY)).unwrap();
    c.close(d).unwrap();
    assert_eq!(c.open("/mnt", OpenArgs::new(O_WRONLY)), Err(Errno::EISDIR));
}

#[test]
fn authority_is_the_servers() {
    let r = mounted();
    let bob = r.client("BOB", 1);
    let ann = r.client("ANN", 2);
    bob.mkdir("/mnt/private", 0o700).unwrap();
    put(&bob, "/mnt/private/p", b"secret");
    // QS/400 asks the server: ANN may not search BOB's directory.
    assert_eq!(ann.stat("/mnt/private/p", true), Err(Errno::EACCES));
    assert_eq!(ann.access("/mnt/private", X_OK), Err(Errno::EACCES));
    assert_eq!(bob.access("/mnt/private", R_OK | W_OK | X_OK), Ok(()));
    assert_eq!(ann.read_dir("/mnt/private"), Err(Errno::EACCES));
    bob.chmod("/mnt/private", 0o755).unwrap();
    assert_eq!(ann.stat("/mnt/private/p", true).unwrap().size, 6);
    assert_eq!(
        ann.open("/mnt/private/p", OpenArgs::new(O_WRONLY)),
        Err(Errno::EACCES)
    );
    // The server refuses ANN's chmod of BOB's file.
    assert_eq!(ann.chmod("/mnt/private/p", 0o777), Err(Errno::EPERM));
    // Group and *PUBLIC data authorities map onto the mode.
    bob.chgaut(
        "/mnt/private/p",
        &ChgAut {
            who: vec![Who::Public],
            data: Some(ifs::auth::data::DataAuth::parse("*RW").unwrap()),
            ..ChgAut::default()
        },
    )
    .unwrap();
    assert_eq!(bob.stat("/mnt/private/p", true).unwrap().mode & 0o7, 0o6);
    assert_eq!(
        bob.chgaut(
            "/mnt/private/p",
            &ChgAut {
                who: vec![Who::User("ANN".into())],
                data: Some(ifs::auth::data::DataAuth::parse("*R").unwrap()),
                ..ChgAut::default()
            },
        ),
        Err(Errno::ENOTSUP)
    );
    assert_eq!(bob.rvkaut("/mnt/private/p", "ANN"), Err(Errno::ENOTSUP));
    let a = bob.dspaut("/mnt/private/p").unwrap();
    assert_eq!(a.owner, "BOB");
    // Owner and group by wire UID and GID, or by profile.
    let sec = r.client("QSECOFR", 3);
    sec.chown("/mnt/private/p", 101, u32::MAX).unwrap();
    assert_eq!(sec.stat("/mnt/private/p", true).unwrap().owner, "ANN");
    sec.chgown("/mnt/private/p", "BOB", true).unwrap();
    assert_eq!(sec.stat("/mnt/private/p", true).unwrap().uid, 100);
    sec.chgpgp("/mnt/private/p", Some("DEV"), true).unwrap();
    assert_eq!(sec.stat("/mnt/private/p", true).unwrap().gid, 500);
    assert_eq!(
        sec.chgpgp("/mnt/private/p", None, true),
        Err(Errno::ENOTSUP)
    );
    assert_eq!(ann.access("/mnt/private/p", F_OK), Ok(()));
}

#[test]
fn ibm_attributes() {
    let r = mounted();
    let c = r.client("BOB", 1);
    put(&c, "/mnt/f", b"x");
    let attrs = c.get_attrs("/mnt/f", &[AttrId::LocalRemote], true).unwrap();
    assert_eq!(attrs, vec![Attr::LocalRemote(false)]);
    assert_eq!(
        c.get_attrs("/tmp", &[AttrId::LocalRemote], true).unwrap(),
        vec![Attr::LocalRemote(true)]
    );
    assert_eq!(
        c.set_attr("/mnt/f", &Attr::Ccsid(819), true),
        Err(Errno::ENOTSUP)
    );
    assert_eq!(
        c.set_attr("/mnt/f", &Attr::PcHidden(true), true),
        Err(Errno::ENOTSUP)
    );
    c.set_attr("/mnt/f", &Attr::Sgid(true), true).unwrap();
    assert_ne!(c.stat("/mnt/f", true).unwrap().mode & 0o2000, 0);
    assert_eq!(c.check_out("/mnt/f"), Err(Errno::ENOTSUP));
    assert_eq!(c.check_in("/mnt/f"), Err(Errno::ENOTSUP));
    assert_eq!(c.end_journal("/mnt/f"), Err(Errno::ENOTSUP));
    c.utime("/mnt/f", Some((1_000_000_000, 2_000_000_000)))
        .unwrap();
    let st = c.stat("/mnt/f", true).unwrap();
    assert_eq!((st.atime, st.mtime), (1_000_000_000, 2_000_000_000));
    c.utime("/mnt/f", None).unwrap();
    assert!(c.stat("/mnt/f", true).unwrap().mtime > 2_000_000_000);
    assert_eq!(c.access("/mnt/f", W_OK), Ok(()));
}

#[test]
fn copies_moves_trees_walks() {
    let r = mounted();
    let c = r.client("QSECOFR", 1);
    c.mkdir("/tmp/src", 0o755).unwrap();
    c.mkdir("/tmp/src/sub", 0o755).unwrap();
    put(&c, "/tmp/src/a", &vec![1u8; 5000]);
    put(&c, "/tmp/src/sub/b", b"bee");
    let tree = CopyOpts {
        subtree: true,
        ..CopyOpts::default()
    };
    // Local to remote.
    c.copy("/tmp/src", "/mnt/dst", tree).unwrap();
    assert_eq!(r.server.file("dst/a").unwrap(), vec![1u8; 5000]);
    assert_eq!(r.server.file("dst/sub/b").as_deref(), Some(&b"bee"[..]));
    // Remote to remote, remote to local.
    c.copy("/mnt/dst", "/mnt/dst2", tree).unwrap();
    assert_eq!(r.server.file("dst2/sub/b").as_deref(), Some(&b"bee"[..]));
    c.copy("/mnt/dst2", "/tmp/back", tree).unwrap();
    assert_eq!(get(&c, "/tmp/back/a"), vec![1u8; 5000]);
    // A move across file systems is a copy and a removal.
    c.mov("/mnt/dst2", "/tmp/moved", false).unwrap();
    assert!(!r.server.exists("dst2"));
    assert_eq!(get(&c, "/tmp/moved/sub/b"), b"bee");
    c.mov("/tmp/moved", "/mnt/moved", false).unwrap();
    assert!(r.server.exists("moved/sub/b"));
    // Within the mount it is a rename.
    c.mov("/mnt/moved", "/mnt/moved2", false).unwrap();
    let mut seen = Vec::new();
    for e in c
        .walk(
            "/mnt/moved2",
            WalkOpts {
                follow: false,
                one_fs: false,
            },
            100,
        )
        .unwrap()
    {
        assert!(e.stat.is_ok(), "{}", e.path);
        seen.push(e.path);
    }
    seen.sort();
    assert_eq!(
        seen,
        vec![
            "/mnt/moved2",
            "/mnt/moved2/a",
            "/mnt/moved2/sub",
            "/mnt/moved2/sub/b"
        ]
    );
    c.remove_tree("/mnt/moved2").unwrap();
    c.remove_tree("/mnt/dst").unwrap();
    assert!(r.server.names("").is_empty());
    assert!(!r.server.threads().contains(&std::thread::current().id()));
}

#[test]
fn read_only_and_nosuid_are_enforced_here() {
    let mut info = remote_common::info();
    info.nosuid = true;
    let r = remote_common::mounted_with(info, ifs::remote::RemoteConfig::default());
    let c = r.client("BOB", 1);
    put(&c, "/mnt/f", b"x");
    c.chmod("/mnt/f", 0o4755).unwrap();
    assert_eq!(c.stat("/mnt/f", true).unwrap().mode & 0o7777, 0o755);
    assert_eq!(c.statvfs("/mnt").unwrap().flag, 2);
    let r = remote_common::rig_with(remote_common::info(), ifs::remote::RemoteConfig::default());
    let sec = r.client("QSECOFR", 1);
    r.server.put_file("f", b"data");
    sec.mount_remote(
        "*NFS",
        "srv:/exp",
        "/mnt",
        "ro",
        ifs::remote::MountCcsid::default(),
    )
    .unwrap();
    let calls = r.server.calls();
    assert_eq!(sec.mkdir("/mnt/x", 0o777), Err(Errno::EROFS));
    assert_eq!(
        sec.open("/mnt/f", OpenArgs::new(O_WRONLY)),
        Err(Errno::EROFS)
    );
    assert_eq!(sec.access("/mnt/f", W_OK), Err(Errno::EROFS));
    assert_eq!(get(&sec, "/mnt/f"), b"data");
    assert_eq!(sec.unlink("/mnt/f"), Err(Errno::EROFS));
    assert!(r.server.calls() > calls);
    assert_eq!(sec.statvfs("/mnt").unwrap().flag & 1, 1);
}

#[test]
fn server_job_calls_on_a_remote_mount() {
    use ifs::client::Caller;
    use ifs::lock::range::RangeKind;
    use ifs::ops::audit::FileId;
    use ifs::ops::flags::{F_SETLK, SEEK_SET};
    let r = mounted();
    let c = r.client("QSECOFR", 1);
    // A call as another user sends that user's credentials.
    let bob = c
        .as_caller(&Caller::Profile { name: "BOB".into() })
        .unwrap();
    bob.mkdir("/mnt/asbob", 0o755).unwrap();
    assert_eq!(c.stat("/mnt/asbob", true).unwrap().uid, 100);
    // File IDs of remote objects are the server's: not searched here.
    let st = c.stat("/mnt/asbob", true).unwrap();
    let id = FileId {
        fs: st.vfs,
        ino: st.ino,
        generation: st.ino_gen,
    };
    assert_eq!(c.stat_by_id(id).map(|_| ()), Err(Errno::ENOTSUP));
    assert_eq!(
        c.open_by_id(id, OpenArgs::new(O_RDONLY)).map(|_| ()),
        Err(Errno::ENOTSUP)
    );
    // Locks go to the server as the job's; sub-owners' are refused.
    put(&c, "/mnt/l", b"lock me");
    let fd = c.open("/mnt/l", OpenArgs::new(O_RDWR)).unwrap();
    assert_eq!(
        c.lock_range_as(fd, 0, F_SETLK, RangeKind::Write, SEEK_SET, 0, 4),
        Ok(None)
    );
    assert_eq!(r.server.calls_of("lock"), 1);
    assert_eq!(
        c.lock_range_as(fd, 7, F_SETLK, RangeKind::Write, SEEK_SET, 0, 4),
        Err(Errno::ENOTSUP)
    );
    assert_eq!(
        c.lock_wait(fd, 0, RangeKind::Write, SEEK_SET, 0, 4),
        Err(Errno::ENOTSUP)
    );
    c.close(fd).unwrap();
}
