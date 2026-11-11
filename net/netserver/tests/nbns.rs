// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `NetBIOS` name service over loopback UDP: the socket loop claims
//! the names on a stand-in broadcast address, answers queries and node
//! status, defends its name and releases the names when it stops.

#![allow(clippy::many_single_char_names)]

use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use netserver::nbns::packet::{
    Header, OP_QUERY, OP_REGISTRATION, OP_RELEASE, Question, RCODE_ACT_ERR, Resource, TYPE_NB,
    TYPE_NBSTAT,
};
use netserver::nbns::{Datagrams, NameConfig, NameService, Packet, serve};
use netserver::nbss::NbName;

const LO: Ipv4Addr = Ipv4Addr::LOCALHOST;

/// The server's socket on loopback; its broadcasts go to `net`.
struct Loop {
    sock: UdpSocket,
    net: SocketAddrV4,
}

impl Datagrams for Loop {
    fn recv(&mut self, wait: Duration) -> io::Result<Option<(Vec<u8>, SocketAddrV4)>> {
        self.sock
            .set_read_timeout(Some(wait.max(Duration::from_millis(1))))?;
        let mut b = [0u8; 1500];
        match self.sock.recv_from(&mut b) {
            Ok((n, SocketAddr::V4(a))) => Ok(Some((b[..n].to_vec(), a))),
            Ok(_) => Ok(None),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }
    fn send(&mut self, bytes: &[u8], to: SocketAddrV4) -> io::Result<()> {
        self.sock.send_to(bytes, to).map(|_| ())
    }
    fn broadcast(&self) -> SocketAddrV4 {
        self.net
    }
    fn local_ip(&self, _: Ipv4Addr) -> Option<Ipv4Addr> {
        Some(LO)
    }
}

fn v4(a: SocketAddr) -> SocketAddrV4 {
    match a {
        SocketAddr::V4(a) => a,
        SocketAddr::V6(_) => unreachable!(),
    }
}

fn next(net: &UdpSocket) -> Packet {
    let mut b = [0u8; 1500];
    let (n, _) = net.recv_from(&mut b).expect("a packet in time");
    Packet::parse(&b[..n]).unwrap()
}

fn request(id: u16, opcode: u8, name: &str, suffix: u8, qtype: u16) -> Packet {
    Packet {
        header: Header {
            id,
            opcode,
            rd: true,
            ..Header::default()
        },
        questions: vec![Question {
            name: NbName::new(name, suffix),
            qtype,
        }],
        ..Packet::default()
    }
}

#[test]
fn names_are_claimed_answered_defended_and_released() {
    let net = UdpSocket::bind((LO, 0)).unwrap();
    net.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let sock = UdpSocket::bind((LO, 0)).unwrap();
    let server = v4(sock.local_addr().unwrap());
    let mut d = Loop {
        sock,
        net: v4(net.local_addr().unwrap()),
    };
    let stop = Arc::new(AtomicBool::new(false));
    let s = stop.clone();
    let t = std::thread::spawn(move || {
        let mut svc = NameService::new(&NameConfig::new("QSTEST", "WORKGROUP"));
        serve(&mut d, &mut svc, &s);
    });

    // Three requests for each of three names, then three demands.
    let claims: Vec<Packet> = (0..12).map(|_| next(&net)).collect();
    for p in &claims {
        assert_eq!(p.header.opcode, OP_REGISTRATION);
        assert!(p.header.broadcast && !p.header.response);
        assert_eq!(p.additional[0].addresses()[0].1, LO);
    }
    assert_eq!(claims.iter().filter(|p| !p.header.rd).count(), 3);

    // A name query and a node status, unicast.
    let q = request(0x4242, OP_QUERY, "QSTEST", 0x20, TYPE_NB);
    net.send_to(&q.encode(), server).unwrap();
    let a = next(&net);
    assert!(a.header.response && a.header.aa && a.header.id == 0x4242);
    assert_eq!(a.answers[0].addresses(), [(0, LO)]);
    let q = request(0x4243, OP_QUERY, "*", 0, TYPE_NBSTAT);
    net.send_to(&q.encode(), server).unwrap();
    let a = next(&net);
    assert_eq!(a.answers[0].rtype, TYPE_NBSTAT);
    assert_eq!(a.answers[0].rdata[0], 3);

    // Another node registering QSTEST<20> is refused.
    let mut r = request(0x4244, OP_REGISTRATION, "QSTEST", 0x20, TYPE_NB);
    r.additional = vec![Resource::nb(
        NbName::new("QSTEST", 0x20),
        60,
        0,
        Ipv4Addr::new(127, 0, 0, 2),
    )];
    net.send_to(&r.encode(), server).unwrap();
    let a = next(&net);
    assert_eq!(a.header.rcode, RCODE_ACT_ERR);
    assert_eq!(a.answers[0].addresses(), [(0, LO)]);

    // Ended: each name released.
    stop.store(true, Ordering::Release);
    t.join().unwrap();
    let released: Vec<String> = (0..3)
        .map(|_| {
            let p = next(&net);
            assert_eq!(p.header.opcode, OP_RELEASE);
            let n = &p.questions[0].name;
            format!("{}<{:02X}>", n.name, n.suffix)
        })
        .collect();
    assert_eq!(released, ["QSTEST<00>", "QSTEST<20>", "WORKGROUP<00>"]);
}
