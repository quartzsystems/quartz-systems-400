// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Rename rules, copy and move, user-defined file systems, the walk,
//! file IDs, attributes, journaling, `RCLLNK` and the save walk.

#![allow(clippy::many_single_char_names)]

mod common;

use common::{sys, user};
use ifs::Errno;
use ifs::backend::Backend;
use ifs::meta::reclaim::{Problem, Repair};
use ifs::meta::{CrtObjAud, ObjAudit};
use ifs::ops::flags::{O_RDONLY, PC_LINK_MAX, PC_NAME_MAX, PC_PATH_MAX};
use ifs::ops::{Attr, AttrId, CopyOpts, OpenArgs, RenameMode, Visit, WalkOpts};
use ifs::save::{Descriptor, SaveSink};

#[test]
fn rename_rules() {
    let s = sys();
    let p = user("QSECOFR");
    let cx = s.cx(&p, 1);
    s.put(&cx, "/tmp/a", b"a");
    s.put(&cx, "/tmp/b", b"b");
    assert_eq!(
        s.ifs.rename(&cx, "/tmp/a", "/tmp/b", RenameMode::Keep),
        Err(Errno::EEXIST)
    );
    s.ifs
        .rename(&cx, "/tmp/a", "/tmp/b", RenameMode::Unlink)
        .unwrap();
    assert_eq!(s.get(&cx, "/tmp/b"), b"a");
    assert_eq!(
        s.ifs
            .rename(&cx, "/tmp/b", "/QOpenSys/tmp/b", RenameMode::Keep),
        Err(Errno::EXDEV)
    );
    s.ifs.mkdir(&cx, "/tmp/d", 0o755).unwrap();
    s.ifs.mkdir(&cx, "/tmp/d/e", 0o755).unwrap();
    assert_eq!(
        s.ifs.rename(&cx, "/tmp/d", "/tmp/d/e/f", RenameMode::Keep),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        s.ifs.rename(&cx, "/tmp/b", "/tmp/d", RenameMode::Unlink),
        Err(Errno::EISDIR)
    );
    assert_eq!(
        s.ifs.rename(&cx, "/QOpenSys", "/x", RenameMode::Keep),
        Err(Errno::EBUSY)
    );
    assert_eq!(s.ifs.rmdir(&cx, "/tmp/d"), Err(Errno::ENOTEMPTY));
    assert_eq!(s.ifs.rmdir(&cx, "/tmp/b"), Err(Errno::ENOTDIR));
    assert_eq!(s.ifs.unlink(&cx, "/tmp/d"), Err(Errno::EISDIR));
    assert_eq!(s.ifs.rmdir(&cx, "/QOpenSys"), Err(Errno::EBUSY));
    assert_eq!(s.ifs.mkdir(&cx, "/tmp/d", 0o755), Err(Errno::EEXIST));
    assert_eq!(s.ifs.mkdir(&cx, "/", 0o755), Err(Errno::EEXIST));
    let ev = s.audit.take();
    assert!(
        ev.iter()
            .any(|e| e.kind == ifs::ops::audit::AuditKind::Move && e.detail == "/tmp/b")
    );
}

