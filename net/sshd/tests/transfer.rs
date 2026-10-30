// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! File transfer and forwarding: SFTP on the root file system,
//! `/QOpenSys` and `/QSYS.LIB`, the legacy scp in both directions, and
//! TCP forwarding refused by default, allowed by the configuration and
//! held to the port restrictions.

#![allow(clippy::many_single_char_names)]

mod common;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;

use common::{TestClient, signed_on, world, world_with};
use sshd::client::Incoming;
use sshd::client::sftp::Sftp;
use sshd::sftp::proto::{Attrs, status};
use sshd::wire::Writer;

const SFTP: &str = "Subsystem sftp /QOpenSys/QIBM/ProdData/SC1/OpenSSH/libexec/sftp-server\n";

#[test]
fn sftp_on_root_and_qopensys() {
    let w = world(SFTP);
    let mut c = signed_on(&w.server, "qstest");
    let mut s = Sftp::start(&mut c).unwrap();
    assert!(s.extensions.iter().any(|(n, _)| n == "posix-rename@openssh.com"));
    assert_eq!(s.realpath(".").unwrap().unwrap(), "/home/QSTEST");

    let big: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
    s.put("big.bin", &big).unwrap().unwrap();
    assert_eq!(s.get("/home/QSTEST/big.bin").unwrap().unwrap(), big);
    assert_eq!(w.fs.get("/home/QSTEST/big.bin").unwrap(), big);
    let a = s.stat("big.bin").unwrap().unwrap();
    assert_eq!(a.size, Some(200_000));
    assert_eq!(a.permissions.map(|p| p & 0o170_000), Some(0o100_000));

    s.mkdir("dir").unwrap().unwrap();
    s.put("dir/a.txt", b"alpha").unwrap().unwrap();
    s.rename("dir/a.txt", "dir/b.txt").unwrap().unwrap();
    s.put("dir/c.txt", b"gamma").unwrap().unwrap();
    // Plain rename does not replace; posix-rename does.
    assert!(s.rename("dir/b.txt", "dir/c.txt").unwrap().is_err());
    s.extended("posix-rename@openssh.com", &["dir/b.txt", "dir/c.txt"])
        .unwrap()
        .unwrap();
    assert_eq!(s.get("dir/c.txt").unwrap().unwrap(), b"alpha");
    let mut names: Vec<String> = s.list("dir").unwrap().unwrap().into_iter().map(|e| e.0).collect();
    names.retain(|n| n != "." && n != "..");
    assert_eq!(names, ["c.txt"]);
    let long = s.list("/home/QSTEST").unwrap().unwrap();
    let dir = long.iter().find(|e| e.0 == "dir").unwrap();
    assert!(dir.1.starts_with("drwx"), "{}", dir.1);
    assert!(dir.1.contains("QSTEST"), "{}", dir.1);

    s.symlink("dir/c.txt", "link").unwrap().unwrap();
    assert_eq!(s.readlink("link").unwrap().unwrap(), "dir/c.txt");
    let l = s.lstat("link").unwrap().unwrap();
    assert_eq!(l.permissions.map(|p| p & 0o170_000), Some(0o120_000));
    assert_eq!(s.get("link").unwrap().unwrap(), b"alpha");

    s.setstat("dir/c.txt", &Attrs { permissions: Some(0o600), ..Attrs::default() })
        .unwrap()
        .unwrap();
    assert_eq!(s.stat("dir/c.txt").unwrap().unwrap().permissions.map(|p| p & 0o777), Some(0o600));
    s.setstat("dir/c.txt", &Attrs { size: Some(2), ..Attrs::default() }).unwrap().unwrap();
    assert_eq!(s.get("dir/c.txt").unwrap().unwrap(), b"al");

    assert_eq!(s.rmdir("dir").unwrap().unwrap_err().code, status::FAILURE);
    s.remove("dir/c.txt").unwrap().unwrap();
    s.remove("link").unwrap().unwrap();
    s.rmdir("dir").unwrap().unwrap();
    assert_eq!(s.stat("dir").unwrap().unwrap_err().code, status::NO_SUCH_FILE);

    // /QOpenSys: case-sensitive, the user's own directory there.
    s.put("/QOpenSys/qstest/Data.txt", b"open").unwrap().unwrap();
    assert_eq!(s.get("/QOpenSys/qstest/Data.txt").unwrap().unwrap(), b"open");
    assert_eq!(
        s.get("/QOpenSys/qstest/data.txt").unwrap().unwrap_err().code,
        status::NO_SUCH_FILE
    );
    // Authority is the user's: another user's home is not writable.
    assert_eq!(
        s.put("/home/QSTEST2/x.txt", b"no").unwrap().unwrap_err().code,
        status::PERMISSION_DENIED
    );
    s.finish().unwrap();
}

