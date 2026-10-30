// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The bytes the codecs write, and the offsets the APIs read, sit where
//! the tables of docs/TCPIP.md "Sockets" put each field (`spec.rs`
//! checks the `Layout`s; this checks the code that does not use them).

use std::collections::BTreeMap;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};
use std::path::Path;
use std::time::Duration;

use sockets::layout::misc::{PollFd, iovec, msghdr, put_linger, put_timeval};
use sockets::layout::netdb::{AddrInfo, Image, addrinfo, hostent, servent};
use sockets::{AddrForm, SockAddr};

/// Structure name to `(size, field -> (offset, size))`, from the doc.
type Doc = BTreeMap<String, (usize, BTreeMap<String, (usize, usize)>)>;

fn doc() -> Doc {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/TCPIP.md");
    let md = std::fs::read_to_string(path).expect("docs/TCPIP.md").replace("\r\n", "\n");
    let start = md.find("\n## Sockets").expect("a Sockets section");
    let s = &md[start + 1..];
    let s = &s[..s[3..].find("\n## ").map_or(s.len(), |i| i + 3)];
    let mut out = Doc::new();
    let mut cur: Option<String> = None;
    for l in s.lines() {
        if let Some(rest) = l.strip_prefix("#### `") {
            let (name, tail) = rest.split_once('`').expect("closing backtick");
            let size = tail.trim().trim_start_matches('(').trim_end_matches(" bytes)").parse().expect("size");
            out.insert(name.to_string(), (size, BTreeMap::new()));
            cur = Some(name.to_string());
        } else if l.starts_with("### ") {
            cur = None;
        } else if let Some(name) = &cur
            && l.starts_with('|')
            && !l.starts_with("| Offset")
            && !l.starts_with("| ---")
            && !l.starts_with("|---")
        {
            let c: Vec<&str> = l.trim().trim_matches('|').split('|').map(|c| c.trim().trim_matches('`')).collect();
            let entry = out.get_mut(name).expect("current");
            entry.1.insert(c[2].to_string(), (c[0].parse().expect("offset"), c[1].parse().expect("size")));
        }
    }
    out
}

/// `(offset, size)` of a field.
fn at(d: &Doc, s: &str, f: &str) -> (usize, usize) {
    *d.get(s)
        .unwrap_or_else(|| panic!("docs/TCPIP.md has no {s}"))
        .1
        .get(f)
        .unwrap_or_else(|| panic!("docs/TCPIP.md: {s} has no {f}"))
}

fn size(d: &Doc, s: &str) -> usize {
    d.get(s).unwrap_or_else(|| panic!("docs/TCPIP.md has no {s}")).0
}

/// The big-endian unsigned number in a field.
fn num(b: &[u8], (off, n): (usize, usize)) -> u64 {
    b[off..off + n].iter().fold(0, |a, x| (a << 8) | u64::from(*x))
}

fn field(b: &[u8], (off, n): (usize, usize)) -> &[u8] {
    &b[off..off + n]
}

#[test]
fn sockaddr_in_both_forms() {
    let d = doc();
    let a = SockAddr::V4(SocketAddrV4::new(Ipv4Addr::new(10, 1, 2, 3), 8471));
    let b = a.to_bytes(AddrForm::Bsd43);
    let t = "sockaddr_in (BSD 4.3)";
    assert_eq!(b.len(), size(&d, t));
    assert_eq!(num(&b, at(&d, t, "sin_family")), 2);
    assert_eq!(num(&b, at(&d, t, "sin_port")), 8471);
    assert_eq!(field(&b, at(&d, t, "sin_addr")), &[10, 1, 2, 3]);
    assert!(field(&b, at(&d, t, "sin_zero")).iter().all(|x| *x == 0));
    assert_eq!(SockAddr::decode(&b, AddrForm::Bsd43), Ok(a.clone()));

    let b = a.to_bytes(AddrForm::Bsd44);
    let t = "sockaddr_in (BSD 4.4)";
    assert_eq!(b.len(), size(&d, t));
    assert_eq!(num(&b, at(&d, t, "sin_len")), 16);
    assert_eq!(num(&b, at(&d, t, "sin_family")), 2);
    assert_eq!(num(&b, at(&d, t, "sin_port")), 8471);
    assert_eq!(field(&b, at(&d, t, "sin_addr")), &[10, 1, 2, 3]);
    assert_eq!(SockAddr::decode(&b, AddrForm::Bsd44), Ok(a));
}

#[test]
fn sockaddr_in6_both_forms() {
    let d = doc();
    let ip = Ipv6Addr::new(0xfe80, 0, 0, 0, 1, 2, 3, 4);
    let a = SockAddr::V6(SocketAddrV6::new(ip, 22, 0x0001_2345, 7));
    for (form, t) in [(AddrForm::Bsd43, "sockaddr_in6 (BSD 4.3)"), (AddrForm::Bsd44, "sockaddr_in6 (BSD 4.4)")] {
        let b = a.to_bytes(form);
        assert_eq!(b.len(), size(&d, t), "{t}");
        let fam = if form == AddrForm::Bsd43 { "sin6_family" } else { "sin6_len" };
        if form == AddrForm::Bsd44 {
            assert_eq!(num(&b, at(&d, t, fam)), 28, "{t}");
            assert_eq!(num(&b, at(&d, t, "sin6_family")), 24, "{t}");
        } else {
            assert_eq!(num(&b, at(&d, t, fam)), 24, "{t}");
        }
        assert_eq!(num(&b, at(&d, t, "sin6_port")), 22, "{t}");
        assert_eq!(num(&b, at(&d, t, "sin6_flowinfo")), 0x0001_2345, "{t}");
        assert_eq!(field(&b, at(&d, t, "sin6_addr")), &ip.octets(), "{t}");
        assert_eq!(num(&b, at(&d, t, "sin6_scope_id")), 7, "{t}");
        assert_eq!(SockAddr::decode(&b, form), Ok(a.clone()), "{t}");
    }
}