#[test]
fn copy_and_move_across_file_systems() {
    let s = sys();
    let p = user("QSECOFR");
    let cx = s.cx(&p, 1);
    s.ifs.mkdir(&cx, "/home/src", 0o755).unwrap();
    s.ifs.mkdir(&cx, "/home/src/sub", 0o755).unwrap();
    s.put(&cx, "/home/src/sub/f.txt", b"hello\n");
    s.ifs.symlink(&cx, "sub/f.txt", "/home/src/l").unwrap();
    s.ifs
        .set_attr(
            &cx,
            "/home/src/sub/f.txt",
            &Attr::Text("A file".into()),
            true,
        )
        .unwrap();
    s.ifs
        .copy(
            &cx,
            "/home/src",
            "/QOpenSys/tmp/dst",
            CopyOpts {
                subtree: true,
                ..CopyOpts::default()
            },
        )
        .unwrap();
    assert_eq!(s.get(&cx, "/QOpenSys/tmp/dst/sub/f.txt"), b"hello\n");
    assert_eq!(
        s.ifs.readlink(&cx, "/QOpenSys/tmp/dst/l").unwrap(),
        "sub/f.txt"
    );
    let a = s
        .ifs
        .get_attrs(&cx, "/QOpenSys/tmp/dst/sub/f.txt", &[AttrId::Text], true)
        .unwrap();
    assert_eq!(a, vec![Attr::Text("A file".into())]);
    // Without REPLACE an existing target stops the copy.
    assert_eq!(
        s.ifs.copy(
            &cx,
            "/home/src/sub/f.txt",
            "/QOpenSys/tmp/dst/sub/f.txt",
            CopyOpts::default()
        ),
        Err(Errno::EEXIST)
    );
    // DTAFMT(*TEXT) with a target CCSID.
    s.ifs
        .copy(
            &cx,
            "/home/src/sub/f.txt",
            "/tmp/e.txt",
            CopyOpts {
                to_ccsid: Some(37),
                ..CopyOpts::default()
            },
        )
        .unwrap();
    assert_eq!(
        s.get(&cx, "/tmp/e.txt"),
        ccsid::to_ebcdic("hello\n").unwrap()
    );
    assert_eq!(s.ifs.stat(&cx, "/tmp/e.txt").unwrap().ccsid, 37);
    // Copy a stream file into a member and back.
    s.ifs
        .copy(
            &cx,
            "/tmp/e.txt",
            "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/E.MBR",
            CopyOpts::default(),
        )
        .unwrap();
    assert_eq!(
        ccsid::from_ebcdic(&s.get(&cx, "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/E.MBR")),
        "hello\n"
    );
    // Move within a file system renames; across, copies and removes.
    s.ifs.mov(&cx, "/home/src", "/home/moved", false).unwrap();
    assert!(s.ifs.stat(&cx, "/home/moved/sub/f.txt").is_ok());
    s.ifs
        .mov(&cx, "/home/moved", "/QOpenSys/tmp/moved", false)
        .unwrap();
    assert_eq!(s.ifs.stat(&cx, "/home/moved"), Err(Errno::ENOENT));
    assert_eq!(s.get(&cx, "/QOpenSys/tmp/moved/sub/f.txt"), b"hello\n");
}

#[test]
fn user_defined_file_systems() {
    let s = sys();
    let p = user("QSECOFR");
    let cx = s.cx(&p, 1);
    s.ifs
        .crtudfs(
            &cx,
            "/dev/QASP01/Mine.udfs",
            false,
            CrtObjAud::Value(ObjAudit::Change),
            "Mine",
        )
        .unwrap();
    assert_eq!(
        s.ifs
            .crtudfs(&cx, "/dev/qasp01/MINE.UDFS", true, CrtObjAud::SysVal, ""),
        Err(Errno::EEXIST)
    );
    s.ifs
        .crtudfs(&cx, "/dev/QASP01/cs.udfs", true, CrtObjAud::SysVal, "")
        .unwrap();
    let names: Vec<String> = s
        .ifs
        .read_dir(&cx, "/dev/QASP01")
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(names, vec!["Mine.udfs", "cs.udfs"]);
    assert_eq!(
        s.ifs.stat(&cx, "/dev/QASP01/Mine.udfs").unwrap().objtype,
        "*BLKSF"
    );
    s.ifs.mkdir(&cx, "/mnt", 0o755).unwrap();
    s.ifs.mkdir(&cx, "/mnt2", 0o755).unwrap();
    let bob = user("BOB");
    assert_eq!(
        s.ifs
            .mount(&s.cx(&bob, 2), "/dev/QASP01/Mine.udfs", "/mnt", false),
        Err(Errno::EPERM)
    );
    let id = s
        .ifs
        .mount(&cx, "/dev/QASP01/Mine.udfs", "/mnt", false)
        .unwrap();
    s.ifs
        .mount(&cx, "/dev/QASP01/cs.udfs", "/mnt2", false)
        .unwrap();
    assert_eq!(
        s.ifs.mount(&cx, "/dev/QASP01/cs.udfs", "/mnt2", false),
        Err(Errno::EBUSY)
    );
    s.put(&cx, "/mnt/File", b"in udfs");
    assert_eq!(s.ifs.stat(&cx, "/mnt/file").unwrap().vfs, id);
    // New objects take the UDFS root's *CRTOBJAUD.
    let a = s
        .ifs
        .get_attrs(&cx, "/mnt/file", &[AttrId::Audit], true)
        .unwrap();
    assert_eq!(a, vec![Attr::Audit(ObjAudit::Change)]);
    s.put(&cx, "/mnt2/A", b"1");
    assert_eq!(s.ifs.stat(&cx, "/mnt2/a"), Err(Errno::ENOENT));
    assert_eq!(s.ifs.link(&cx, "/mnt/File", "/home/x"), Err(Errno::EXDEV));
    let info = s.ifs.dspudfs(&cx, "/dev/QASP01/mine.udfs").unwrap();
    assert_eq!(info.mounted_at, vec!["/mnt".to_string()]);
    assert!(!info.case_sensitive);
    assert_eq!(info.text, "Mine");
    // Busy while open or a current directory.
    let fd = s
        .ifs
        .open(&cx, "/mnt/File", OpenArgs::new(O_RDONLY))
        .unwrap();
    assert_eq!(s.ifs.unmount(&cx, "/mnt"), Err(Errno::EBUSY));
    s.ifs.close(&cx, fd).unwrap();
    s.ifs.chdir(&cx, "/mnt").unwrap();
    assert_eq!(s.ifs.unmount(&cx, "/mnt"), Err(Errno::EBUSY));
    s.ifs.chdir(&cx, "/").unwrap();
    assert_eq!(
        s.ifs.dltudfs(&cx, "/dev/QASP01/Mine.udfs"),
        Err(Errno::EBUSY)
    );
    s.ifs.unmount(&cx, "/dev/QASP01/Mine.udfs").unwrap();
    assert_eq!(s.ifs.stat(&cx, "/mnt/File"), Err(Errno::ENOENT));
    // Mounted again, the data is still there.
    s.ifs
        .mount(&cx, "/dev/QASP01/Mine.udfs", "/mnt2/A", false)
        .map(|_| ())
        .unwrap_err();
    s.ifs
        .mount(&cx, "/dev/QASP01/Mine.udfs", "/mnt", true)
        .unwrap();
    assert_eq!(s.get(&cx, "/mnt/File"), b"in udfs");
    assert_eq!(s.ifs.creat(&cx, "/mnt/new", 0o644), Err(Errno::EROFS));
    s.ifs.unmount(&cx, "/mnt").unwrap();
    s.ifs.dltudfs(&cx, "/dev/QASP01/Mine.udfs").unwrap();
    assert_eq!(s.ifs.stat(&cx, "/dev/QASP01/Mine.udfs"), Err(Errno::ENOENT));
}

