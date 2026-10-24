// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Against the real kernel, as root in a fresh network namespace:
//! `net/netlink/tests/netns.sh` runs these with `QS400_NETNS=1`.
//! Without it they return at once.

#![cfg(target_os = "linux")]
#![allow(clippy::many_single_char_names, clippy::too_many_lines)]

use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::time::Duration;

use netlink::diag::{Diag, Proto, TcpState};
use netlink::nft::{L4, Nft, PortRule};
use netlink::route::consts::{IFA_F_NODAD, IFA_F_PERMANENT};
use netlink::route::{Address, BondMode, BondSpec, LinkEvents, Route, Rtnl};
use netlink::{Family, procnet};

fn in_netns() -> bool {
    let on = std::env::var("QS400_NETNS").is_ok_and(|v| v == "1");
    if !on {
        eprintln!("skipped: run net/netlink/tests/netns.sh (root, fresh netns)");
    }
    on
}

/// Running on the QS/400 kernel (QEMU), where every option is on.
fn own_kernel() -> bool {
    std::env::var("QS400_OWN_KERNEL").is_ok_and(|v| v == "1")
}

fn lo_up(r: &mut Rtnl) {
    let lo = r.link_by_name("lo").unwrap();
    r.set_up(lo.index, true).unwrap();
}

