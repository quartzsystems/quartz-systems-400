// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The binder over real loopback sockets with its client, its refusals,
//! and `/etc/RPCBTAB` against the oracle's file.

#![allow(clippy::many_single_char_names, clippy::items_after_statements)]

mod wire_common;

use std::net::{SocketAddr, TcpListener, UdpSocket};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use nfs::rpc::msg::{AcceptError, encode_call};
use nfs::rpc::record::MAX_RECORD;
use nfs::rpc::{
    ClientConfig, Credential, Dispatcher, Proto, RpcClient, RpcError, Transport, auth,
    serve_datagram, serve_tcp,
};
use nfs::rpcb::{Binder, BinderClient, Table, rpcbtab};
use nfs::types::rpc::call_body;
use nfs::types::rpcb::{RPCBPROG, rpcb};
use wire_common::{hex, seed};

fn reg(prog: u32, vers: u32, netid: &str, addr: &str) -> rpcb {
    rpcb {
        r_prog: prog,
        r_vers: vers,
        r_netid: netid.into(),
        r_addr: addr.into(),
        r_owner: b"4294967295".to_vec(),
    }
}

/// A binder on loopback TCP and UDP; what it last wrote to RPCBTAB.
fn serve() -> (SocketAddr, SocketAddr, Arc<Mutex<Vec<u8>>>) {
    let written = Arc::new(Mutex::new(Vec::new()));
    let w = written.clone();
    let b = Binder::new(Table::new())
        .on_change(move |e| *w.lock().unwrap() = rpcbtab::encode(e).unwrap());
    let mut d = Dispatcher::new();
    d.add(Arc::new(b));
    let d = Arc::new(d);
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let ta = l.local_addr().unwrap();
    let dt = d.clone();
    thread::spawn(move || {
        for s in l.incoming().flatten() {
            let d = dt.clone();
            thread::spawn(move || {
                let _ = serve_tcp(&s, &d, MAX_RECORD);
            });
        }
    });
    let u = UdpSocket::bind("127.0.0.1:0").unwrap();
    let ua = u.local_addr().unwrap();
    thread::spawn(move || while serve_datagram(&u, &d).is_ok() {});
    (ta, ua, written)
}

fn cfg() -> ClientConfig {
    ClientConfig {
        timeout: Duration::from_secs(2),
        ..ClientConfig::default()
    }
}

#[test]
fn register_look_up_dump_and_remove_over_tcp_and_udp() {
    let (ta, ua, written) = serve();
    let mut c = BinderClient::at(ta, Proto::Tcp, cfg());
    assert!(c.set(&reg(100_003, 3, "tcp", "0.0.0.0.8.1")).unwrap());
    assert!(c.set(&reg(100_003, 3, "udp", "0.0.0.0.8.1")).unwrap());
    assert!(c.set(&reg(100_005, 3, "tcp", "::.3.232")).unwrap());
    // A registration taken is refused.
    assert!(!c.set(&reg(100_003, 3, "tcp", "0.0.0.0.8.2")).unwrap());
    // GETADDR answers the binder's host for a wildcard.
    assert_eq!(
        c.getaddr(100_003, 3, "tcp").unwrap(),
        Some("127.0.0.1:2049".parse().unwrap())
    );
    assert_eq!(
        c.getaddr(100_005, 3, "tcp").unwrap(),
        Some("127.0.0.1:1000".parse().unwrap())
    );
    assert_eq!(c.getaddr(100_021, 4, "tcp").unwrap(), None);
    // Portmap over UDP sees the same table.
    let mut u = BinderClient::at(ua, Proto::Udp, cfg());
    assert_eq!(u.getport(100_003, 3, 17).unwrap(), Some(2049));
    assert_eq!(u.getport(100_003, 3, 99).unwrap(), None);
    // Another version of a registered program: its port, as rpcbind answers.
    assert_eq!(u.getport(100_003, 4, 6).unwrap(), Some(2049));
    assert_eq!(c.dump().unwrap().len(), 3);
    // What was written is what is registered.
    assert_eq!(rpcbtab::decode(&written.lock().unwrap()).unwrap().len(), 3);
    assert!(c.unset(&reg(100_003, 3, "", "")).unwrap());
    assert_eq!(c.dump().unwrap().len(), 1);
    assert!(!c.unset(&reg(100_003, 3, "", "")).unwrap());
}

