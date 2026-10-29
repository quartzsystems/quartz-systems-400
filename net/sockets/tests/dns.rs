// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The stub resolver against a name server on loopback that this test
//! runs: A, AAAA, CNAME and PTR answers, NXDOMAIN, the search list,
//! a truncated UDP answer retried over TCP, a lost query retried, and
//! `HOSTSCHPTY` ordering against the host table.

#![cfg(unix)]

use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, TcpListener, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use sockets::consts::{AF_INET, AF_INET6, C_IN, T_A, T_AAAA, T_PTR};
use sockets::resolver::codec::{Message, Rdata, Record, rcode};
use sockets::resolver::{
    DefaultServices, DomainConfig, HostEntry, HostError, Resolver, SearchPriority, StaticHostConfig,
};

/// The answer to one question.
fn answer(q: &Message, tcp: bool, lost: &AtomicUsize) -> Option<Message> {
    let question = q.questions.first()?;
    let name = question.name.to_ascii_lowercase();
    let mut m = Message {
        header: q.header,
        questions: q.questions.clone(),
        ..Message::default()
    };
    m.header.qr = true;
    m.header.ra = true;
    let a = |n: &str, ip: [u8; 4]| Record::new(n, 60, Rdata::A(Ipv4Addr::from(ip)));
    match (name.as_str(), question.qtype) {
        ("www.example.test", T_A) => {
            m.answers.push(Record::new("www.example.test", 60, Rdata::Cname("web.example.test".into())));
            m.answers.push(a("web.example.test", [192, 0, 2, 10]));
            m.answers.push(a("web.example.test", [192, 0, 2, 11]));
        }
        ("www.example.test", T_AAAA) => {
            m.answers.push(Record::new("www.example.test", 60, Rdata::Aaaa("2001:db8::10".parse().unwrap())));
        }
        ("v4only.example.test", T_AAAA) => {}
        ("v4only.example.test", T_A) => m.answers.push(a("v4only.example.test", [192, 0, 2, 4])),
        ("10.2.0.192.in-addr.arpa", T_PTR) => {
            m.answers.push(Record::new(&name, 60, Rdata::Ptr("web.example.test".into())));
        }
        ("big.example.test", T_A) => {
            if tcp {
                for i in 0..40u8 {
                    m.answers.push(a("big.example.test", [198, 51, 100, i]));
                }
            } else {
                m.header.tc = true;
            }
        }
        ("retry.example.test", T_A) => {
            if lost.fetch_add(1, Ordering::SeqCst) == 0 {
                return None;
            }
            m.answers.push(a("retry.example.test", [192, 0, 2, 99]));
        }
        ("local.example.test", T_A) => m.answers.push(a("local.example.test", [192, 0, 2, 50])),
        _ => m.header.rcode = rcode::NXDOMAIN,
    }
    Some(m)
}

/// A name server on one loopback port, UDP and TCP.
fn server() -> u16 {
    let lost = Arc::new(AtomicUsize::new(0));
    let (udp, tcp) = loop {
        let u = UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = u.local_addr().unwrap().port();
        if let Ok(t) = TcpListener::bind(("127.0.0.1", port)) {
            break (u, t);
        }
    };
    let port = udp.local_addr().unwrap().port();
    let l2 = lost.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 512];
        loop {
            let Ok((n, from)) = udp.recv_from(&mut buf) else { return };
            let Ok(q) = Message::decode(&buf[..n]) else { continue };
            if let Some(a) = answer(&q, false, &l2) {
                udp.send_to(&a.encode().unwrap(), from).unwrap();
            }
        }
    });
    std::thread::spawn(move || {
        for s in tcp.incoming() {
            let Ok(mut s) = s else { return };
            let mut l = [0u8; 2];
            if s.read_exact(&mut l).is_err() {
                continue;
            }
            let mut q = vec![0u8; usize::from(u16::from_be_bytes(l))];
            s.read_exact(&mut q).unwrap();
            let q = Message::decode(&q).unwrap();
            if let Some(a) = answer(&q, true, &lost) {
                let b = a.encode().unwrap();
                let mut out = u16::try_from(b.len()).unwrap().to_be_bytes().to_vec();
                out.extend_from_slice(&b);
                s.write_all(&out).unwrap();
            }
        }
    });
    port
}