#[test]
fn links_bonds_vlans_addresses_routes() {
    if !in_netns() {
        return;
    }
    let mut r = Rtnl::open().unwrap();
    lo_up(&mut r);
    let mut events = LinkEvents::open().unwrap();
    events.set_timeout(Some(Duration::from_secs(2))).unwrap();

    let d0 = r.create_simple("qsd0", "dummy").unwrap();
    let d1 = r.create_simple("qsd1", "dummy").unwrap();
    assert_eq!(d0.kind.as_deref(), Some("dummy"));
    r.set_mtu(d0.index, 1400).unwrap();
    r.set_mac(d0.index, &[2, 0, 0, 0, 0, 1]).unwrap();
    let d0 = r.link_by_index(d0.index).unwrap();
    assert_eq!(d0.mtu, 1400);
    assert_eq!(d0.mac, vec![2, 0, 0, 0, 0, 1]);
    assert!(r.link_by_name("nosuch0").unwrap_err().is_not_found());

    // Events: the dummies appeared.
    let mut seen = Vec::new();
    while let Ok(evs) = events.wait() {
        seen.extend(evs.into_iter().map(|e| e.link.name));
        if seen.iter().any(|n| n == "qsd1") {
            break;
        }
    }
    assert!(seen.iter().any(|n| n == "qsd0"), "events: {seen:?}");

    let bond = r
        .create_bond("qsbond0", &BondSpec::new(BondMode::ActiveBackup))
        .unwrap();
    assert_eq!(bond.kind.as_deref(), Some("bond"));
    assert_eq!(bond.bond_mode, Some(BondMode::ActiveBackup));
    r.enslave(d0.index, bond.index).unwrap();
    r.enslave(d1.index, bond.index).unwrap();
    let d1 = r.link_by_index(d1.index).unwrap();
    assert_eq!(d1.master, Some(bond.index));
    assert_eq!(d1.slave_kind.as_deref(), Some("bond"));

    let vlan = r.create_vlan("qsbond0.100", bond.index, 100).unwrap();
    assert_eq!(vlan.vlan_id, Some(100));
    assert_eq!(vlan.parent, Some(bond.index));
    for i in [d0.index, d1.index, bond.index, vlan.index] {
        r.set_up(i, true).unwrap();
    }

    // Carrier: the bond comes up with its dummy slaves.
    let mut carrier = false;
    while let Ok(evs) = events.wait() {
        if evs
            .iter()
            .any(|e| e.link.name == "qsbond0" && e.link.has_carrier())
        {
            carrier = true;
            break;
        }
    }
    assert!(carrier, "no carrier event for the bond");

    let v4 = Address::new(vlan.index, "192.0.2.10".parse().unwrap(), 24);
    let mut v6 = Address::new(vlan.index, "2001:db8::10".parse().unwrap(), 64);
    v6.flags = IFA_F_NODAD;
    r.add_address(&v4).unwrap();
    r.add_address(&v6).unwrap();
    assert!(r.add_address(&v4).unwrap_err().is_exists());
    let addrs = r.addresses(None).unwrap();
    let got4 = addrs.iter().find(|a| a.addr == v4.addr).unwrap();
    assert_eq!(got4.prefix, 24);
    assert_eq!(got4.broadcast, Some("192.0.2.255".parse().unwrap()));
    assert_eq!(got4.label.as_deref(), Some("qsbond0.100"));
    let got6 = addrs.iter().find(|a| a.addr == v6.addr).unwrap();
    assert_ne!(got6.flags & IFA_F_PERMANENT, 0);
    assert_ne!(got6.flags & IFA_F_NODAD, 0);

    let mut rt4 = Route::new(Family::V4, Some("198.51.100.0".parse().unwrap()), 24);
    rt4.gateway = Some("192.0.2.1".parse().unwrap());
    rt4.metric = Some(50);
    rt4.mtu = Some(1300);
    rt4.tos = 0x10;
    // A table other than main; IPv6 has one table on the QS/400 kernel.
    rt4.table = 1000;
    r.add_route(&rt4).unwrap();
    let mut rt6 = Route::new(Family::V6, Some("2001:db8:1::".parse().unwrap()), 48);
    rt6.gateway = Some("2001:db8::1".parse().unwrap());
    rt6.oif = Some(vlan.index);
    r.add_route(&rt6).unwrap();
    let mut def = Route::new(Family::V4, None, 0);
    def.gateway = Some("192.0.2.1".parse().unwrap());
    r.add_route(&def).unwrap();

    let routes = r.routes(None).unwrap();
    let g4 = routes.iter().find(|x| x.dst == rt4.dst).unwrap();
    assert_eq!(g4.gateway, rt4.gateway);
    assert_eq!(g4.metric, Some(50));
    assert_eq!(g4.mtu, Some(1300));
    assert_eq!(g4.tos, 0x10);
    assert_eq!(g4.oif, Some(vlan.index));
    assert_eq!(g4.protocol, netlink::route::consts::RTPROT_QS400);
    assert_eq!(g4.table, 1000);
    let g6 = routes.iter().find(|x| x.dst == rt6.dst).unwrap();
    assert_eq!(g6.table, netlink::route::consts::RT_TABLE_MAIN);
    assert_eq!(g6.gateway, rt6.gateway);
    assert!(routes.iter().any(|x| x.is_default() && x.gateway == def.gateway));

    r.delete_route(&rt4).unwrap();
    r.delete_route(&rt6).unwrap();
    r.delete_route(&def).unwrap();
    let routes = r.routes(None).unwrap();
    assert!(!routes.iter().any(|x| x.dst == rt4.dst || x.dst == rt6.dst));

    // The neighbor cache answers (a ping fills it; empty is fine here).
    r.neighbors(None).unwrap();
    r.neighbors(Some(Family::V6)).unwrap();

    r.delete_address(&v4).unwrap();
    r.delete_address(&v6).unwrap();
    assert!(!r.addresses(Some(Family::V4)).unwrap().iter().any(|a| a.addr == v4.addr));
    r.release(d1.index).unwrap();
    assert_eq!(r.link_by_index(d1.index).unwrap().master, None);
    r.delete_link(vlan.index).unwrap();
    r.delete_link(bond.index).unwrap();
    r.delete_link(d0.index).unwrap();
    r.delete_link(d1.index).unwrap();
    assert!(r.link_by_name("qsbond0").is_err());
}

