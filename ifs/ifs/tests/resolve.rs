// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The resolver: folding per mount, the casefold index, mounts, `..`,
//! symbolic links and their loops, hard links, the views.

mod common;

use common::{sys, user};
use ifs::Errno;
use ifs::backend::Backend;
use ifs::ops::flags::{O_CREAT, O_EXCL, O_RDONLY, O_RDWR};
use ifs::ops::{OpenArgs, RenameMode};
use ifs::path::SYMLOOP_MAX;

#[test]
fn root_folds_and_keeps_case() {
    let s = sys();
    let sec = user("QSECOFR");
    let cx = s.cx(&sec, 1);
    s.ifs.mkdir(&cx, "/home/Bob", 0o755).unwrap();
    s.put(&cx, "/home/bob/ReadMe.TXT", b"hi");
    assert_eq!(s.get(&cx, "/HOME/BOB/readme.txt"), b"hi");
    // Listing shows the stored case.
    let names: Vec<String> = s
        .ifs
        .read_dir(&cx, "/home/BOB")
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(names, vec!["ReadMe.TXT"]);
    // Creating under another case is the same name.
    assert_eq!(
        s.ifs.open(
            &cx,
            "/home/bob/README.txt",
            OpenArgs::new(O_RDWR | O_CREAT | O_EXCL).mode(0o644)
        ),
        Err(Errno::EEXIST)
    );
    let fd = s
        .ifs
        .open(
            &cx,
            "/home/bob/README.txt",
            OpenArgs::new(O_RDWR | O_CREAT).mode(0o644),
        )
        .unwrap();
    s.ifs.close(&cx, fd).unwrap();
    assert_eq!(s.ifs.read_dir(&cx, "/home/bob").unwrap().len(), 1);
    // A case-only rename keeps one entry with the new case.
    s.ifs
        .rename(
            &cx,
            "/home/bob/readme.txt",
            "/home/bob/README.txt",
            RenameMode::Keep,
        )
        .unwrap();
    let names: Vec<String> = s
        .ifs
        .read_dir(&cx, "/home/bob")
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(names, vec!["README.txt"]);
    assert_eq!(s.ifs.getcwd(&cx), "/");
    s.ifs.chdir(&cx, "/HOME/bob").unwrap();
    assert_eq!(s.ifs.getcwd(&cx), "/home/Bob");
}

#[test]
fn qopensys_is_case_sensitive() {
    let s = sys();
    let sec = user("QSECOFR");
    let cx = s.cx(&sec, 1);
    s.put(&cx, "/qopensys/usr/a", b"lower");
    s.put(&cx, "/QOpenSys/usr/A", b"upper");
    assert_eq!(s.get(&cx, "/QOpenSys/usr/a"), b"lower");
    assert_eq!(s.get(&cx, "/QOpenSys/usr/A"), b"upper");
    // The mount point folds (it is in root), names below do not.
    assert_eq!(s.ifs.stat(&cx, "/QOPENSYS/USR/a"), Err(Errno::ENOENT));
    assert_eq!(s.ifs.statvfs(&cx, "/QOpenSys").unwrap().name, "QOpenSys");
    assert!(s.ifs.statvfs(&cx, "/QOpenSys").unwrap().case_sensitive);
    assert!(!s.ifs.statvfs(&cx, "/home").unwrap().case_sensitive);
}