#[test]
#[allow(clippy::too_many_lines)]
fn walk_file_ids_and_attributes() {
    let s = sys();
    let p = user("QSECOFR");
    let cx = s.cx(&p, 1);
    s.ifs.mkdir(&cx, "/home/w", 0o755).unwrap();
    s.ifs.mkdir(&cx, "/home/w/b", 0o755).unwrap();
    s.put(&cx, "/home/w/a", b"1");
    s.put(&cx, "/home/w/b/c", b"2");
    s.ifs.symlink(&cx, "/home/w", "/home/w/b/up").unwrap();
    let mut seen = Vec::new();
    s.ifs
        .walk(&cx, "/home/w", WalkOpts::default(), &mut |p, st| {
            seen.push((p.to_string(), st.unwrap().objtype.clone()));
            Visit::Continue
        })
        .unwrap();
    let paths: Vec<&str> = seen.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(
        paths,
        vec![
            "/home/w",
            "/home/w/a",
            "/home/w/b",
            "/home/w/b/c",
            "/home/w/b/up"
        ]
    );
    // Following links never loops.
    let mut n = 0;
    s.ifs
        .walk(
            &cx,
            "/home/w",
            WalkOpts {
                follow: true,
                one_fs: false,
            },
            &mut |_, _| {
                n += 1;
                Visit::Continue
            },
        )
        .unwrap();
    assert_eq!(n, 5);
    // Skip and stop.
    let mut n = 0;
    s.ifs
        .walk(&cx, "/home/w", WalkOpts::default(), &mut |p, _| {
            n += 1;
            if p.ends_with("/b") {
                Visit::Skip
            } else {
                Visit::Continue
            }
        })
        .unwrap();
    assert_eq!(n, 3);
    // File IDs.
    let attrs = s
        .ifs
        .get_attrs(&cx, "/home/w/b/c", &[AttrId::FileId], true)
        .unwrap();
    let Attr::FileId(id) = attrs[0] else { panic!() };
    assert_eq!(s.ifs.path_from_file_id(&cx, id).unwrap(), "/home/w/b/c");
    let wrong = ifs::ops::audit::FileId {
        generation: id.generation + 99,
        ..id
    };
    assert_eq!(s.ifs.path_from_file_id(&cx, wrong), Err(Errno::ENOENT));
    // Attributes.
    let all = s.ifs.get_attrs(&cx, "/home/w/a", &[], true).unwrap();
    assert!(all.contains(&Attr::ObjType("*STMF".into())));
    assert!(all.contains(&Attr::Ccsid(1208)));
    assert!(all.contains(&Attr::Alwsav(true)));
    s.ifs
        .set_attr(&cx, "/home/w/a", &Attr::PcHidden(true), true)
        .unwrap();
    s.ifs
        .set_attr(&cx, "/home/w/a", &Attr::Ccsid(819), true)
        .unwrap();
    s.ifs
        .set_attr(&cx, "/home/w/a", &Attr::PcReadOnly(true), true)
        .unwrap();
    let a = s
        .ifs
        .get_attrs(
            &cx,
            "/home/w/a",
            &[AttrId::PcHidden, AttrId::Ccsid, AttrId::Codepage],
            true,
        )
        .unwrap();
    assert_eq!(
        a,
        vec![Attr::PcHidden(true), Attr::Codepage(819), Attr::Ccsid(819)]
    );
    assert_eq!(
        s.ifs
            .open(&cx, "/home/w/a", OpenArgs::new(ifs::ops::flags::O_WRONLY)),
        Err(Errno::EROOBJ)
    );
    assert_eq!(
        s.ifs.set_attr(&cx, "/home/w/a", &Attr::Ccsid(4242), true),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        s.ifs.set_attr(&cx, "/home/w/a", &Attr::DataSize(1), true),
        Err(Errno::EINVAL)
    );
    // Journaling is stored.
    let j = ifs::meta::Journal {
        journal: "QGPL/J".into(),
        ..ifs::meta::Journal::default()
    };
    s.ifs.start_journal(&cx, "/home/w/b/c", j.clone()).unwrap();
    assert_eq!(
        s.ifs.start_journal(&cx, "/home/w/b/c", j),
        Err(Errno::EJOURNALED)
    );
    s.ifs.end_journal(&cx, "/home/w/b/c").unwrap();
    assert_eq!(
        s.ifs.end_journal(&cx, "/home/w/b/c"),
        Err(Errno::EJRNINACTIVE)
    );
    // utime.
    s.ifs
        .utime(&cx, "/home/w/b/c", Some((1_000_000_000, 2_000_000_000)))
        .unwrap();
    let st = s.ifs.stat(&cx, "/home/w/b/c").unwrap();
    assert_eq!((st.atime, st.mtime), (1_000_000_000, 2_000_000_000));
    let zed = user("ZED");
    assert_eq!(
        s.ifs.utime(&s.cx(&zed, 3), "/home/w/b/c", Some((0, 0))),
        Err(Errno::EPERM)
    );
    // pathconf and statvfs.
    assert_eq!(
        s.ifs.pathconf(&cx, "/home", PC_NAME_MAX).unwrap(),
        Some(255)
    );
    assert_eq!(
        s.ifs.pathconf(&cx, "/QSYS.LIB", PC_LINK_MAX).unwrap(),
        Some(1)
    );
    assert_eq!(
        s.ifs.pathconf(&cx, "/home", PC_PATH_MAX).unwrap(),
        Some(16 * 1024 * 1024)
    );
    assert_eq!(s.ifs.pathconf(&cx, "/home", 99), Err(Errno::EINVAL));
    let v = s.ifs.statvfs(&cx, "/home").unwrap();
    assert!(v.blocks > 0 && v.bfree > 0);
    assert_eq!(s.ifs.statvfs(&cx, "/QOPT").unwrap().flag, 1);
}