#[test]
fn sockaddr_un_size() {
    let d = doc();
    let a = SockAddr::Unix("/tmp/s".into());
    assert_eq!(a.to_bytes(AddrForm::Bsd43).len(), size(&d, "sockaddr_un (BSD 4.3)"));
    assert_eq!(sockets::layout::addr::SUN_PATH_LEN, at(&d, "sockaddr_un (BSD 4.3)", "sun_path").1);
    assert_eq!(sockets::layout::addr::SUN_PATH_LEN, at(&d, "sockaddr_un (BSD 4.4)", "sun_path").1);
}

#[test]
fn msghdr_and_iovec_offsets() {
    let d = doc();
    for (c, f) in [
        (msghdr::NAME, "msg_name"),
        (msghdr::NAMELEN, "msg_namelen"),
        (msghdr::IOV, "msg_iov"),
        (msghdr::IOVLEN, "msg_iovlen"),
        (msghdr::CONTROL, "msg_control"),
        (msghdr::CONTROLLEN, "msg_controllen"),
        (msghdr::FLAGS, "msg_flags"),
    ] {
        assert_eq!(c, at(&d, "msghdr", f).0, "msghdr.{f}");
    }
    assert_eq!(msghdr::SIZE, size(&d, "msghdr"));
    assert_eq!(iovec::BASE, at(&d, "iovec", "iov_base").0);
    assert_eq!(iovec::LEN, at(&d, "iovec", "iov_len").0);
    assert_eq!(iovec::SIZE, size(&d, "iovec"));
}

#[test]
fn timeval_linger_pollfd() {
    let d = doc();
    let mut b = vec![0u8; size(&d, "timeval")];
    put_timeval(&mut b, Duration::from_micros(3_000_250));
    assert_eq!(num(&b, at(&d, "timeval", "tv_sec")), 3);
    assert_eq!(num(&b, at(&d, "timeval", "tv_usec")), 250);

    let mut b = vec![0u8; size(&d, "linger")];
    put_linger(&mut b, true, 45);
    assert_eq!(num(&b, at(&d, "linger", "l_onoff")), 1);
    assert_eq!(num(&b, at(&d, "linger", "l_linger")), 45);

    let mut b = vec![0u8; size(&d, "pollfd")];
    PollFd { fd: 9, events: 0x11, revents: 0x22 }.write(&mut b);
    assert_eq!(num(&b, at(&d, "pollfd", "fd")), 9);
    assert_eq!(num(&b, at(&d, "pollfd", "events")), 0x11);
    assert_eq!(num(&b, at(&d, "pollfd", "revents")), 0x22);
}

/// Whether the image has a pointer slot at `slot`.
fn has_ptr(im: &Image, slot: usize) -> bool {
    im.ptrs.iter().any(|(s, _)| *s == slot)
}

#[test]
fn hostent_servent_images() {
    let d = doc();
    let im = hostent("h", &["a".into()], 2, &[vec![1, 2, 3, 4]]);
    assert!(im.bytes.len() >= size(&d, "hostent"));
    for p in ["h_name", "h_aliases", "h_addr_list"] {
        assert_eq!(at(&d, "hostent", p).1, 16, "{p} is a pointer");
        assert!(has_ptr(&im, at(&d, "hostent", p).0), "hostent.{p}");
    }
    assert_eq!(num(&im.bytes, at(&d, "hostent", "h_addrtype")), 2);
    assert_eq!(num(&im.bytes, at(&d, "hostent", "h_length")), 4);

    let im = servent("ftp", &[], 21, "tcp");
    assert!(im.bytes.len() >= size(&d, "servent"));
    for p in ["s_name", "s_aliases", "s_proto"] {
        assert!(has_ptr(&im, at(&d, "servent", p).0), "servent.{p}");
    }
    assert_eq!(num(&im.bytes, at(&d, "servent", "s_port")), 21);
}

#[test]
fn addrinfo_chain() {
    let d = doc();
    let one = |port| AddrInfo {
        flags: 0x2,
        socktype: 1,
        protocol: 6,
        addr: SockAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)),
        canonname: Some("x".into()),
    };
    let im = addrinfo(&[one(1), one(2)], AddrForm::Bsd43);
    let z = size(&d, "addrinfo");
    for i in 0..2 {
        let b = &im.bytes[z * i..z * (i + 1)];
        assert_eq!(num(b, at(&d, "addrinfo", "ai_flags")), 2);
        assert_eq!(num(b, at(&d, "addrinfo", "ai_family")), 2);
        assert_eq!(num(b, at(&d, "addrinfo", "ai_socktype")), 1);
        assert_eq!(num(b, at(&d, "addrinfo", "ai_protocol")), 6);
        assert_eq!(num(b, at(&d, "addrinfo", "ai_addrlen")), 16);
        assert!(has_ptr(&im, z * i + at(&d, "addrinfo", "ai_addr").0));
        assert!(has_ptr(&im, z * i + at(&d, "addrinfo", "ai_canonname").0));
    }
    let next = at(&d, "addrinfo", "ai_next").0;
    assert!(im.ptrs.contains(&(next, z)), "the first ai_next points at the second");
    assert!(!has_ptr(&im, z + next), "the last ai_next is null");
}