#[test]
fn sftp_on_qsys_lib() {
    let w = world(SFTP);
    let mut c = signed_on(&w.server, "qstest");
    let mut s = Sftp::start(&mut c).unwrap();
    let libs: Vec<String> = s.list("/QSYS.LIB").unwrap().unwrap().into_iter().map(|e| e.0).collect();
    assert!(libs.iter().any(|n| n == "QGPL.LIB"), "{libs:?}");
    let members: Vec<String> = s
        .list("/QSYS.LIB/QGPL.LIB/QCLSRC.FILE")
        .unwrap()
        .unwrap()
        .into_iter()
        .map(|e| e.0)
        .collect();
    assert!(members.iter().any(|n| n == "HELLO.MBR"), "{members:?}");
    let hello = s.get("/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/HELLO.MBR").unwrap().unwrap();
    assert!(!hello.is_empty());
    let a = s.stat("/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/HELLO.MBR").unwrap().unwrap();
    // IBM's member size counts two line-end bytes per source record;
    // a binary read is the records alone.
    assert_eq!(a.size, Some(hello.len() as u64 + 4));
    // Case does not matter in /QSYS.LIB.
    assert_eq!(s.get("/qsys.lib/qgpl.lib/qclsrc.file/hello.mbr").unwrap().unwrap(), hello);
    // A new member in the user's own source file.
    let path = "/QSYS.LIB/QSTEST.LIB/QRPGLESRC.FILE/NEW.MBR";
    match s.put(path, &hello).unwrap() {
        Ok(()) => assert_eq!(s.get(path).unwrap().unwrap(), hello),
        Err(st) => panic!("member write refused: {st:?}"),
    }
    s.finish().unwrap();
}

/// Read until `n` bytes arrived on `ch`.
fn read_n(c: &mut TestClient, ch: u32, n: usize) -> Vec<u8> {
    let mut out = Vec::new();
    while out.len() < n {
        match c.recv().unwrap() {
            Incoming::Data(x, d) if x == ch => out.extend(d),
            Incoming::Stderr(_, d) => panic!("stderr: {}", String::from_utf8_lossy(&d)),
            other => panic!("unexpected {other:?}"),
        }
    }
    out
}