#[test]
fn reclaim_repairs_records() {
    let s = sys();
    let p = user("QSECOFR");
    let cx = s.cx(&p, 1);
    s.put(&cx, "/tmp/ok", b"x");
    // A crash between creating a node and writing its record.
    s.ifs.backend().fail_xattr_set(true);
    assert_eq!(
        s.ifs.creat(&cx, "/tmp/half", 0o644).map(|_| ()),
        Err(Errno::EIO)
    );
    s.ifs.backend().fail_xattr_set(false);
    // The node is there without a record: it gets the fallback.
    let st = s.ifs.stat(&cx, "/tmp/half").unwrap();
    assert_eq!(st.owner, "QSYS", "UID 0 maps to QSYS");
    // A garbled record, a stray attribute, a private authority of a
    // deleted profile, a broken mirror.
    s.ifs
        .backend()
        .xattr_set("root/tmp/ok", ifs::meta::XATTR, b"junk")
        .unwrap();
    s.put(&cx, "/tmp/p", b"p");
    s.ifs
        .chgaut(
            &cx,
            "/tmp/p",
            &ifs::ops::ChgAut {
                who: vec![ifs::ops::Who::User("ZED".into())],
                data: Some(ifs::auth::data::DataAuth(4)),
                ..ifs::ops::ChgAut::default()
            },
        )
        .unwrap();
    s.ifs
        .backend()
        .xattr_set("root/tmp/p", "trusted.qs400.old", b"x")
        .unwrap();
    s.ifs.backend().set_mode("root/tmp", 0o777).unwrap();
    // A name that folds like another, made behind the IFS's back.
    drop(s.ifs.backend().create("root/tmp/OK", 0o644).unwrap());
    let mut env = s.env.clone();
    env.profiles.retain(|p| p.0 != "ZED");
    let s2 = common::Sys {
        ifs: s.ifs,
        env,
        audit: ifs::ops::audit::MemAudit::default(),
    };
    let cx = s2.cx(&p, 1);
    let bob = user("BOB");
    assert_eq!(
        s2.ifs.rcllnk(&s2.cx(&bob, 2), false).map(|_| ()),
        Err(Errno::EPERM)
    );
    let check = s2.ifs.rcllnk(&cx, true).unwrap();
    let rep = s2.ifs.rcllnk(&cx, false).unwrap();
    assert_eq!(check.repaired, rep.repaired);
    let has = |path: &str, r: &Repair| rep.repaired.iter().any(|(p, x)| p == path && x == r);
    assert!(has("/tmp/half", &Repair::MissingMeta), "{rep:?}");
    assert!(has("/tmp/ok", &Repair::InvalidMeta));
    assert!(has("/tmp/p", &Repair::Private("ZED".into())));
    assert!(has(
        "/tmp/p",
        &Repair::StrayAttr("trusted.qs400.old".into())
    ));
    assert!(has("/tmp", &Repair::Mirror));
    assert!(
        rep.problems
            .iter()
            .any(|(p, x)| p == "/tmp/ok" && *x == Problem::FoldCollision("OK".into()))
    );
    // A second run finds nothing to repair.
    let again = s2.ifs.rcllnk(&cx, false).unwrap();
    assert!(
        again.repaired.iter().all(|(p, _)| p == "/tmp/OK"),
        "{again:?}"
    );
    assert!(again.objects > 10);
    // The exact name still wins over the one that folds like it.
    assert_eq!(s2.get(&cx, "/tmp/ok"), b"x");
    assert!(s2.get(&cx, "/tmp/OK").is_empty());
}

