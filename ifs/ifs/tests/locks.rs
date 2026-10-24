// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Share modes at open, byte-range locks through descriptors, check-out,
//! and what an owner's end releases.

#![allow(clippy::many_single_char_names)]

mod common;

use common::{sys, user};
use ifs::Errno;
use ifs::lock::range::RangeKind;
use ifs::lock::share::Share;
use ifs::lock::{Held, Owner};
use ifs::ops::OpenArgs;
use ifs::ops::flags::{
    F_GETLK, F_SETLK, F_SETLKW, O_RDONLY, O_RDWR, O_SHARE_NONE, O_SHARE_RDONLY, O_SHARE_RDWR,
    O_SHARE_WRONLY, O_WRONLY, SEEK_SET,
};

#[test]
fn share_mode_matrix_through_open() {
    let s = sys();
    let p = user("QSECOFR");
    let a = s.cx(&p, 1);
    let b = s.cx(&p, 2);
    s.put(&a, "/tmp/s", b"x");
    // (first access + share, second access + share, second allowed)
    let rows = [
        (O_RDONLY | O_SHARE_RDONLY, O_RDONLY | O_SHARE_RDWR, true),
        (O_RDONLY | O_SHARE_RDONLY, O_WRONLY | O_SHARE_RDWR, false),
        (O_WRONLY | O_SHARE_WRONLY, O_WRONLY | O_SHARE_RDWR, true),
        (O_WRONLY | O_SHARE_WRONLY, O_RDONLY | O_SHARE_RDWR, false),
        (O_WRONLY | O_SHARE_WRONLY, O_WRONLY | O_SHARE_WRONLY, true),
        (O_RDWR | O_SHARE_NONE, O_RDONLY | O_SHARE_RDWR, false),
        (O_RDONLY | O_SHARE_RDWR, O_RDWR | O_SHARE_NONE, false),
        (O_RDONLY, O_RDWR, true),
        (O_RDWR | O_SHARE_RDWR, O_RDONLY | O_SHARE_RDONLY, false),
        (O_RDONLY | O_SHARE_RDONLY, O_RDONLY | O_SHARE_RDONLY, true),
    ];
    for (first, second, ok) in rows {
        let f1 = s.ifs.open(&a, "/tmp/s", OpenArgs::new(first)).unwrap();
        let r = s.ifs.open(&b, "/tmp/s", OpenArgs::new(second));
        assert_eq!(r.is_ok(), ok, "{first:o} then {second:o}");
        if let Ok(f2) = r {
            s.ifs.close(&b, f2).unwrap();
        } else {
            assert_eq!(r, Err(Errno::EBUSY));
        }
        s.ifs.close(&a, f1).unwrap();
    }
    assert_eq!(
        s.ifs.open(
            &a,
            "/tmp/s",
            OpenArgs::new(O_RDONLY | O_SHARE_NONE | O_SHARE_RDONLY)
        ),
        Err(Errno::EINVAL)
    );
    // WRKOBJLCK sees the opens.
    let f1 = s
        .ifs
        .open(&a, "/tmp/s", OpenArgs::new(O_RDONLY | O_SHARE_RDONLY))
        .unwrap();
    let locks = s.ifs.object_locks(&a, "/tmp/s").unwrap();
    assert_eq!(locks.len(), 1);
    assert_eq!(locks[0].owner, Owner(1));
    assert!(matches!(locks[0].what, Held::Open(_, Share::ReadOnly)));
    // The owner's end releases its open: a writer gets in.
    s.ifs.release_owner(Owner(1));
    let _ = f1;
    let f2 = s
        .ifs
        .open(&b, "/tmp/s", OpenArgs::new(O_WRONLY | O_SHARE_NONE))
        .unwrap();
    s.ifs.close(&b, f2).unwrap();
}

