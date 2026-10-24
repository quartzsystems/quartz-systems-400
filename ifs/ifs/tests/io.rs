// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Descriptors and data: read, write, positions, append, truncate,
//! `dup`, limits, unlinked open files, devices, members, and text-mode
//! conversion round trips.

#![allow(clippy::many_single_char_names)]

mod common;

use common::{sys, user};
use ifs::Errno;
use ifs::ops::flags::{
    F_GETFL, F_SETFL, O_APPEND, O_CCSID, O_CREAT, O_EXCL, O_RDONLY, O_RDWR, O_TEXT_CREAT,
    O_TEXTDATA, O_TRUNC, O_WRONLY, SEEK_CUR, SEEK_END, SEEK_SET,
};
use ifs::ops::{Config, Ifs, OpenArgs};

#[test]
fn read_write_seek() {
    let s = sys();
    let p = user("BOB");
    let cx = s.cx(&p, 2);
    let sec = user("QSECOFR");
    s.ifs.mkdir(&s.cx(&sec, 1), "/home/bob", 0o777).unwrap();
    s.ifs.chmod(&s.cx(&sec, 1), "/home/bob", 0o777).unwrap();
    let fd = s
        .ifs
        .open(
            &cx,
            "/home/bob/f",
            OpenArgs::new(O_RDWR | O_CREAT | O_EXCL).mode(0o600),
        )
        .unwrap();
    assert_eq!(s.ifs.write(&cx, fd, b"hello world").unwrap(), 11);
    assert_eq!(s.ifs.lseek(&cx, fd, 0, SEEK_CUR).unwrap(), 11);
    assert_eq!(s.ifs.lseek(&cx, fd, -5, SEEK_END).unwrap(), 6);
    let mut b = [0u8; 10];
    assert_eq!(s.ifs.read(&cx, fd, &mut b).unwrap(), 5);
    assert_eq!(&b[..5], b"world");
    assert_eq!(s.ifs.read(&cx, fd, &mut b).unwrap(), 0);
    assert_eq!(s.ifs.pread(&cx, fd, &mut b[..5], 0).unwrap(), 5);
    assert_eq!(&b[..5], b"hello");
    s.ifs.pwrite(&cx, fd, b"J", 0).unwrap();
    assert_eq!(s.ifs.lseek(&cx, fd, 0, SEEK_CUR).unwrap(), 11);
    // A hole.
    s.ifs.lseek(&cx, fd, 20, SEEK_SET).unwrap();
    s.ifs.write(&cx, fd, b"!").unwrap();
    assert_eq!(s.ifs.fstat(&cx, fd).unwrap().size, 21);
    s.ifs.ftruncate(&cx, fd, 5).unwrap();
    s.ifs.fsync(&cx, fd, false).unwrap();
    s.ifs.close(&cx, fd).unwrap();
    assert_eq!(s.get(&cx, "/home/bob/f"), b"Jello");
    assert_eq!(s.ifs.close(&cx, fd), Err(Errno::EBADF));
    assert_eq!(s.ifs.lseek(&cx, 99, 0, SEEK_SET), Err(Errno::EBADF));
    // Read-only descriptors do not write and the other way round.
    let fd = s
        .ifs
        .open(&cx, "/home/bob/f", OpenArgs::new(O_RDONLY))
        .unwrap();
    assert_eq!(s.ifs.write(&cx, fd, b"x"), Err(Errno::EBADF));
    s.ifs.close(&cx, fd).unwrap();
    let fd = s
        .ifs
        .open(&cx, "/home/bob/f", OpenArgs::new(O_WRONLY | O_APPEND))
        .unwrap();
    assert_eq!(s.ifs.read(&cx, fd, &mut b), Err(Errno::EBADF));
    s.ifs.write(&cx, fd, b"!").unwrap();
    s.ifs.close(&cx, fd).unwrap();
    assert_eq!(s.get(&cx, "/home/bob/f"), b"Jello!");
    let fd = s
        .ifs
        .open(&cx, "/home/bob/f", OpenArgs::new(O_WRONLY | O_TRUNC))
        .unwrap();
    s.ifs.close(&cx, fd).unwrap();
    assert!(s.get(&cx, "/home/bob/f").is_empty());
    // Bad flags.
    assert_eq!(
        s.ifs.open(&cx, "/home/bob/f", OpenArgs::new(0)),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        s.ifs.open(&cx, "/home", OpenArgs::new(O_RDWR)),
        Err(Errno::EISDIR)
    );
    // A directory opens for reading: openat and fchdir use it.
    let d = s
        .ifs
        .open(&cx, "/home/BOB", OpenArgs::new(O_RDONLY))
        .unwrap();
    assert_eq!(s.ifs.read(&cx, d, &mut b), Err(Errno::EISDIR));
    let fd = s
        .ifs
        .openat(&cx, Some(d), "f", OpenArgs::new(O_RDONLY))
        .unwrap();
    s.ifs.close(&cx, fd).unwrap();
    assert_eq!(s.ifs.fd_path(&cx, d).unwrap(), "/home/bob");
    s.ifs.fchdir(&cx, d).unwrap();
    assert_eq!(s.ifs.getcwd(&cx), "/home/bob");
    s.ifs.close(&cx, d).unwrap();
    assert_eq!(
        s.ifs.open(&cx, "/home/bob/none", OpenArgs::new(O_RDONLY)),
        Err(Errno::ENOENT)
    );
}

