// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The socket layer against the Linux kernel: TCP and UDP over
//! loopback, `AF_UNIX` through the IFS path map, non-blocking sockets
//! with `poll`, options in IBM's layouts, the port restriction check,
//! descriptor passing between two jobs, and `getaddrinfo` on the
//! shipped host table.

#![cfg(target_os = "linux")]
#![allow(clippy::many_single_char_names)]

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream, UdpSocket};

use sockets::consts::{
    AF_INET, AF_UNIX, IPPROTO_TCP, POLLIN, POLLNVAL, POLLOUT, SO_KEEPALIVE, SO_REUSEADDR, SO_TYPE, SOCK_DGRAM,
    SOCK_STREAM, SOL_SOCKET, TCP_NODELAY,
};
use sockets::layout::i32_at;
use sockets::resolver::{Hints, Resolver};
use sockets::{
    AllowAll, Errno, InProcess, PortRules, Restriction, Restrictions, RootedPaths, SockAddr, Sockets,
};

fn job(rules: Box<dyn PortRules>, relay: InProcess) -> Sockets {
    let mut s = Sockets::new(rules, Box::new(RootedPaths::default()), Box::new(relay));
    s.set_user("QSTEST");
    // The test runs without CAP_NET_ADMIN.
    s.set_mark(None);
    s
}

fn plain() -> Sockets {
    job(Box::new(AllowAll), InProcess::new([1; 16]))
}

fn loopback(port: u16) -> SockAddr {
    SockAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port))
}

#[test]
fn tcp_echo_through_a_listener() {
    let mut s = plain();
    let l = s.socket(AF_INET, SOCK_STREAM, 0).unwrap();
    s.setopt(l, SOL_SOCKET, SO_REUSEADDR, &1i32.to_be_bytes()).unwrap();
    s.bind(l, &loopback(0)).unwrap();
    s.listen(l, 5).unwrap();
    let port = s.sockname(l).unwrap().port();
    assert_ne!(port, 0);
    let peer = std::thread::spawn(move || {
        let mut c = TcpStream::connect(("127.0.0.1", port)).unwrap();
        c.write_all(b"hello").unwrap();
        let mut back = [0u8; 5];
        c.read_exact(&mut back).unwrap();
        back
    });
    let (a, from) = s.accept(l).unwrap();
    assert_eq!(from.unwrap().ip(), Some(Ipv4Addr::LOCALHOST.into()));
    let mut buf = [0u8; 16];
    let n = s.recv(a, &mut buf, 0).unwrap();
    assert_eq!(&buf[..n], b"hello");
    assert_eq!(s.send(a, &buf[..n], 0).unwrap(), 5);
    assert_eq!(&peer.join().unwrap(), b"hello");
    assert_eq!(i32_at(&s.getopt(a, SOL_SOCKET, SO_TYPE).unwrap(), 0), SOCK_STREAM);
    s.setopt(a, IPPROTO_TCP, TCP_NODELAY, &1i32.to_be_bytes()).unwrap();
    assert_eq!(i32_at(&s.getopt(a, IPPROTO_TCP, TCP_NODELAY).unwrap(), 0), 1);
    assert_eq!(i32_at(&s.getopt(a, SOL_SOCKET, SO_KEEPALIVE).unwrap(), 0), 0);
    assert_eq!(s.getopt(a, SOL_SOCKET, 9999), Err(Errno::ENOPROTOOPT));
    s.close(a).unwrap();
    s.close(l).unwrap();
    assert_eq!(s.close(l), Err(Errno::EBADF));
    assert_eq!(s.count(), 0);
}