#[test]
fn legacy_scp_both_ways() {
    let w = world("");
    let mut c = signed_on(&w.server, "qstest");
    // Upload: scp -t.
    let ch = c.session().unwrap();
    assert!(c.exec(ch, "scp -t /home/QSTEST/up.txt").unwrap());
    assert_eq!(read_n(&mut c, ch, 1), [0]);
    c.send_data(ch, b"C0640 5 up.txt\n").unwrap();
    assert_eq!(read_n(&mut c, ch, 1), [0]);
    c.send_data(ch, b"hello\0").unwrap();
    assert_eq!(read_n(&mut c, ch, 1), [0]);
    c.send_eof(ch).unwrap();
    assert_eq!(c.collect(ch).unwrap().status, Some(0));
    assert_eq!(w.fs.get("/home/QSTEST/up.txt").unwrap(), b"hello");

    // A directory, recursively, into the home directory.
    let ch = c.session().unwrap();
    assert!(c.exec(ch, "scp -r -t .").unwrap());
    assert_eq!(read_n(&mut c, ch, 1), [0]);
    for (send, data) in [
        (&b"D0755 0 tree\n"[..], &b""[..]),
        (b"C0644 3 one\n", b"111\0"),
        (b"E\n", b""),
    ] {
        c.send_data(ch, send).unwrap();
        assert_eq!(read_n(&mut c, ch, 1), [0]);
        if !data.is_empty() {
            c.send_data(ch, data).unwrap();
            assert_eq!(read_n(&mut c, ch, 1), [0]);
        }
    }
    c.send_eof(ch).unwrap();
    assert_eq!(c.collect(ch).unwrap().status, Some(0));
    assert_eq!(w.fs.get("/home/QSTEST/tree/one").unwrap(), b"111");

    // Download: scp -f.
    let ch = c.session().unwrap();
    assert!(c.exec(ch, "scp -f up.txt").unwrap());
    c.send_data(ch, &[0]).unwrap();
    // The mode is what the IFS kept for the file.
    let head = String::from_utf8(read_n(&mut c, ch, 15)).unwrap();
    assert!(head.starts_with("C06") && head.ends_with(" 5 up.txt\n"), "{head:?}");
    c.send_data(ch, &[0]).unwrap();
    assert_eq!(read_n(&mut c, ch, 6), b"hello\0");
    c.send_data(ch, &[0]).unwrap();
    c.send_eof(ch).unwrap();
    assert_eq!(c.collect(ch).unwrap().status, Some(0));

    // A file that is not there.
    let ch = c.session().unwrap();
    assert!(c.exec(ch, "scp -f nothere").unwrap());
    c.send_data(ch, &[0]).unwrap();
    let err = read_n(&mut c, ch, 1);
    assert_eq!(err[0], 1);
    assert!(String::from_utf8_lossy(&err).contains("nothere"));
    c.send_eof(ch).unwrap();
    assert_eq!(c.collect(ch).unwrap().status, Some(1));
}

fn direct(host: &str, port: u16) -> Vec<u8> {
    let mut w = Writer::new();
    w.str(host).u32(u32::from(port)).str("127.0.0.1").u32(4000);
    w.finish()
}

#[test]
fn forwarding_refused_by_default() {
    let w = world("");
    let mut c = signed_on(&w.server, "qstest");
    assert!(c.open("direct-tcpip", &direct("127.0.0.1", 9)).unwrap().is_err());
    let mut req = Writer::new();
    req.str("127.0.0.1").u32(0);
    assert!(c.global("tcpip-forward", &req.finish()).unwrap().is_none());
    // The connection still works.
    assert_eq!(c.run("echo still", b"").unwrap().stdout, b"still\n");
}

#[test]
fn forwarding_allowed_and_port_restrictions() {
    let rules = sockets::Restrictions(vec![sockets::Restriction {
        protocol: 6,
        low: 40_000,
        high: 40_010,
        user: Some("QSECOFR".into()),
    }]);
    let w = world_with("AllowTcpForwarding yes\n", |mut d| {
        d.ports = Arc::new(rules);
        d
    });
    let mut c = signed_on(&w.server, "qstest");

    // Local forward to an echo server.
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut b = [0u8; 5];
        s.read_exact(&mut b).unwrap();
        s.write_all(&b).unwrap();
    });
    let ch = c.open("direct-tcpip", &direct("127.0.0.1", port)).unwrap().unwrap();
    c.send_data(ch, b"ping!").unwrap();
    assert_eq!(read_n(&mut c, ch, 5), b"ping!");

    // Remote forward: a restricted port is refused, any free one works.
    let mut req = Writer::new();
    req.str("127.0.0.1").u32(40_005);
    assert!(c.global("tcpip-forward", &req.finish()).unwrap().is_none());
    let mut req = Writer::new();
    req.str("127.0.0.1").u32(0);
    let reply = c.global("tcpip-forward", &req.finish()).unwrap().unwrap();
    let bound = u16::try_from(u32::from_be_bytes(reply[..4].try_into().unwrap())).unwrap();
    std::thread::spawn(move || {
        let mut s = std::net::TcpStream::connect(("127.0.0.1", bound)).unwrap();
        s.write_all(b"back").unwrap();
    });
    let fwd = loop {
        if let Incoming::Opened(id) = c.recv().unwrap() {
            break id;
        }
    };
    assert_eq!(read_n(&mut c, fwd, 4), b"back");
}