#[test]
fn dup_shares_the_position_and_flags() {
    let s = sys();
    let p = user("QSECOFR");
    let cx = s.cx(&p, 1);
    s.put(&cx, "/tmp/d", b"abcdef");
    let fd = s.ifs.open(&cx, "/tmp/d", OpenArgs::new(O_RDWR)).unwrap();
    let fd2 = s.ifs.dup(&cx, fd).unwrap();
    assert_ne!(fd, fd2);
    let mut b = [0u8; 2];
    s.ifs.read(&cx, fd, &mut b).unwrap();
    s.ifs.read(&cx, fd2, &mut b).unwrap();
    assert_eq!(&b, b"cd");
    s.ifs
        .fcntl(&cx, fd, F_SETFL, i32::try_from(O_APPEND).unwrap())
        .unwrap();
    assert_ne!(
        s.ifs.fcntl(&cx, fd2, F_GETFL, 0).unwrap() & i32::try_from(O_APPEND).unwrap(),
        0
    );
    assert_eq!(s.ifs.dup2(&cx, fd, 10).unwrap(), 10);
    s.ifs.close(&cx, fd).unwrap();
    s.ifs.close(&cx, fd2).unwrap();
    s.ifs.write(&cx, 10, b"!").unwrap();
    s.ifs.close(&cx, 10).unwrap();
    assert_eq!(s.get(&cx, "/tmp/d"), b"abcdef!");
    assert_eq!(s.ifs.open_count(), 0);
    // The lowest free descriptor is used.
    let a = s.ifs.open(&cx, "/tmp/d", OpenArgs::new(O_RDONLY)).unwrap();
    let b2 = s.ifs.open(&cx, "/tmp/d", OpenArgs::new(O_RDONLY)).unwrap();
    s.ifs.close(&cx, a).unwrap();
    assert_eq!(
        s.ifs.open(&cx, "/tmp/d", OpenArgs::new(O_RDONLY)).unwrap(),
        a
    );
    let _ = b2;
}