#[test]
fn udp_datagrams_with_addresses() {
    let mut s = plain();
    let u = s.socket(AF_INET, SOCK_DGRAM, 0).unwrap();
    s.bind(u, &loopback(0)).unwrap();
    let port = s.sockname(u).unwrap().port();
    let other = UdpSocket::bind("127.0.0.1:0").unwrap();
    other.send_to(b"ping", ("127.0.0.1", port)).unwrap();
    let mut buf = [0u8; 8];
    let (n, from) = s.recvfrom(u, &mut buf, 0).unwrap();
    assert_eq!(&buf[..n], b"ping");
    let from = from.unwrap();
    assert_eq!(from.port(), other.local_addr().unwrap().port());
    s.sendto(u, b"pong", 0, &from).unwrap();
    let mut back = [0u8; 8];
    let (n, _) = other.recv_from(&mut back).unwrap();
    assert_eq!(&back[..n], b"pong");
}

#[test]
fn unix_sockets_live_under_the_path_map() {
    let dir = std::env::temp_dir().join(format!("qs400-sock-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("tmp")).unwrap();
    let root = dir.to_string_lossy().into_owned();
    let mut s = Sockets::new(
        Box::new(AllowAll),
        Box::new(RootedPaths::new(&root)),
        Box::new(InProcess::new([2; 16])),
    );
    s.set_mark(None);
    let l = s.socket(AF_UNIX, SOCK_STREAM, 0).unwrap();
    let ifs = SockAddr::Unix("/tmp/app.sock".into());
    s.bind(l, &ifs).unwrap();
    assert!(dir.join("tmp/app.sock").exists(), "bound under the root");
    assert_eq!(s.sockname(l).unwrap(), ifs, "reported as the IFS path");
    s.listen(l, 1).unwrap();
    let c = s.socket(AF_UNIX, SOCK_STREAM, 0).unwrap();
    s.connect(c, &ifs).unwrap();
    let (a, _) = s.accept(l).unwrap();
    s.write(c, b"over unix").unwrap();
    let mut buf = [0u8; 16];
    let n = s.read(a, &mut buf).unwrap();
    assert_eq!(&buf[..n], b"over unix");
    let (x, y) = s.socketpair(AF_UNIX, SOCK_STREAM, 0).unwrap();
    s.send(x, b"pair", 0).unwrap();
    assert_eq!(s.recv(y, &mut buf, 0).unwrap(), 4);
    assert_eq!(s.socketpair(AF_INET, SOCK_STREAM, 0), Err(Errno::EAFNOSUPPORT));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn non_blocking_and_poll() {
    let mut s = plain();
    let l = s.socket(AF_INET, SOCK_STREAM, 0).unwrap();
    s.bind(l, &loopback(0)).unwrap();
    s.listen(l, 5).unwrap();
    s.set_nonblock(l, true).unwrap();
    assert_eq!(s.accept(l).unwrap_err(), Errno::EAGAIN, "EWOULDBLOCK");
    let port = s.sockname(l).unwrap().port();
    assert_eq!(s.poll(&[(l, POLLIN)], 0).unwrap(), vec![0]);
    let c = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let ready = s.poll(&[(l, POLLIN), (9999, POLLIN)], 2000).unwrap();
    assert_eq!(ready[0] & POLLIN, POLLIN);
    assert_eq!(ready[1], POLLNVAL);
    let (a, _) = s.accept(l).unwrap();
    s.set_nonblock(a, true).unwrap();
    let mut buf = [0u8; 4];
    assert_eq!(s.recv(a, &mut buf, 0), Err(Errno::EAGAIN));
    assert_eq!(s.pending(a).unwrap(), 0);
    (&c).write_all(b"abc").unwrap();
    let r = s.poll(&[(a, POLLIN | POLLOUT)], 2000).unwrap();
    assert_eq!(r[0] & (POLLIN | POLLOUT), POLLIN | POLLOUT);
    assert_eq!(s.pending(a).unwrap(), 3);
}

#[test]
fn port_restrictions_refuse_with_eacces() {
    let free = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let rules = Restrictions(vec![Restriction {
        protocol: IPPROTO_TCP,
        low: free,
        high: free,
        user: Some("WEBSRV".into()),
    }]);
    let mut s = job(Box::new(rules.clone()), InProcess::new([3; 16]));
    let t = s.socket(AF_INET, SOCK_STREAM, 0).unwrap();
    assert_eq!(s.bind(t, &loopback(free)), Err(Errno::EACCES));
    let u = s.socket(AF_INET, SOCK_DGRAM, 0).unwrap();
    s.bind(u, &loopback(free)).unwrap();
    let mut w = job(Box::new(rules), InProcess::new([4; 16]));
    w.set_user("websrv");
    let t2 = w.socket(AF_INET, SOCK_STREAM, 0).unwrap();
    w.bind(t2, &loopback(free)).unwrap();
}

#[test]
fn the_mark_needs_cap_net_admin_but_never_fails_a_bind() {
    let mut s = Sockets::default();
    let t = s.socket(AF_INET, SOCK_STREAM, 0).unwrap();
    s.bind(t, &loopback(0)).unwrap();
}

#[test]
fn descriptors_pass_between_jobs() {
    let relay = InProcess::new([5; 16]);
    let mut giver = job(Box::new(AllowAll), relay.clone());
    let taker_relay = relay.join([6; 16]);
    let l = giver.socket(AF_INET, SOCK_STREAM, 0).unwrap();
    giver.bind(l, &loopback(0)).unwrap();
    giver.listen(l, 1).unwrap();
    let port = giver.sockname(l).unwrap().port();
    let mut c = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let (a, _) = giver.accept(l).unwrap();
    let taker = std::thread::spawn(move || {
        let mut t = job(Box::new(AllowAll), taker_relay);
        let k = t.take(Some(&[5; 16])).unwrap();
        let mut buf = [0u8; 8];
        let n = t.recv(k, &mut buf, 0).unwrap();
        t.send(k, b"taken", 0).unwrap();
        buf[..n].to_vec()
    });
    giver.give(a, &[6; 16]).unwrap();
    giver.close(a).unwrap();
    c.write_all(b"handoff").unwrap();
    assert_eq!(taker.join().unwrap(), b"handoff");
    let mut back = [0u8; 5];
    c.read_exact(&mut back).unwrap();
    assert_eq!(&back, b"taken");
    assert_eq!(relay.waiting(), 0);
}

#[test]
fn getaddrinfo_localhost_then_connect() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let mut r = Resolver::default();
    let hints = Hints {
        family: AF_INET,
        socktype: SOCK_STREAM,
        ..Hints::default()
    };
    let list = r.getaddrinfo(Some("localhost"), Some(&port.to_string()), &hints).unwrap();
    let mut s = plain();
    let sd = s.socket(AF_INET, list[0].socktype, list[0].protocol).unwrap();
    s.connect(sd, &list[0].addr).unwrap();
    let (_, peer) = listener.accept().unwrap();
    assert_eq!(s.sockname(sd).unwrap().port(), peer.port());
}

#[test]
fn descriptors_pass_between_processes_through_the_relay() {
    use std::os::fd::OwnedFd;
    use sockets::relay;
    let dir = std::env::temp_dir().join(format!("qs400-relay-{}", std::process::id()));
    let path = dir.join("fdrelay").display().to_string();
    {
        let p = path.clone();
        std::thread::spawn(move || relay::serve(&p));
    }
    while !std::path::Path::new(&path).exists() {
        std::thread::yield_now();
    }
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let (server, _) = l.accept().unwrap();
    let pool = relay::pool("QZRCSRVS");
    // Nobody waits yet: the descriptor queues.
    let fd = sockets::Fd::from(OwnedFd::from(server));
    relay::give(&path, &fd, &pool, &[7; 16], b"T").unwrap();
    drop(fd);
    let t = relay::take(&path, &pool, None, 2000).unwrap().expect("a descriptor");
    assert_eq!(t.source, [7; 16]);
    assert_eq!(t.note, b"T");
    let mut taken = TcpStream::from(t.fd.into_owned());
    client.write_all(b"ping").unwrap();
    let mut b = [0u8; 4];
    taken.read_exact(&mut b).unwrap();
    assert_eq!(&b, b"ping");
    assert!(relay::take(&path, &pool, None, 100).unwrap().is_none());
    assert_eq!(relay::idle(&path, &pool).unwrap(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}