#[test]
fn byte_range_locks() {
    let s = sys();
    let p = user("QSECOFR");
    let a = s.cx(&p, 1);
    let b = s.cx(&p, 2);
    s.put(&a, "/tmp/r", &[0u8; 100]);
    let fa = s.ifs.open(&a, "/tmp/r", OpenArgs::new(O_RDWR)).unwrap();
    let fb = s.ifs.open(&b, "/tmp/r", OpenArgs::new(O_RDWR)).unwrap();
    s.ifs
        .lock_range(&a, fa, F_SETLK, RangeKind::Write, SEEK_SET, 10, 10)
        .unwrap();
    assert_eq!(
        s.ifs
            .lock_range(&b, fb, F_SETLK, RangeKind::Read, SEEK_SET, 15, 1),
        Err(Errno::EAGAIN)
    );
    let held = s
        .ifs
        .lock_range(&b, fb, F_GETLK, RangeKind::Read, SEEK_SET, 0, 0)
        .unwrap()
        .unwrap();
    assert_eq!((held.owner, held.start, held.len()), (Owner(1), 10, 10));
    // Outside the range is fine; whole-file read locks share.
    s.ifs
        .lock_range(&b, fb, F_SETLK, RangeKind::Read, SEEK_SET, 20, 0)
        .unwrap();
    assert_eq!(
        s.ifs
            .lock_range(&a, fa, F_SETLK, RangeKind::Write, SEEK_SET, 50, 1),
        Err(Errno::EAGAIN)
    );
    // F_SETLKW: try again later, then deadlock detection.
    assert_eq!(
        s.ifs
            .lock_range(&b, fb, F_SETLKW, RangeKind::Write, SEEK_SET, 10, 1),
        Err(Errno::EAGAIN)
    );
    assert_eq!(
        s.ifs
            .lock_range(&a, fa, F_SETLKW, RangeKind::Write, SEEK_SET, 25, 1),
        Err(Errno::EDEADLK)
    );
    // Unlock part; then B's wait is satisfied.
    s.ifs
        .lock_range(&a, fa, F_SETLK, RangeKind::Unlock, SEEK_SET, 10, 5)
        .unwrap();
    s.ifs
        .lock_range(&b, fb, F_SETLKW, RangeKind::Write, SEEK_SET, 10, 1)
        .unwrap();
    // Closing any descriptor of the object drops the owner's locks.
    let fa2 = s.ifs.open(&a, "/tmp/r", OpenArgs::new(O_RDONLY)).unwrap();
    s.ifs.close(&a, fa2).unwrap();
    assert!(
        s.ifs
            .object_locks(&a, "/tmp/r")
            .unwrap()
            .iter()
            .all(|l| l.owner != Owner(1) || matches!(l.what, Held::Open(..)))
    );
    // A read lock needs a descriptor open for reading.
    let fw = s.ifs.open(&a, "/tmp/r", OpenArgs::new(O_WRONLY)).unwrap();
    assert_eq!(
        s.ifs
            .lock_range(&a, fw, F_SETLK, RangeKind::Read, SEEK_SET, 90, 1),
        Err(Errno::EBADF)
    );
    s.ifs.release_owner(Owner(2));
    s.ifs
        .lock_range(&a, fa, F_SETLK, RangeKind::Write, SEEK_SET, 0, 0)
        .unwrap();
    assert_eq!(s.ifs.owner_locks(Owner(1)).len(), 3);
}

#[test]
fn check_out_and_in() {
    let s = sys();
    let sec = user("QSECOFR");
    let cx = s.cx(&sec, 1);
    s.put(&cx, "/tmp/c", b"v1");
    s.ifs.chmod(&cx, "/tmp/c", 0o666).unwrap();
    let bob = user("BOB");
    let zed = user("ZED");
    let bx = s.cx(&bob, 2);
    let zx = s.cx(&zed, 3);
    s.ifs.check_out(&bx, "/tmp/c").unwrap();
    assert_eq!(s.ifs.check_out(&zx, "/tmp/c"), Err(Errno::EBUSY));
    // Others read but do not write, rename or remove.
    assert!(s.ifs.open(&zx, "/tmp/c", OpenArgs::new(O_RDONLY)).is_ok());
    assert_eq!(
        s.ifs.open(&zx, "/tmp/c", OpenArgs::new(O_WRONLY)),
        Err(Errno::EBUSY)
    );
    s.put(&bx, "/tmp/c", b"v2");
    let attrs = s
        .ifs
        .get_attrs(&zx, "/tmp/c", &[ifs::ops::AttrId::CheckedOut], true)
        .unwrap();
    assert!(matches!(&attrs[0], ifs::ops::Attr::CheckedOut(Some(c)) if c.user == "BOB"));
    assert_eq!(s.ifs.check_in(&zx, "/tmp/c"), Err(Errno::EPERM));
    s.ifs.check_in(&bx, "/tmp/c").unwrap();
    assert_eq!(s.ifs.check_in(&bx, "/tmp/c"), Err(Errno::EINVAL));
    let fd = s.ifs.open(&zx, "/tmp/c", OpenArgs::new(O_WRONLY)).unwrap();
    s.ifs.close(&zx, fd).unwrap();
    // The owner (QSECOFR here) can check in what someone else checked out.
    s.ifs.check_out(&bx, "/tmp/c").unwrap();
    s.ifs.check_in(&cx, "/tmp/c").unwrap();
}

#[test]
fn release_owner_writes_back_members() {
    let s = sys();
    let p = user("QSECOFR");
    let cx = s.cx(&p, 7);
    let m = "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/HELLO.MBR";
    let fd = s
        .ifs
        .open(&cx, m, OpenArgs::new(O_WRONLY | ifs::ops::flags::O_TRUNC))
        .unwrap();
    s.ifs
        .write(&cx, fd, &ccsid::to_ebcdic("X\n").unwrap())
        .unwrap();
    s.ifs.chdir(&cx, "/tmp").unwrap();
    // The job ends without closing.
    s.ifs.release_owner(Owner(7));
    assert_eq!(s.ifs.open_count(), 0);
    let raw = s.get(&s.cx(&p, 8), m);
    assert_eq!(ccsid::from_ebcdic(&raw), "X\n");
    assert_eq!(s.ifs.getcwd(&s.cx(&p, 7)), "/");
}