#[test]
fn descriptor_limit() {
    let env = common::env();
    let ifs = Ifs::new(
        ifs::backend::mem::MemBackend::new(),
        Box::new(ifs::vfs::qsys::MemQsys::new(sec::auth::bits::Auth::EXCLUDE)),
        Config {
            max_fds: 3,
            ..Config::default()
        },
    );
    ifs.ensure_layout(&env).unwrap();
    let p = user("QSECOFR");
    let audit = ifs::ops::audit::NoAudit;
    let cx = ifs::Ctx {
        owner: ifs::Owner(1),
        principal: &p,
        env: &env,
        audit: &audit,
        job_ccsid: 37,
    };
    for _ in 0..3 {
        ifs.open(&cx, "/QOpenSys/dev/null", OpenArgs::new(O_RDONLY))
            .unwrap();
    }
    assert_eq!(
        ifs.open(&cx, "/QOpenSys/dev/null", OpenArgs::new(O_RDONLY)),
        Err(Errno::EMFILE)
    );
    ifs.release_owner(ifs::Owner(1));
    assert_eq!(ifs.open_count(), 0);
    assert!(ifs.owners().is_empty());
}

#[test]
fn unlinked_open_file_stays_readable() {
    let s = sys();
    let p = user("QSECOFR");
    let cx = s.cx(&p, 1);
    s.put(&cx, "/tmp/gone", b"still here");
    let fd = s
        .ifs
        .open(&cx, "/tmp/gone", OpenArgs::new(O_RDONLY))
        .unwrap();
    s.ifs.unlink(&cx, "/tmp/gone").unwrap();
    let st = s.ifs.fstat(&cx, fd).unwrap();
    assert_eq!(st.nlink, 0);
    assert_eq!(st.size, 10);
    let mut b = [0u8; 32];
    assert_eq!(s.ifs.read(&cx, fd, &mut b).unwrap(), 10);
    s.ifs.close(&cx, fd).unwrap();
}

#[test]
fn devices() {
    let s = sys();
    let p = user("ZED");
    let cx = s.cx(&p, 3);
    let fd = s.ifs.open(&cx, "/dev/zero", OpenArgs::new(O_RDWR)).unwrap();
    let mut b = [9u8; 8];
    assert_eq!(s.ifs.read(&cx, fd, &mut b).unwrap(), 8);
    assert_eq!(b, [0; 8]);
    assert_eq!(s.ifs.pread(&cx, fd, &mut b, 0), Err(Errno::ESPIPE));
    s.ifs.close(&cx, fd).unwrap();
    let fd = s
        .ifs
        .open(&cx, "/QOpenSys/dev/null", OpenArgs::new(O_WRONLY))
        .unwrap();
    assert_eq!(s.ifs.write(&cx, fd, b"gone").unwrap(), 4);
    s.ifs.close(&cx, fd).unwrap();
    let fd = s
        .ifs
        .open(&cx, "/dev/full", OpenArgs::new(O_WRONLY))
        .unwrap();
    assert_eq!(s.ifs.write(&cx, fd, b"x"), Err(Errno::ENOSPC));
    s.ifs.close(&cx, fd).unwrap();
    let fd = s
        .ifs
        .open(&cx, "/dev/urandom", OpenArgs::new(O_RDONLY))
        .unwrap();
    let mut r = [0u8; 32];
    s.ifs.read(&cx, fd, &mut r).unwrap();
    assert_ne!(r, [0u8; 32]);
    s.ifs.close(&cx, fd).unwrap();
    let st = s.ifs.stat(&cx, "/dev/null").unwrap();
    assert_eq!(st.objtype, "*CHRSF");
    assert_eq!(st.mode & 0o170_000, 0o020_000);
    let names: Vec<String> = s
        .ifs
        .read_dir(&cx, "/QOpenSys/dev")
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(
        names,
        vec!["full", "null", "random", "tty", "urandom", "zero"]
    );
    // `/dev` also lists the system ASP's directory.
    let names: Vec<String> = s
        .ifs
        .read_dir(&cx, "/dev")
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert!(names.contains(&"QASP01".to_string()));
    let fd = s.ifs.open(&cx, "/dev/tty", OpenArgs::new(O_RDWR)).unwrap();
    assert_eq!(s.ifs.read(&cx, fd, &mut b), Err(Errno::ENXIO));
    s.ifs.close(&cx, fd).unwrap();
}