fn resolver(port: u16, priority: SearchPriority, hosts: Vec<HostEntry>) -> Resolver {
    Resolver::new(
        Box::new(StaticHostConfig {
            domain: DomainConfig {
                domain_name: "example.test".into(),
                servers: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
                port,
                priority,
                initial_query_interval: Duration::from_millis(200),
                query_timeout: Duration::from_secs(5),
                ..DomainConfig::default()
            },
            hosts,
        }),
        Box::new(DefaultServices),
    )
}

#[test]
fn names_resolve_through_the_server() {
    let port = server();
    let mut r = resolver(port, SearchPriority::Local, Vec::new());
    let h = r.by_name("www.example.test", AF_INET).unwrap();
    assert_eq!(h.name, "web.example.test", "the CNAME's target is the official name");
    assert_eq!(h.aliases, vec!["www.example.test".to_string()]);
    assert_eq!(h.addrs, vec![IpAddr::from([192, 0, 2, 10]), IpAddr::from([192, 0, 2, 11])]);
    let six = r.by_name("www", AF_INET6).unwrap();
    assert_eq!(six.addrs, vec![IpAddr::V6("2001:db8::10".parse::<Ipv6Addr>().unwrap())], "through the search list");
    assert_eq!(r.by_name("nosuch.example.test", AF_INET), Err(HostError::NotFound));
    assert_eq!(r.by_name("v4only.example.test", AF_INET6), Err(HostError::NoData));
    let back = r.by_addr(IpAddr::from([192, 0, 2, 10])).unwrap();
    assert_eq!(back.name, "web.example.test");
    let raw = r.query("www.example.test", C_IN, T_A).unwrap();
    assert_eq!(Message::decode(&raw).unwrap().answers.len(), 3);
    assert!(r.search("www", C_IN, T_A).is_ok());
    assert_eq!(r.query("www", C_IN, T_A), Err(HostError::NotFound), "res_query takes the name as given");
}

#[test]
fn truncated_answers_come_over_tcp_and_lost_queries_are_retried() {
    let port = server();
    let mut r = resolver(port, SearchPriority::Local, Vec::new());
    let big = r.by_name("big.example.test", AF_INET).unwrap();
    assert_eq!(big.addrs.len(), 40);
    let retried = r.by_name("retry.example.test", AF_INET).unwrap();
    assert_eq!(retried.addrs, vec![IpAddr::from([192, 0, 2, 99])]);
}

#[test]
fn search_priority_orders_the_host_table_and_the_server() {
    let port = server();
    let table = vec![HostEntry {
        addr: IpAddr::from([10, 9, 9, 9]),
        names: vec!["LOCAL.EXAMPLE.TEST".into()],
    }];
    let mut local = resolver(port, SearchPriority::Local, table.clone());
    assert_eq!(local.by_name("local.example.test", AF_INET).unwrap().addrs, vec![IpAddr::from([10, 9, 9, 9])]);
    let mut remote = resolver(port, SearchPriority::Remote, table);
    assert_eq!(remote.by_name("local.example.test", AF_INET).unwrap().addrs, vec![IpAddr::from([192, 0, 2, 50])]);
    assert_eq!(remote.by_name("LOCALHOST", AF_INET), Err(HostError::NotFound), "not in this table");
}

#[test]
fn an_unanswered_lookup_is_try_again() {
    // A port nobody answers on.
    let silent = UdpSocket::bind("127.0.0.1:0").unwrap();
    let port = silent.local_addr().unwrap().port();
    let mut r = Resolver::new(
        Box::new(StaticHostConfig {
            domain: DomainConfig {
                servers: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
                port,
                initial_query_interval: Duration::from_millis(50),
                query_timeout: Duration::from_secs(2),
                retries: 1,
                ..DomainConfig::default()
            },
            hosts: Vec::new(),
        }),
        Box::new(DefaultServices),
    );
    assert_eq!(r.by_name("x.example.test", AF_INET), Err(HostError::TryAgain));
    drop(silent);
}
