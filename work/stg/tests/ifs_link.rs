// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A job's IFS calls end to end: `RemoteIfs` over a link whose far end
//! answers `ifs` requests as the storage service does, with
//! `stg::ifs::serve` over an `Ifs<MemBackend>`; every call crosses the
//! wire codec both ways.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use ifs::auth::MemEnv;
use ifs::backend::mem::MemBackend;
use ifs::client::{IfsClient, read_all, write_all};
use ifs::lock::range::RangeKind;
use ifs::ops::audit::{AuditKind, MemAudit};
use ifs::ops::flags::{
    F_SETLK, O_CCSID, O_CREAT, O_RDONLY, O_RDWR, O_SHARE_NONE, O_TEXTDATA, O_TRUNC, O_WRONLY,
    SEEK_SET,
};
use ifs::ops::{Config, Ctx, Ifs, OpenArgs, Owner, WalkOpts};
use ifs::vfs::qsys::{MemQsys, MemberData};
use ifs::{Errno, Owner as IfsOwner};
use sec::auth::bits::Auth;
use sec::auth::principal::Principal;
use sec::profile::spcaut::SpecialAuth;
use stg::ifs::serve;
use stg::remote::RemoteIfs;
use stg::{Link, Request};

/// The far end: the service's state for one system.
struct Far {
    ifs: Arc<Ifs<MemBackend>>,
    env: MemEnv,
    audit: Arc<MemAudit>,
}

fn far() -> Far {
    let q = MemQsys::new(Auth::USE);
    q.add_lib("QGPL");
    q.add_obj("QGPL", "QCLSRC", "*FILE", "PF-SRC", "QSYS");
    q.add_member(
        "QGPL",
        "QCLSRC",
        "HELLO",
        MemberData {
            records: vec![
                ccsid::to_ebcdic(&format!("{:<20}", "PGM")).unwrap(),
                ccsid::to_ebcdic(&format!("{:<20}", "ENDPGM")).unwrap(),
            ],
            record_len: 20,
            source: true,
            ccsid: 37,
        },
    );
    let env = MemEnv::default()
        .user("QSYS", 0)
        .user("QSECOFR", 1)
        .user("BOB", 100)
        .user("ANN", 101)
        .user("QDFTOWN", 3);
    let ifs = Ifs::new(MemBackend::new(), Box::new(q), Config::default());
    ifs.ensure_layout(&env).unwrap();
    Far {
        ifs: Arc::new(ifs),
        env,
        audit: Arc::new(MemAudit::default()),
    }
}

fn principal(user: &str) -> Principal {
    let mut p = Principal {
        user: user.into(),
        qsecurity: 40,
        objaud: "*NONE".into(),
        ..Principal::default()
    };
    if user == "QSECOFR" {
        p.spcaut = SpecialAuth::ALLOBJ.0;
    }
    p
}

/// A job's connection: a link whose far end serves `user`'s requests
/// as owner `owner`, exactly as the service does per connection.
fn connect(far: &Far, user: &str, owner: u64) -> RemoteIfs {
    let ifs = Arc::clone(&far.ifs);
    let env = far.env.clone();
    let audit = Arc::clone(&far.audit);
    let p = principal(user);
    let link = Link::answering(move |req| match req {
        Request::IfsReq { op } => {
            let cx = Ctx {
                owner: Owner(owner),
                principal: &p,
                env: &env,
                audit: &*audit,
                job_ccsid: 37,
            };
            let direct = ifs::client::Direct { ifs: &ifs, cx: &cx };
            ipc::encode(&serve(&direct, op))
        }
        other => panic!("not an IFS request: {}", other.name()),
    });
    RemoteIfs::new(Rc::new(RefCell::new(link)))
}

/// `/home/bob` made by the security officer and given to BOB, as
/// `CRTUSRPRF HOMEDIR` leaves it; `/home` is `*PUBLIC *RX`.
fn home(far: &Far, mode: u32) {
    let sec = connect(far, "QSECOFR", 99);
    sec.mkdir("/home/bob", mode).unwrap();
    sec.chgown("/home/bob", "BOB", true).unwrap();
    sec.chmod("/home/bob", mode).unwrap();
    far.ifs.release_owner(IfsOwner(99));
}