#[test]
#[allow(clippy::too_many_lines)]
fn text_mode_conversions() {
    let s = sys();
    let p = user("QSECOFR");
    let cx = s.cx(&p, 1);
    // A file tagged 37 written in EBCDIC.
    let fd = s
        .ifs
        .open(
            &cx,
            "/tmp/e.txt",
            OpenArgs::new(O_WRONLY | O_CREAT | O_CCSID)
                .mode(0o644)
                .conv(37),
        )
        .unwrap();
    s.ifs
        .write(
            &cx,
            fd,
            &ccsid::to_ebcdic("Hello, QS/400!\nline 2\n").unwrap(),
        )
        .unwrap();
    s.ifs.close(&cx, fd).unwrap();
    assert_eq!(s.ifs.stat(&cx, "/tmp/e.txt").unwrap().ccsid, 37);
    // Read as UTF-8, a few bytes at a time.
    let fd = s
        .ifs
        .open(
            &cx,
            "/tmp/e.txt",
            OpenArgs::new(O_RDONLY | O_TEXTDATA | O_CCSID).conv(1208),
        )
        .unwrap();
    let mut out = Vec::new();
    let mut b = [0u8; 3];
    loop {
        let n = s.ifs.read(&cx, fd, &mut b).unwrap();
        if n == 0 {
            break;
        }
        out.extend_from_slice(&b[..n]);
    }
    s.ifs.close(&cx, fd).unwrap();
    assert_eq!(String::from_utf8(out).unwrap(), "Hello, QS/400!\nline 2\n");
    // Without O_CCSID the job's CCSID (37 here): no conversion.
    let fd = s
        .ifs
        .open(&cx, "/tmp/e.txt", OpenArgs::new(O_RDONLY | O_TEXTDATA))
        .unwrap();
    let mut b = [0u8; 5];
    s.ifs.read(&cx, fd, &mut b).unwrap();
    assert_eq!(ccsid::from_ebcdic(&b), "Hello");
    s.ifs.close(&cx, fd).unwrap();
    // Writing UTF-16 into a 37 file.
    let fd = s
        .ifs
        .open(
            &cx,
            "/tmp/e.txt",
            OpenArgs::new(O_WRONLY | O_TRUNC | O_TEXTDATA | O_CCSID).conv(1200),
        )
        .unwrap();
    let u16: Vec<u8> = "Grüße\n"
        .encode_utf16()
        .flat_map(u16::to_be_bytes)
        .collect();
    // Split inside a character: the converter carries it.
    s.ifs.write(&cx, fd, &u16[..3]).unwrap();
    s.ifs.write(&cx, fd, &u16[3..]).unwrap();
    s.ifs.close(&cx, fd).unwrap();
    let raw = s.get(&cx, "/tmp/e.txt");
    assert_eq!(ccsid::from_ebcdic(&raw), "Grüße\n");
    // Every supported CCSID round-trips text it can hold.
    for c in [
        37u16, 273, 277, 278, 280, 284, 285, 297, 500, 871, 819, 850, 1252, 367, 1208, 1200, 13488,
    ] {
        let name = format!("/tmp/rt{c}");
        let fd = s
            .ifs
            .open(
                &cx,
                &name,
                OpenArgs::new(O_WRONLY | O_CREAT | O_TEXT_CREAT | O_TEXTDATA | O_CCSID)
                    .mode(0o644)
                    .conv(u32::from(c))
                    .text_conv(1208),
            )
            .unwrap();
        s.ifs.write(&cx, fd, b"ABC xyz 0123 +-*/\n").unwrap();
        s.ifs.close(&cx, fd).unwrap();
        assert_eq!(s.ifs.stat(&cx, &name).unwrap().ccsid, c);
        let fd = s
            .ifs
            .open(
                &cx,
                &name,
                OpenArgs::new(O_RDONLY | O_TEXTDATA | O_CCSID).conv(1208),
            )
            .unwrap();
        let mut b = [0u8; 64];
        let n = s.ifs.read(&cx, fd, &mut b).unwrap();
        s.ifs.close(&cx, fd).unwrap();
        assert_eq!(&b[..n], b"ABC xyz 0123 +-*/\n", "CCSID {c}");
        if c == 1200 {
            assert_eq!(s.ifs.stat(&cx, &name).unwrap().size, 36);
        }
    }
    // An unknown CCSID.
    assert_eq!(
        s.ifs.open(
            &cx,
            "/tmp/e.txt",
            OpenArgs::new(O_RDONLY | O_TEXTDATA | O_CCSID).conv(4242)
        ),
        Err(Errno::ECONVERT)
    );
    // Binary files are never converted.
    let fd = s
        .ifs
        .open(
            &cx,
            "/tmp/bin",
            OpenArgs::new(O_WRONLY | O_CREAT | O_CCSID)
                .mode(0o644)
                .conv(65535),
        )
        .unwrap();
    s.ifs.write(&cx, fd, &[0xC1, 0x25]).unwrap();
    s.ifs.close(&cx, fd).unwrap();
    let fd = s
        .ifs
        .open(
            &cx,
            "/tmp/bin",
            OpenArgs::new(O_RDONLY | O_TEXTDATA | O_CCSID).conv(819),
        )
        .unwrap();
    let mut b = [0u8; 2];
    s.ifs.read(&cx, fd, &mut b).unwrap();
    assert_eq!(b, [0xC1, 0x25]);
    s.ifs.close(&cx, fd).unwrap();
}

