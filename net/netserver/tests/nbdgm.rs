// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `NetBIOS` datagram service over loopback UDP: the socket loop
//! announces the server to a stand-in broadcast address at its start and
//! at each interval, answers an `AnnouncementRequest` and says it is gone
//! when it stops.

use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use netserver::nbdgm::packet::{
    ANNOUNCEMENT_REQUEST, Announcement, BROADCAST, BROWSE, mailslot_write, parse_mailslot,
};
use netserver::nbdgm::service::{PORT, SUFFIX_MASTER_BROWSER};
use netserver::nbdgm::{AnnounceConfig, Announcer, Datagram, serve};
use netserver::nbns::Datagrams;
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

/// The next announcement on `net` and when it came.
fn next(net: &UdpSocket) -> (Datagram, Announcement, Instant) {
    let mut b = [0u8; 1500];
    let (n, _) = net.recv_from(&mut b).expect("an announcement in time");
    let d = Datagram::parse(&b[..n]).unwrap();
    let (slot, frame) = parse_mailslot(&d.data).unwrap();
    assert_eq!(slot, BROWSE);
    let a = Announcement::parse(frame).unwrap();
    (d, a, Instant::now())
}

#[test]
fn announces_answers_a_request_and_says_goodbye() {
    let net = UdpSocket::bind((LO, 0)).unwrap();
    net.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let sock = UdpSocket::bind((LO, 0)).unwrap();
    let server = v4(sock.local_addr().unwrap());
    let mut d = Loop {
        sock,
        net: v4(net.local_addr().unwrap()),
    };
    let cfg = AnnounceConfig {
        server: "QS400".into(),
        workgroup: "WORKGROUP".into(),
        comment: "Quartz Systems".into(),
        printing: false,
        interval: Duration::from_secs(3),
    };
    let stop = Arc::new(AtomicBool::new(false));
    let s = stop.clone();
    let t = std::thread::spawn(move || serve(&mut d, &mut Announcer::new(cfg), &s));

    // At the start, to WORKGROUP<1D> from QS400<00> at the server's address.
    let (dg, first, t0) = next(&net);
    assert!(dg.dest.is("WORKGROUP", SUFFIX_MASTER_BROWSER));
    assert!(dg.source.is("QS400", 0));
    assert_eq!((dg.source_ip, dg.source_port), (LO, PORT));
    assert_eq!(first.server, "QS400");
    assert_eq!(first.comment, "Quartz Systems");
    assert_eq!(first.periodicity, 3000);
    assert_ne!(first.server_type, 0);

    // A browser asks: the next one comes sooner than the interval would
    // have it only when the random wait is short, so just within 30 s.
    let ask = Datagram {
        kind: BROADCAST,
        id: 7,
        source_ip: LO,
        source_port: PORT,
        source: NbName::new("MASTER", 0),
        dest: NbName::new("WORKGROUP", 0),
        data: mailslot_write(BROWSE, &[ANNOUNCEMENT_REQUEST, 0, b'M', 0]),
    };
    net.send_to(&ask.encode(), server).unwrap();
    let (_, second, t1) = next(&net);
    assert!(t1 - t0 <= Duration::from_secs(4), "{:?}", t1 - t0);
    assert_eq!(second.server, "QS400");

    stop.store(true, Ordering::Release);
    t.join().unwrap();
    // The last one says the server is gone.
    let mut last = next(&net).1;
    while last.server_type != 0 {
        last = next(&net).1;
    }
    assert_eq!(last.periodicity, 0);
}