#[test]
fn files_over_the_link() {
    let far = far();
    let c = connect(&far, "BOB", 1);
    assert_eq!(c.mkdir("/home/bob", 0o755), Err(Errno::EACCES));
    home(&far, 0o755);
    let fd = c
        .open(
            "/home/bob/a.txt",
            OpenArgs::new(O_WRONLY | O_CREAT).mode(0o644),
        )
        .unwrap();
    assert_eq!(fd, 0);
    // More than one chunk goes in several writes.
    let big = vec![0x41u8; ifs::client::CHUNK + 10];
    write_all(&c, fd, &big).unwrap();
    c.close(fd).unwrap();
    let st = c.stat("/home/bob/a.txt", true).unwrap();
    assert_eq!(st.size, big.len() as u64);
    assert_eq!(st.objtype, "*STMF");
    assert_eq!(st.owner, "BOB");
    assert_eq!(st.uid, 100);
    assert_eq!(st.ccsid, 1208);
    let fd = c.open("/home/bob/a.txt", OpenArgs::new(O_RDONLY)).unwrap();
    assert_eq!(read_all(&c, fd).unwrap(), big);
    assert_eq!(c.lseek(fd, 5, SEEK_SET).unwrap(), 5);
    assert_eq!(c.read(fd, 3).unwrap(), b"AAA");
    assert_eq!(c.pread(fd, 2, big.len() as u64 - 1).unwrap(), b"A");
    assert_eq!(c.fstat(fd).unwrap().size, big.len() as u64);
    let d = c.dup(fd).unwrap();
    assert_eq!(c.fd_path(d).unwrap(), "/home/bob/a.txt");
    c.close(d).unwrap();
    c.close(fd).unwrap();
    assert_eq!(c.close(fd), Err(Errno::EBADF));
    // Directory streams, batched.
    c.symlink("a.txt", "/home/bob/link").unwrap();
    assert_eq!(c.readlink("/home/bob/link").unwrap(), "a.txt");
    let h = c.opendir("/home/bob").unwrap();
    let mut names = Vec::new();
    loop {
        let batch = c.readdir(h, 2).unwrap();
        if batch.is_empty() {
            break;
        }
        assert!(batch.len() <= 2);
        names.extend(batch.into_iter().map(|e| e.name));
    }
    c.closedir(h).unwrap();
    names.sort();
    assert_eq!(names, vec![".", "..", "a.txt", "link"]);
    assert_eq!(
        c.read_dir("/home/bob").unwrap().len(),
        2,
        "read_dir leaves out . and .."
    );
    c.chdir("/home/bob").unwrap();
    assert_eq!(c.getcwd().unwrap(), "/home/bob");
    assert_eq!(c.stat("link", false).unwrap().objtype, "*SYMLNK");
    assert_eq!(c.umask(0o077).unwrap(), 0o022);
    let v = c.statvfs("/").unwrap();
    assert_eq!(v.name, "root");
    let walk = c.walk("/home", WalkOpts::default(), 10).unwrap();
    assert_eq!(walk[0].path, "/home");
    assert!(walk.iter().any(|w| w.path == "/home/bob/a.txt"));
    c.unlink("/home/bob/link").unwrap();
    assert_eq!(c.stat("/home/bob/link", false), Err(Errno::ENOENT));
    assert_eq!(
        c.stat("/home/bob/link", false).unwrap_err().message_id(),
        "CPE3025"
    );
}

#[test]
fn locks_end_with_the_connection() {
    let far = far();
    let a = connect(&far, "BOB", 1);
    let b = connect(&far, "ANN", 2);
    let fd = a
        .open("/tmp/shared", OpenArgs::new(O_RDWR | O_CREAT).mode(0o666))
        .unwrap();
    write_all(&a, fd, b"0123456789").unwrap();
    // The umask took the public's write; give it back.
    a.chmod("/tmp/shared", 0o666).unwrap();
    a.lock_range(fd, F_SETLK, RangeKind::Write, SEEK_SET, 0, 4)
        .unwrap();
    let fdb = b.open("/tmp/shared", OpenArgs::new(O_RDWR)).unwrap();
    assert_eq!(
        b.lock_range(fdb, F_SETLK, RangeKind::Write, SEEK_SET, 2, 2),
        Err(Errno::EAGAIN)
    );
    // Another open refused by a share mode.
    let fdx = a
        .open(
            "/tmp/x",
            OpenArgs::new(O_WRONLY | O_CREAT | O_SHARE_NONE).mode(0o666),
        )
        .unwrap();
    assert_eq!(b.open("/tmp/x", OpenArgs::new(O_RDONLY)), Err(Errno::EBUSY));
    assert_eq!(a.object_locks("/tmp/shared").unwrap().len(), 3);
    assert_eq!(a.owner_locks().unwrap().len(), 3);
    // BOB's connection closes without closing anything.
    far.ifs.release_owner(IfsOwner(1));
    b.lock_range(fdb, F_SETLK, RangeKind::Write, SEEK_SET, 2, 2)
        .unwrap();
    let fdx2 = b.open("/tmp/x", OpenArgs::new(O_RDONLY)).unwrap();
    b.close(fdx2).unwrap();
    b.close(fdb).unwrap();
    // The old descriptors are gone with the owner.
    assert_eq!(a.close(fd), Err(Errno::EBADF));
    assert_eq!(a.close(fdx), Err(Errno::EBADF));
    assert_eq!(a.getcwd().unwrap(), "/");
}