#[test]
fn mounts_show_in_root_and_dotdot_crosses_them() {
    let s = sys();
    let sec = user("QSECOFR");
    let cx = s.cx(&sec, 1);
    let names: Vec<String> = s
        .ifs
        .read_dir(&cx, "/")
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    for n in [
        "QOpenSys",
        "QSYS.LIB",
        "QOPT",
        "QNTC",
        "QDLS",
        "QFileSvr.400",
        "dev",
        "home",
        "tmp",
        "QIBM",
        "etc",
    ] {
        assert!(names.contains(&n.to_string()), "{n} in {names:?}");
    }
    s.ifs.chdir(&cx, "/QOpenSys/usr").unwrap();
    s.ifs.chdir(&cx, "../..").unwrap();
    assert_eq!(s.ifs.getcwd(&cx), "/");
    assert_eq!(s.ifs.stat(&cx, "/QOpenSys/..").unwrap().vfs, 1);
    assert_eq!(
        s.ifs.stat(&cx, "/QOpenSys/dev/null/").map(|_| ()),
        Err(Errno::ENOTDIR)
    );
    assert_eq!(
        s.ifs.stat(&cx, "/QOpenSys/dev/null").unwrap().objtype,
        "*CHRSF"
    );
    assert_eq!(s.ifs.stat(&cx, "/../../home").unwrap().objtype, "*DIR");
}

#[test]
fn unsupported_file_systems() {
    let s = sys();
    let sec = user("QSECOFR");
    let cx = s.cx(&sec, 1);
    for fs in ["/QNTC", "/QFileSvr.400", "/QDLS"] {
        assert_eq!(s.ifs.stat(&cx, fs).unwrap().objtype, "*DIR");
        assert_eq!(s.ifs.stat(&cx, &format!("{fs}/x")), Err(Errno::ENOTSUP));
        assert_eq!(
            s.ifs.mkdir(&cx, &format!("{fs}/x"), 0o755),
            Err(Errno::ENOTSUP)
        );
        assert_eq!(s.ifs.opendir(&cx, fs), Err(Errno::ENOTSUP));
        assert_eq!(Errno::ENOTSUP.command_message(), "CPFA0AD");
    }
    // /QOPT is there, empty and read-only.
    assert!(s.ifs.read_dir(&cx, "/QOPT").unwrap().is_empty());
    assert_eq!(s.ifs.mkdir(&cx, "/QOPT/V", 0o755), Err(Errno::EROFS));
}

#[test]
fn symbolic_links() {
    let s = sys();
    let sec = user("QSECOFR");
    let cx = s.cx(&sec, 1);
    s.ifs.mkdir(&cx, "/home/a", 0o755).unwrap();
    s.ifs.mkdir(&cx, "/home/a/b", 0o755).unwrap();
    s.put(&cx, "/home/a/b/f", b"data");
    // Relative and absolute links.
    s.ifs.symlink(&cx, "a/b", "/home/rel").unwrap();
    s.ifs.symlink(&cx, "/home/a/b/f", "/home/abs").unwrap();
    assert_eq!(s.get(&cx, "/home/rel/f"), b"data");
    assert_eq!(s.get(&cx, "/home/abs"), b"data");
    assert_eq!(s.ifs.lstat(&cx, "/home/abs").unwrap().objtype, "*SYMLNK");
    assert_eq!(s.ifs.stat(&cx, "/home/abs").unwrap().objtype, "*STMF");
    assert_eq!(s.ifs.readlink(&cx, "/home/rel").unwrap(), "a/b");
    assert_eq!(s.ifs.readlink(&cx, "/home/a"), Err(Errno::EINVAL));
    // `..` after a followed link is the target's parent.
    assert_eq!(
        s.ifs.stat(&cx, "/home/rel/..").unwrap().ino,
        s.ifs.stat(&cx, "/home/a").unwrap().ino
    );
    // A link into another file system.
    s.ifs.symlink(&cx, "/QOpenSys/usr", "/home/qos").unwrap();
    assert_eq!(s.ifs.stat(&cx, "/home/qos").unwrap().vfs, 2);
    // Loops.
    s.ifs.symlink(&cx, "loop2", "/home/loop1").unwrap();
    s.ifs.symlink(&cx, "loop1", "/home/loop2").unwrap();
    assert_eq!(s.ifs.stat(&cx, "/home/loop1"), Err(Errno::ELOOP));
    assert_eq!(Errno::ELOOP.command_message(), "CPFA0A3");
    // A chain just under the limit resolves, one more does not.
    s.put(&cx, "/home/end", b"e");
    let mut prev = "end".to_string();
    for i in 0..SYMLOOP_MAX {
        let name = format!("c{i}");
        s.ifs.symlink(&cx, &prev, &format!("/home/{name}")).unwrap();
        prev = name;
    }
    assert_eq!(s.get(&cx, &format!("/home/{prev}")), b"e");
    s.ifs.symlink(&cx, &prev, "/home/over").unwrap();
    assert_eq!(s.ifs.stat(&cx, "/home/over"), Err(Errno::ELOOP));
    // Dangling.
    s.ifs.symlink(&cx, "/nowhere", "/home/dangle").unwrap();
    assert_eq!(s.ifs.stat(&cx, "/home/dangle"), Err(Errno::ENOENT));
    assert!(s.ifs.lstat(&cx, "/home/dangle").is_ok());
    // O_NOFOLLOW on a link.
    assert_eq!(
        s.ifs.open(
            &cx,
            "/home/abs",
            OpenArgs::new(O_RDONLY | ifs::ops::flags::O_NOFOLLOW)
        ),
        Err(Errno::ELOOP)
    );
    // A file in the middle of a path.
    assert_eq!(s.ifs.stat(&cx, "/home/a/b/f/x"), Err(Errno::ENOTDIR));
}