struct Sink(Vec<Descriptor>, Vec<u8>);

impl SaveSink for Sink {
    fn descriptor(&mut self, d: &Descriptor) -> ifs::Result<()> {
        self.0.push(Descriptor::decode(&d.encode())?);
        Ok(())
    }
    fn data(&mut self, chunk: &[u8]) -> ifs::Result<()> {
        self.1.extend_from_slice(chunk);
        Ok(())
    }
}

#[test]
fn save_walk() {
    let s = sys();
    let p = user("QSECOFR");
    let cx = s.cx(&p, 1);
    s.ifs.mkdir(&cx, "/home/s", 0o755).unwrap();
    s.put(&cx, "/home/s/a", b"aaa");
    s.ifs.link(&cx, "/home/s/a", "/home/s/b").unwrap();
    s.ifs.symlink(&cx, "a", "/home/s/l").unwrap();
    s.put(&cx, "/home/s/nosave", b"n");
    s.ifs
        .set_attr(&cx, "/home/s/nosave", &Attr::Alwsav(false), true)
        .unwrap();
    let mut sink = Sink(Vec::new(), Vec::new());
    assert_eq!(s.ifs.save(&cx, "/home/s", &mut sink).unwrap(), 4);
    let d: Vec<(&str, &str)> = sink
        .0
        .iter()
        .map(|d| (d.path.as_str(), d.objtype.as_str()))
        .collect();
    assert_eq!(
        d,
        vec![
            ("/home/s", "*DIR"),
            ("/home/s/a", "*STMF"),
            ("/home/s/b", "*STMF"),
            ("/home/s/l", "*SYMLNK")
        ]
    );
    assert_eq!(sink.0[2].hard_link_of.as_deref(), Some("/home/s/a"));
    assert_eq!(sink.0[3].link_target.as_deref(), Some("a"));
    assert_eq!(sink.1, b"aaa");
    assert_eq!(
        ifs::meta::codec::decode(&sink.0[1].meta).unwrap().owner,
        "QSECOFR"
    );
    // Saving needs *OBJEXIST or *SAVSYS.
    let zed = user("ZED");
    let mut sink = Sink(Vec::new(), Vec::new());
    assert_eq!(
        s.ifs.save(&s.cx(&zed, 3), "/home/s", &mut sink),
        Err(Errno::EACCES)
    );
}