/// A call to the binder as if from `from`.
fn ask(
    d: &Dispatcher,
    from: &str,
    vers: u32,
    procedure: u32,
    args: &[u8],
    t: Transport,
) -> Option<Vec<u8>> {
    let body = call_body {
        rpcvers: 2,
        prog: 100_000,
        vers,
        proc: procedure,
        cred: auth::none(),
        verf: auth::none(),
    };
    d.handle(
        &encode_call(1, &body, args).unwrap(),
        from.parse().unwrap(),
        t,
    )
}

#[test]
fn set_from_another_host_and_remote_calls_are_refused() {
    let mut d = Dispatcher::new();
    d.add(Arc::new(Binder::default()));
    let set =
        hex("000186a3 00000003 00000003 74637000 0000000b 302e302e 302e302e 382e3100 00000000");
    // FALSE from another host, TRUE from this one.
    let r = ask(&d, "10.0.2.2:800", 4, 1, &set, Transport::Tcp).unwrap();
    assert_eq!(&r[r.len() - 4..], &[0, 0, 0, 0]);
    let r = ask(&d, "127.0.0.1:800", 4, 1, &set, Transport::Tcp).unwrap();
    assert_eq!(&r[r.len() - 4..], &[0, 0, 0, 1]);
    // CALLIT, BCAST, INDIRECT: PROC_UNAVAIL over TCP, nothing over UDP.
    let rmt = hex("000186a3 00000003 00000000 00000000");
    for (v, p) in [(2, 5), (3, 5), (4, 5), (4, 10)] {
        let r = ask(&d, "127.0.0.1:800", v, p, &rmt, Transport::Tcp).unwrap();
        assert_eq!(
            nfs::rpc::client::results(&r).unwrap_err().to_string(),
            AcceptError::ProcUnavail.to_string()
        );
        assert_eq!(ask(&d, "127.0.0.1:800", v, p, &rmt, Transport::Udp), None);
    }
    // Version 5 is PROG_MISMATCH 2 to 4.
    let r = ask(&d, "127.0.0.1:800", 5, 0, &[], Transport::Udp).unwrap();
    assert_eq!(
        nfs::rpc::client::results(&r).unwrap_err().mismatch(),
        Some((2, 4))
    );
    seed(
        "nfsrpcb",
        &[
            2, 1, 0, 1, 0x86, 0xa3, 0, 0, 0, 3, 0, 0, 0, 3, b't', b'c', b'p', 0, 0, 0, 0, 11,
        ],
    );
}

