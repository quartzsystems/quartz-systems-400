// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Captures the kernel replies under `tests/fixtures/` that the codec
//! tests parse. Runs only from `tests/netns.sh capture` (root, fresh
//! netns, `QS400_CAPTURE_DIR` set).

#![cfg(target_os = "linux")]
#![allow(clippy::many_single_char_names, clippy::too_many_lines)]

use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;

use netlink::codec::{Builder, NLM_F_ACK, NLMSG_DONE, NLMSG_ERROR, messages};
use netlink::diag::{self, Proto};
use netlink::nft::{self, L4, Nft, PortRule};
use netlink::route::{self, Address, BondMode, BondSpec, Route, Rtnl};
use netlink::{Bus, Family, Socket};

/// Every datagram of the reply to `req`, as hex lines.
fn raw(bus: Bus, req: &Builder) -> String {
    let mut s = Socket::open(bus).unwrap();
    let seq = s.next_seq();
    s.send_bytes(&req.finish(seq)).unwrap();
    let mut out = String::new();
    loop {
        let d = s.recv().unwrap().to_vec();
        let _ = writeln!(out, "{}", hex(&d));
        let last = messages(&d).filter_map(Result::ok).any(|m| {
            m.kind == NLMSG_DONE || m.kind == NLMSG_ERROR
        });
        if last || req.flags() & netlink::codec::NLM_F_DUMP == 0 {
            return out;
        }
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

#[test]
fn capture_fixtures() {
    let Ok(dir) = std::env::var("QS400_CAPTURE_DIR") else {
        eprintln!("skipped: run net/netlink/tests/netns.sh capture");
        return;
    };
    let dir = PathBuf::from(dir);
    let save = |name: &str, text: String| std::fs::write(dir.join(name), text).unwrap();

    let mut r = Rtnl::open().unwrap();
    let lo = r.link_by_name("lo").unwrap();
    r.set_up(lo.index, true).unwrap();
    let d0 = r.create_simple("qsd0", "dummy").unwrap();
    r.set_mac(d0.index, &[2, 0, 0, 0, 0, 1]).unwrap();
    let bond = r
        .create_bond("qsbond0", &BondSpec::new(BondMode::Lacp))
        .unwrap();
    r.enslave(d0.index, bond.index).unwrap();
    let vlan = r.create_vlan("qsbond0.100", bond.index, 100).unwrap();
    for i in [d0.index, bond.index, vlan.index] {
        r.set_up(i, true).unwrap();
    }
    r.add_address(&Address::new(vlan.index, "192.0.2.10".parse().unwrap(), 24))
        .unwrap();
    let mut a6 = Address::new(vlan.index, "2001:db8::10".parse().unwrap(), 64);
    a6.flags = route::consts::IFA_F_NODAD;
    r.add_address(&a6).unwrap();
    let mut rt = Route::new(Family::V4, Some("198.51.100.0".parse().unwrap()), 24);
    rt.gateway = Some("192.0.2.1".parse().unwrap());
    rt.metric = Some(50);
    rt.mtu = Some(1300);
    r.add_route(&rt).unwrap();

    save("link_vlan.hex", raw(Bus::Route, &route::link::get_by_index(vlan.index)));
    save("link_bond.hex", raw(Bus::Route, &route::link::get_by_index(bond.index)));
    save("link_slave.hex", raw(Bus::Route, &route::link::get_by_index(d0.index)));
    save("addr_dump.hex", raw(Bus::Route, &route::addr::dump_request(None)));
    save("route_dump_v4.hex", raw(Bus::Route, &route::rt::dump_request(Some(Family::V4))));
    let mut exists = route::addr::request(&Address::new(vlan.index, "192.0.2.10".parse().unwrap(), 24), true).unwrap();
    exists.add_flags(NLM_F_ACK);
    save("error_eexist.hex", raw(Bus::Route, &exists));

    let l = TcpListener::bind("127.0.0.1:8470").unwrap();
    let mut c = TcpStream::connect("127.0.0.1:8470").unwrap();
    let (mut s, _) = l.accept().unwrap();
    c.write_all(&[1u8; 300]).unwrap();
    let mut b = [0u8; 300];
    s.read_exact(&mut b).unwrap();
    let l6 = TcpListener::bind("[::1]:8471").unwrap();
    save("proc_net_tcp.txt", std::fs::read_to_string("/proc/net/tcp").unwrap());
    save("proc_net_tcp6.txt", std::fs::read_to_string("/proc/net/tcp6").unwrap());
    drop(l6);
    save(
        "diag_tcp4.hex",
        raw(Bus::SockDiag, &diag::dump_request(Family::V4, Proto::Tcp, diag::ALL_STATES, true)),
    );

    let mut n = Nft::open().unwrap();
    n.remove_all().unwrap();
    // Kernels without NFT_SOCKET (WSL) cannot hold the rule.
    match n.add_port_drop(&PortRule::new(L4::Tcp, 8470, 8476).unwrap()) {
        Ok(_) => save("nft_rules.hex", raw(Bus::Netfilter, &nft::list_rules())),
        Err(e) if e.is_not_found() => eprintln!("nft: no socket expression, rules not captured"),
        Err(e) => panic!("nft: {e}"),
    }
    n.remove_all().unwrap();
}