#[test]
fn default_ccsid_and_members() {
    let s = sys();
    let p = user("QSECOFR");
    let cx = s.cx(&p, 1);
    s.put(&cx, "/tmp/new", b"x");
    assert_eq!(s.ifs.stat(&cx, "/tmp/new").unwrap().ccsid, 1208);
    // A member read as UTF-8 text, then written back.
    let m = "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/HELLO.MBR";
    let fd = s
        .ifs
        .open(
            &cx,
            m,
            OpenArgs::new(O_RDONLY | O_TEXTDATA | O_CCSID).conv(1208),
        )
        .unwrap();
    let mut b = [0u8; 64];
    let n = s.ifs.read(&cx, fd, &mut b).unwrap();
    assert_eq!(&b[..n], b"PGM\nENDPGM\n");
    s.ifs.close(&cx, fd).unwrap();
    let fd = s
        .ifs
        .open(
            &cx,
            m,
            OpenArgs::new(O_WRONLY | O_TRUNC | O_TEXTDATA | O_CCSID).conv(1208),
        )
        .unwrap();
    s.ifs
        .write(&cx, fd, b"PGM\r\nSNDMSG MSG(HI)\nENDPGM\n")
        .unwrap();
    s.ifs.close(&cx, fd).unwrap();
    let raw = s.get(&cx, m);
    assert_eq!(ccsid::from_ebcdic(&raw), "PGM\nSNDMSG MSG(HI)\nENDPGM\n");
    // A new member through O_CREAT.
    s.put(
        &cx,
        "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/NEW.MBR",
        &ccsid::to_ebcdic("A\n").unwrap(),
    );
    assert!(
        s.ifs
            .read_dir(&cx, "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE")
            .unwrap()
            .iter()
            .any(|e| e.name == "NEW.MBR")
    );
    s.ifs
        .unlink(&cx, "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/NEW.MBR")
        .unwrap();
    // A library through mkdir.
    s.ifs.mkdir(&cx, "/QSYS.LIB/MYLIB.LIB", 0o755).unwrap();
    assert_eq!(
        s.ifs.stat(&cx, "/QSYS.LIB/MYLIB.LIB").unwrap().objtype,
        "*LIB"
    );
    s.ifs.rmdir(&cx, "/QSYS.LIB/MYLIB.LIB").unwrap();
}