#[test]
fn versaddr_addrlist_stat_time_and_taddr() {
    let (ta, _, _) = serve();
    let mut c = BinderClient::at(ta, Proto::Tcp, cfg());
    assert!(c.set(&reg(100_021, 4, "tcp", "0.0.0.0.4.1")).unwrap());
    assert!(c.set(&reg(100_021, 4, "udp", "0.0.0.0.4.2")).unwrap());
    use RPCBPROG::RPCBVERS4::{Args, Res};
    let rpc = c.rpc();
    let q = |vers| rpcb {
        r_prog: 100_021,
        r_vers: vers,
        r_netid: b"tcp".to_vec(),
        r_addr: vec![],
        r_owner: vec![],
    };
    // GETVERSADDR takes only the version asked.
    let r: Res = rpc
        .call_proc(&Credential::None, &Args::RPCBPROC_GETVERSADDR(q(3)))
        .unwrap();
    assert!(matches!(r, Res::RPCBPROC_GETVERSADDR(s) if s.0.is_empty()));
    let r: Res = rpc
        .call_proc(&Credential::None, &Args::RPCBPROC_GETVERSADDR(q(4)))
        .unwrap();
    assert!(matches!(r, Res::RPCBPROC_GETVERSADDR(s) if s.0 == b"0.0.0.0.4.1"));
    // GETADDRLIST: both transports.
    let mut all = q(4);
    all.r_netid.clear();
    let r: Res = rpc
        .call_proc(&Credential::None, &Args::RPCBPROC_GETADDRLIST(all))
        .unwrap();
    let Res::RPCBPROC_GETADDRLIST(l) = r else {
        panic!()
    };
    let n: Vec<&[u8]> =
        l.0.iter()
            .map(|e| e.rpcb_entry_map.r_nc_netid.as_slice())
            .collect();
    assert_eq!(n, vec![b"tcp".as_slice(), b"udp".as_slice()]);
    assert_eq!(l.0[0].rpcb_entry_map.r_nc_protofmly, b"inet");
    // GETSTAT counts what was asked of version 4.
    let r: Res = rpc
        .call_proc(&Credential::None, &Args::RPCBPROC_GETSTAT)
        .unwrap();
    let Res::RPCBPROC_GETSTAT(s) = r else {
        panic!()
    };
    assert_eq!(s.0.len(), 3);
    assert_eq!(s.0[2].setinfo, 2);
    assert_eq!(s.0[2].info.0[9], 2);
    assert_eq!(s.0[2].addrinfo.0.len(), 3);
    // GETTIME is near now.
    let r: Res = rpc
        .call_proc(&Credential::None, &Args::RPCBPROC_GETTIME)
        .unwrap();
    let Res::RPCBPROC_GETTIME(t) = r else {
        panic!()
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert!(u64::from(t).abs_diff(now) < 60);
    // UADDR2TADDR and back.
    let r: Res = rpc
        .call_proc(
            &Credential::None,
            &Args::RPCBPROC_UADDR2TADDR(nfs::types::rpcb::rpcb_string(b"10.1.2.3.8.1".to_vec())),
        )
        .unwrap();
    let Res::RPCBPROC_UADDR2TADDR(nb) = r else {
        panic!()
    };
    assert_eq!(nb.buf.len(), 16);
    let r: Res = rpc
        .call_proc(&Credential::None, &Args::RPCBPROC_TADDR2UADDR(nb))
        .unwrap();
    assert!(matches!(r, Res::RPCBPROC_TADDR2UADDR(s) if s.0 == b"10.1.2.3.8.1"));
}

#[test]
fn an_older_binder_is_asked_with_version_3() {
    // A dispatcher whose binder has versions 2 and 3 only.
    struct Old(Binder);
    impl nfs::rpc::Program for Old {
        fn program(&self) -> u32 {
            100_000
        }
        fn versions(&self) -> &[u32] {
            &[2, 3]
        }
        fn call(
            &self,
            ctx: &nfs::rpc::CallContext,
            d: &mut nfs::xdr::Decoder<'_>,
        ) -> nfs::rpc::Answer {
            self.0.call(ctx, d)
        }
    }
    let b = Binder::default();
    assert!(b.register(reg(100_005, 3, "tcp", "0.0.0.0.3.232")));
    let mut d = Dispatcher::new();
    d.add(Arc::new(Old(b)));
    let d = Arc::new(d);
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let a = l.local_addr().unwrap();
    thread::spawn(move || {
        for s in l.incoming().flatten() {
            let d = d.clone();
            thread::spawn(move || {
                let _ = serve_tcp(&s, &d, MAX_RECORD);
            });
        }
    });
    let mut c = BinderClient::at(a, Proto::Tcp, cfg());
    assert_eq!(
        c.getaddr(100_005, 3, "tcp").unwrap(),
        Some("127.0.0.1:1000".parse().unwrap())
    );
    let mut r = RpcClient::new(a, Proto::Tcp, cfg());
    assert!(matches!(
        r.call_raw(100_000, 4, 0, &Credential::None, &[]),
        Err(RpcError::Refused(_))
    ));
}

#[test]
fn rpcbtab_as_the_oracle_wrote_it() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tools/conformance/oracle/nfs/etc/RPCBTAB.bin"
    );
    let b = std::fs::read(path).unwrap();
    assert_eq!(b.len(), 24 * rpcbtab::RECORD);
    let t = rpcbtab::decode(&b).unwrap();
    assert_eq!(t.len(), 24);
    assert_eq!(t[0], reg(100_003, 4, "tcp", "::.8.1"));
    let binder: Vec<&rpcb> = t.iter().filter(|r| r.r_prog == 100_000).collect();
    assert_eq!(binder.len(), 4);
    assert!(
        binder
            .iter()
            .all(|r| r.r_addr == b"::.0.111" && r.r_owner == b"0")
    );
    // Written again, byte for byte.
    assert_eq!(rpcbtab::encode(&t).unwrap(), b);
    seed("nfsrpcb", &b);
}