#[test]
fn hard_links() {
    let s = sys();
    let sec = user("QSECOFR");
    let cx = s.cx(&sec, 1);
    s.put(&cx, "/home/one", b"1");
    s.ifs.link(&cx, "/home/one", "/home/two").unwrap();
    assert_eq!(s.ifs.stat(&cx, "/home/one").unwrap().nlink, 2);
    s.put(&cx, "/home/two", b"22");
    assert_eq!(s.get(&cx, "/home/one"), b"22");
    // The record is the inode's: a change through one link shows in both.
    s.ifs.chmod(&cx, "/home/two", 0o600).unwrap();
    assert_eq!(s.ifs.stat(&cx, "/home/one").unwrap().mode & 0o777, 0o600);
    s.ifs.unlink(&cx, "/home/one").unwrap();
    assert_eq!(s.ifs.stat(&cx, "/home/two").unwrap().nlink, 1);
    // Across file systems and to directories.
    assert_eq!(
        s.ifs.link(&cx, "/home/two", "/QOpenSys/usr/two"),
        Err(Errno::EXDEV)
    );
    assert_eq!(s.ifs.link(&cx, "/home", "/tmp/h"), Err(Errno::EPERM));
    // QOpenSys has them too.
    s.put(&cx, "/QOpenSys/usr/x", b"x");
    s.ifs
        .link(&cx, "/QOpenSys/usr/x", "/QOpenSys/usr/X")
        .unwrap();
    assert_eq!(s.ifs.stat(&cx, "/QOpenSys/usr/X").unwrap().nlink, 2);
}

#[test]
fn casefold_index_follows_changes() {
    let s = sys();
    let sec = user("QSECOFR");
    let cx = s.cx(&sec, 1);
    s.ifs.mkdir(&cx, "/home/d", 0o755).unwrap();
    s.put(&cx, "/home/d/Alpha", b"1");
    assert!(s.ifs.stat(&cx, "/home/d/ALPHA").is_ok());
    s.ifs
        .rename(&cx, "/home/d/alpha", "/home/d/Beta", RenameMode::Keep)
        .unwrap();
    assert_eq!(s.ifs.stat(&cx, "/home/d/alpha"), Err(Errno::ENOENT));
    assert!(s.ifs.stat(&cx, "/home/d/BETA").is_ok());
    s.ifs.unlink(&cx, "/home/d/beta").unwrap();
    assert_eq!(s.ifs.stat(&cx, "/home/d/Beta"), Err(Errno::ENOENT));
    // A directory renamed: names below it resolve under the new name.
    s.ifs.mkdir(&cx, "/home/d/Sub", 0o755).unwrap();
    s.put(&cx, "/home/d/sub/Leaf", b"l");
    s.ifs.chdir(&cx, "/home/d/SUB").unwrap();
    s.ifs
        .rename(&cx, "/home/d/sub", "/home/d/Moved", RenameMode::Keep)
        .unwrap();
    assert_eq!(s.get(&cx, "/home/d/moved/LEAF"), b"l");
    assert_eq!(s.ifs.getcwd(&cx), "/home/d/Moved");
    assert_eq!(s.get(&cx, "leaf"), b"l");
    // Made behind the crate's back.
    drop(s.ifs.backend().create("root/home/d/Late", 0o644).unwrap());
    assert!(s.ifs.stat(&cx, "/home/d/late").is_ok());
}