#[test]
fn diag_finds_sockets_with_inode_and_owner() {
    if !in_netns() {
        return;
    }
    let mut r = Rtnl::open().unwrap();
    lo_up(&mut r);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let (mut server, _) = listener.accept().unwrap();
    client.write_all(&[7u8; 1000]).unwrap();
    let mut buf = [0u8; 1000];
    server.read_exact(&mut buf).unwrap();
    server.write_all(b"ok").unwrap();
    let mut two = [0u8; 2];
    client.read_exact(&mut two).unwrap();
    let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
    let v6 = TcpListener::bind("[::1]:0").unwrap();

    let mut d = Diag::open().unwrap();
    let tcp = d.sockets(Family::V4, Proto::Tcp).unwrap();
    let l = tcp
        .iter()
        .find(|s| s.state == TcpState::Listen && s.id.local.port() == port)
        .expect("listener in inet_diag");
    assert_ne!(l.inode, 0);
    let owners = procnet::socket_owners();
    assert_eq!(owners.get(&u64::from(l.inode)), Some(&std::process::id()));
    let table = procnet::read(procnet::Table::Tcp).unwrap();
    assert!(table.iter().any(|p| p.inode == u64::from(l.inode) && p.state == 10));

    let est = tcp
        .iter()
        .find(|s| s.state == TcpState::Established && s.id.remote.port() == port)
        .expect("client connection");
    let info = est.info.expect("tcp_info");
    assert!(info.bytes_acked >= 1000, "{info:?}");
    assert!(info.bytes_received >= 2, "{info:?}");
    assert_eq!(
        est.id.local,
        client.local_addr().unwrap(),
        "client side local address"
    );

    let u = d.sockets(Family::V4, Proto::Udp).unwrap();
    let up = udp.local_addr().unwrap().port();
    assert!(u.iter().any(|s| s.id.local.port() == up && s.inode != 0));
    let t6 = d.sockets(Family::V6, Proto::Tcp).unwrap();
    let p6 = v6.local_addr().unwrap().port();
    assert!(t6.iter().any(|s| s.id.local.port() == p6 && s.state == TcpState::Listen));

    // End the connection under its owner, if the kernel allows it.
    match d.destroy(est) {
        Ok(()) => {
            let mut b = [0u8; 1];
            assert!(matches!(client.read(&mut b), Err(_) | Ok(0)));
            eprintln!("SOCK_DESTROY: closed");
        }
        Err(e) if e.is_not_supported() && !own_kernel() => {
            eprintln!("SOCK_DESTROY: not in this kernel");
        }
        Err(e) => panic!("destroy: {e}"),
    }
}

fn connects(port: u16) -> bool {
    let a = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    TcpStream::connect_timeout(&a, Duration::from_millis(800)).is_ok()
}

/// A listener with `SO_MARK` set, as the sockets API will make.
fn marked_listener(port: u16, mark: u32) -> socket2::Socket {
    use socket2::{Domain, Socket, Type};
    let s = Socket::new(Domain::IPV4, Type::STREAM, None).unwrap();
    s.set_mark(mark).unwrap();
    let a = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    s.bind(&a.into()).unwrap();
    s.listen(8).unwrap();
    s
}

#[test]
fn nft_backstop_drops_unmarked_sockets() {
    if !in_netns() {
        return;
    }
    let mut r = Rtnl::open().unwrap();
    lo_up(&mut r);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    assert!(connects(port));

    let mut nft = Nft::open().unwrap();
    nft.remove_all().unwrap();
    assert!(nft.rules().unwrap().is_empty());
    let rule = PortRule::new(L4::Tcp, port, port.saturating_add(10)).unwrap();
    let handle = match nft.add_port_drop(&rule) {
        Err(e) if e.is_not_found() && !own_kernel() => {
            eprintln!("nft: this kernel lacks the socket expression (NFT_SOCKET)");
            nft.remove_all().unwrap();
            return;
        }
        r => r.unwrap(),
    };
    assert!(handle > 0);
    let rules = nft.rules().unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].comment.as_deref(), Some(rule.tag().as_str()));

    assert!(!connects(port), "an unmarked listener in the range must not be reached");

    // A socket bound through the API (marked) in the same range is reachable.
    let marked_port = port + 5;
    let marked = marked_listener(marked_port, netlink::nft::SOCKET_MARK | 0x12);
    assert!(connects(marked_port), "a marked listener must be reached");
    drop(marked);

    assert_eq!(nft.remove_port_drop(&rule).unwrap(), 1);
    assert!(connects(port));
    nft.add_port_drop(&PortRule::new(L4::Udp, 53, 53).unwrap()).unwrap();
    nft.remove_all().unwrap();
    nft.remove_all().unwrap();
    assert!(nft.rules().unwrap().is_empty());
}
