// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The codec against replies captured from the kernel (6.18) by
//! `tests/netns.sh capture`: a bond in 802.3ad mode with one dummy slave
//! (MAC 02:00:00:00:00:01), VLAN 100 on it with 192.0.2.10/24 and
//! `2001:db8::10/64`, a route to 198.51.100.0/24 via 192.0.2.1 metric 50
//! MTU 1300, and a TCP connection to 127.0.0.1:8470 after 300 bytes.
//! `nft_rules.hex` is from the QS/400 kernel under QEMU (WSL lacks
//! `NFT_SOCKET`): one rule for TCP 8470-8476.

use std::net::IpAddr;

use netlink::codec::{Msg, NLMSG_DONE, messages};
use netlink::diag::{self, Proto, TcpState};
use netlink::route::{self, BondMode, consts};
use netlink::{Family, nft, procnet};

fn datagrams(hex: &str) -> Vec<Vec<u8>> {
    hex.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| {
            (0..l.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&l[i..i + 2], 16).unwrap())
                .collect()
        })
        .collect()
}

fn each(hex: &str, mut f: impl FnMut(&Msg<'_>)) -> usize {
    let mut n = 0;
    for d in datagrams(hex) {
        for m in messages(&d) {
            let m = m.unwrap();
            if m.kind != NLMSG_DONE {
                f(&m);
                n += 1;
            }
        }
    }
    n
}

#[test]
fn parses_a_vlan_link() {
    let mut links = Vec::new();
    each(include_str!("fixtures/link_vlan.hex"), |m| {
        links.push(route::link::parse(m.payload).unwrap());
    });
    let l = &links[0];
    assert_eq!(l.name, "qsbond0.100");
    assert_eq!(l.kind.as_deref(), Some("vlan"));
    assert_eq!(l.vlan_id, Some(100));
    assert!(l.parent.is_some());
    assert_eq!(l.mac, vec![2, 0, 0, 0, 0, 1]);
    assert_eq!(l.mtu, 1500);
    assert!(l.is_up());
    assert!(l.stats.is_some());
}

#[test]
fn parses_a_bond_and_its_slave() {
    let mut bond = None;
    each(include_str!("fixtures/link_bond.hex"), |m| {
        bond = Some(route::link::parse(m.payload).unwrap());
    });
    let bond = bond.unwrap();
    assert_eq!(bond.name, "qsbond0");
    assert_eq!(bond.kind.as_deref(), Some("bond"));
    assert_eq!(bond.bond_mode, Some(BondMode::Lacp));
    assert_ne!(bond.flags & consts::IFF_MASTER, 0);

    let mut slave = None;
    each(include_str!("fixtures/link_slave.hex"), |m| {
        slave = Some(route::link::parse(m.payload).unwrap());
    });
    let slave = slave.unwrap();
    assert_eq!(slave.name, "qsd0");
    assert_eq!(slave.kind.as_deref(), Some("dummy"));
    assert_eq!(slave.slave_kind.as_deref(), Some("bond"));
    assert_eq!(slave.master, Some(bond.index));
    assert_ne!(slave.flags & consts::IFF_SLAVE, 0);
}

#[test]
fn parses_addresses() {
    let mut addrs = Vec::new();
    each(include_str!("fixtures/addr_dump.hex"), |m| {
        if let Some(a) = route::addr::parse(m.payload).unwrap() {
            addrs.push(a);
        }
    });
    let v4 = addrs
        .iter()
        .find(|a| a.addr == "192.0.2.10".parse::<IpAddr>().unwrap())
        .unwrap();
    assert_eq!(v4.prefix, 24);
    assert_eq!(v4.broadcast, Some("192.0.2.255".parse().unwrap()));
    assert_eq!(v4.label.as_deref(), Some("qsbond0.100"));
    assert_eq!(v4.scope, consts::RT_SCOPE_UNIVERSE);
    assert_ne!(v4.flags & consts::IFA_F_PERMANENT, 0);
    let v6 = addrs
        .iter()
        .find(|a| a.addr == "2001:db8::10".parse::<IpAddr>().unwrap())
        .unwrap();
    assert_eq!(v6.prefix, 64);
    assert_ne!(v6.flags & consts::IFA_F_NODAD, 0);
    assert_eq!(v6.valid, Some(route::addr::INFINITY_LIFE_TIME));
    assert!(addrs.iter().any(|a| a.addr == "127.0.0.1".parse::<IpAddr>().unwrap()));
}

#[test]
fn parses_routes() {
    let mut routes = Vec::new();
    each(include_str!("fixtures/route_dump_v4.hex"), |m| {
        if let Some(r) = route::rt::parse(m.payload).unwrap() {
            routes.push(r);
        }
    });
    let r = routes
        .iter()
        .find(|r| r.dst == Some("198.51.100.0".parse().unwrap()))
        .unwrap();
    assert_eq!(r.family, Family::V4);
    assert_eq!(r.dst_len, 24);
    assert_eq!(r.gateway, Some("192.0.2.1".parse().unwrap()));
    assert_eq!(r.metric, Some(50));
    assert_eq!(r.mtu, Some(1300));
    assert_eq!(r.table, consts::RT_TABLE_MAIN);
    assert_eq!(r.protocol, consts::RTPROT_QS400);
    // The kernel's connected route for the VLAN subnet.
    assert!(routes.iter().any(|r| r.dst == Some("192.0.2.0".parse().unwrap())
        && r.protocol == consts::RTPROT_KERNEL
        && r.scope == consts::RT_SCOPE_LINK));
}

#[test]
fn parses_an_error_reply() {
    let mut errs = Vec::new();
    each(include_str!("fixtures/error_eexist.hex"), |m| errs.push(m.error()));
    assert_eq!(errs, vec![Some(17)]);
}

#[test]
fn parses_inet_diag_with_tcp_info() {
    let mut socks = Vec::new();
    each(include_str!("fixtures/diag_tcp4.hex"), |m| {
        if let Some(s) = diag::parse(m.payload, Proto::Tcp).unwrap() {
            socks.push(s);
        }
    });
    let l = socks
        .iter()
        .find(|s| s.state == TcpState::Listen && s.id.local.port() == 8470)
        .unwrap();
    assert_eq!(l.id.local.ip(), "127.0.0.1".parse::<IpAddr>().unwrap());
    assert_ne!(l.inode, 0);
    assert_eq!(l.uid, 0);
    let c = socks
        .iter()
        .find(|s| s.state == TcpState::Established && s.id.remote.port() == 8470)
        .unwrap();
    let info = c.info.unwrap();
    assert_eq!(info.state, 1);
    assert_eq!(info.bytes_acked, 301, "SYN counts one");
    assert_eq!(info.bytes_sent, 300);
    assert_eq!(info.bytes_received, 0);
    assert!(info.rtt > 0 && info.snd_mss > 0 && info.pmtu == 65535);
    let s = socks
        .iter()
        .find(|s| s.state == TcpState::Established && s.id.local.port() == 8470)
        .unwrap();
    assert_eq!(s.info.unwrap().bytes_received, 300);
    assert_ne!(s.id.cookie, 0);
}

#[test]
fn parses_nft_rules() {
    let mut rules = Vec::new();
    each(include_str!("fixtures/nft_rules.hex"), |m| rules.extend(nft::parse_rule(m.payload)));
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].comment.as_deref(), Some("qs400 tcp 8470-8476"));
    assert!(rules[0].handle > 0);
}

#[test]
fn parses_proc_net_tables() {
    let t4 = procnet::parse(include_str!("fixtures/proc_net_tcp.txt"), false);
    let l = t4
        .iter()
        .find(|s| s.local == "127.0.0.1:8470".parse().unwrap() && s.state == 10)
        .unwrap();
    assert_ne!(l.inode, 0);
    assert!(t4.iter().any(|s| s.remote == "127.0.0.1:8470".parse().unwrap() && s.state == 1));
    let t6 = procnet::parse(include_str!("fixtures/proc_net_tcp6.txt"), true);
    assert!(t6.iter().any(|s| s.local == "[::1]:8471".parse().unwrap() && s.state == 10));
}