#[test]
fn qsys_lib_view() {
    let s = sys();
    let bob = user("BOB");
    let cx = s.cx(&bob, 2);
    let names: Vec<String> = s
        .ifs
        .read_dir(&cx, "/QSYS.LIB")
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert!(names.contains(&"QGPL.LIB".to_string()));
    assert!(names.contains(&"QSYSOPR.MSGQ".to_string()));
    let st = s
        .ifs
        .stat(&cx, "/qsys.lib/qgpl.lib/qclsrc.file/hello.mbr")
        .unwrap();
    assert_eq!(st.objtype, "*MBR");
    assert_eq!(
        s.ifs.stat(&cx, "/QSYS.LIB/QGPL.LIB").unwrap().objtype,
        "*LIB"
    );
    assert_eq!(
        s.ifs
            .stat(&cx, "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE")
            .unwrap()
            .objtype,
        "*FILE"
    );
    assert_eq!(
        s.ifs.stat(&cx, "/QSYS.LIB/QSYSOPR.MSGQ").unwrap().objtype,
        "*MSGQ"
    );
    assert_eq!(s.ifs.stat(&cx, "/QSYS.LIB/NOPE.LIB"), Err(Errno::ENOENT));
    assert_eq!(
        s.ifs.stat(&cx, "/QSYS.LIB/QGPL.LIB/X.BADTYPE"),
        Err(Errno::ENOENT)
    );
    // Members read as their lines, EBCDIC with 0x25 line ends.
    let raw = s.get(&cx, "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/HELLO.MBR");
    assert_eq!(ccsid::from_ebcdic(&raw), "PGM\nENDPGM\n");
    // BOB has *USE (the view's public): no writes.
    assert_eq!(
        s.ifs.open(
            &cx,
            "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/HELLO.MBR",
            OpenArgs::new(O_RDWR)
        ),
        Err(Errno::EACCES)
    );
    // BOB owns SPACE: a user space is a byte stream.
    let fd = s
        .ifs
        .open(
            &cx,
            "/QSYS.LIB/QGPL.LIB/SPACE.USRSPC",
            OpenArgs::new(O_RDWR),
        )
        .unwrap();
    s.ifs.write(&cx, fd, &[1, 2, 3]).unwrap();
    s.ifs.close(&cx, fd).unwrap();
    assert_eq!(s.get(&cx, "/QSYS.LIB/QGPL.LIB/SPACE.USRSPC"), vec![1, 2, 3]);
}

#[test]
fn bulk_creation_keeps_the_index() {
    let s = sys();
    let sec = user("QSECOFR");
    let cx = s.cx(&sec, 1);
    s.ifs.mkdir(&cx, "/home/many", 0o755).unwrap();
    let (_, before) = s.ifs.index_stats();
    for i in 0..300 {
        s.put(&cx, &format!("/home/many/File{i}"), b"x");
    }
    for i in 0..300 {
        assert!(s.ifs.stat(&cx, &format!("/home/MANY/file{i}")).is_ok());
    }
    let (_, after) = s.ifs.index_stats();
    // One build for the new directory, not one per file.
    assert!(after - before <= 3, "{} builds", after - before);
}