#[test]
fn refusals_are_audited() {
    let far = far();
    let bob = connect(&far, "BOB", 1);
    let ann = connect(&far, "ANN", 2);
    home(&far, 0o700);
    let fd = bob
        .open(
            "/home/bob/secret",
            OpenArgs::new(O_WRONLY | O_CREAT).mode(0o600),
        )
        .unwrap();
    bob.close(fd).unwrap();
    assert_eq!(
        ann.open("/home/bob/secret", OpenArgs::new(O_RDONLY)),
        Err(Errno::EACCES)
    );
    assert_eq!(ann.mkdir("/home/bob/sub", 0o755), Err(Errno::EACCES));
    // She cannot search /home/bob, so she never reaches the file.
    assert_eq!(ann.chmod("/home/bob/secret", 0o644), Err(Errno::EACCES));
    // A file she reaches but does not own: only its owner changes it.
    let fd = bob
        .open("/tmp/bobs", OpenArgs::new(O_WRONLY | O_CREAT).mode(0o644))
        .unwrap();
    bob.close(fd).unwrap();
    assert_eq!(ann.chmod("/tmp/bobs", 0o666), Err(Errno::EPERM));
    let events = far.audit.take();
    let fails: Vec<&str> = events
        .iter()
        .filter(|e| e.kind == AuditKind::AuthFail)
        .map(|e| e.path.as_str())
        .collect();
    assert!(fails.contains(&"/home/bob"), "{fails:?}");
    assert!(
        events
            .iter()
            .all(|e| e.user == "ANN" || e.kind != AuditKind::AuthFail)
    );
    // What the journal would get.
    let af = events
        .iter()
        .find(|e| e.kind == AuditKind::AuthFail)
        .unwrap();
    assert_eq!(af.violation, Some('A'));
    assert_eq!(af.objtype, "*DIR");
    // Errors carry the message the command would send.
    assert_eq!(Errno::EACCES.command_message(), "CPFA09C");
}

#[test]
fn members_through_the_store_view() {
    let far = far();
    let c = connect(&far, "QSECOFR", 1);
    let m = "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/HELLO.MBR";
    let st = c.stat(m, true).unwrap();
    assert_eq!(st.objtype, "*MBR");
    assert_eq!(st.size, 2 * 22);
    let fd = c
        .open(m, OpenArgs::new(O_RDONLY | O_TEXTDATA | O_CCSID).conv(1208))
        .unwrap();
    assert_eq!(
        read_all(&c, fd).unwrap(),
        format!("{:<20}\n{:<20}\n", "PGM", "ENDPGM").as_bytes()
    );
    c.close(fd).unwrap();
    let fd = c
        .open(
            m,
            OpenArgs::new(O_WRONLY | O_TRUNC | O_TEXTDATA | O_CCSID).conv(1208),
        )
        .unwrap();
    write_all(&c, fd, b"PGM\nDCL &A *CHAR 10\nENDPGM\n").unwrap();
    c.close(fd).unwrap();
    assert_eq!(c.stat(m, true).unwrap().size, 3 * 22);
    let fd = c.open(m, OpenArgs::new(O_RDONLY)).unwrap();
    let raw = read_all(&c, fd).unwrap();
    c.close(fd).unwrap();
    assert_eq!(raw.len(), 60);
    assert_eq!(
        ccsid::from_ebcdic(&raw[20..40]).trim_end(),
        "DCL &A *CHAR 10"
    );
    let names: Vec<String> = c
        .read_dir("/QSYS.LIB/QGPL.LIB/QCLSRC.FILE")
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(names, vec!["HELLO.MBR"]);
    let fd = c
        .open(
            "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/NEW.MBR",
            OpenArgs::new(O_WRONLY | O_CREAT),
        )
        .unwrap();
    c.close(fd).unwrap();
    assert_eq!(
        c.read_dir("/QSYS.LIB/QGPL.LIB/QCLSRC.FILE").unwrap().len(),
        2
    );
    c.unlink("/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/NEW.MBR").unwrap();
    c.mkdir("/QSYS.LIB/NEWLIB.LIB", 0o755).unwrap();
    assert_eq!(
        c.stat("/QSYS.LIB/NEWLIB.LIB", true).unwrap().objtype,
        "*LIB"
    );
    c.rmdir("/QSYS.LIB/NEWLIB.LIB").unwrap();
}

#[test]
fn wrong_shaped_replies_are_unknown() {
    let link = Link::answering(|_| ipc::encode(&stg::ifs::IfsReply::Unit { r: Ok(()) }));
    let c = RemoteIfs::new(Rc::new(RefCell::new(link)));
    assert_eq!(c.getcwd(), Err(Errno::EUNKNOWN));
}
