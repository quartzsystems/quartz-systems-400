// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The dispatcher's answers, the duplicate request cache, and the
//! client over real loopback sockets: TCP, UDP, retransmission,
//! reconnection, timeouts, interruption.

#![allow(clippy::many_single_char_names, clippy::items_after_statements)]

mod wire_common;

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use nfs::rpc::msg::{AcceptError, Denied, encode_call};
use nfs::rpc::record::{MAX_RECORD, frame, read_record};
use nfs::rpc::{
    Answer, CallContext, ClientConfig, Credential, Dispatcher, DuplicateCache, Program, Proto,
    RpcClient, RpcError, Transport, auth, serve_datagram, serve_tcp,
};
use nfs::types::rpc::{auth_stat, call_body, opaque_auth};
use nfs::xdr::{Decoder, Encoder};
use wire_common::{hex, hexs};

/// Program 400200 version 1: proc 1 echoes an opaque and counts calls.
struct Count(AtomicU32);

impl Program for Count {
    fn program(&self) -> u32 {
        400_200
    }
    fn versions(&self) -> &[u32] {
        &[1, 3]
    }
    fn call(&self, ctx: &CallContext, d: &mut Decoder<'_>) -> Answer {
        self.0.fetch_add(1, Ordering::SeqCst);
        match ctx.procedure {
            0 => Ok(Some(vec![])),
            1 => {
                let b = d.get_opaque(64).map_err(|_| AcceptError::GarbageArgs)?;
                if !d.is_empty() {
                    return Err(AcceptError::GarbageArgs);
                }
                let mut e = Encoder::new();
                e.put_opaque(&b, 64).map_err(|_| AcceptError::SystemErr)?;
                e.put_u32(ctx.cred.uid().unwrap_or(u32::MAX));
                Ok(Some(e.into_bytes()))
            }
            2 => Err(AcceptError::SystemErr),
            _ => Err(AcceptError::ProcUnavail),
        }
    }
}

fn dispatcher() -> (Dispatcher, Arc<Count>) {
    let c = Arc::new(Count(AtomicU32::new(0)));
    let mut d = Dispatcher::new();
    d.add(c.clone());
    (d, c)
}

fn peer() -> SocketAddr {
    "10.0.2.2:1001".parse().unwrap()
}

fn call(
    xid: u32,
    rpcvers: u32,
    prog: u32,
    vers: u32,
    procedure: u32,
    cred: opaque_auth,
    args: &[u8],
) -> Vec<u8> {
    let body = call_body {
        rpcvers,
        prog,
        vers,
        proc: procedure,
        cred,
        verf: auth::none(),
    };
    encode_call(xid, &body, args).unwrap()
}

fn opaque(b: &[u8]) -> Vec<u8> {
    let mut e = Encoder::new();
    e.put_opaque(b, 64).unwrap();
    e.into_bytes()
}

#[test]
fn the_dispatchers_answers() {
    let (d, _) = dispatcher();
    let none = auth::none;
    let ask = |m: &[u8]| d.handle(m, peer(), Transport::Udp).map(|r| hexs(&r));
    // A success: header, AUTH_NONE verifier, SUCCESS, the results.
    let r = ask(&call(7, 2, 400_200, 1, 1, none(), &opaque(b"hi")));
    assert_eq!(
        r,
        Some(hexs(&hex(
            "00000007 00000001 00000000 00000000 00000000 00000000 00000002 68690000 ffffffff"
        )))
    );
    // RPC version 3: RPC_MISMATCH 2 to 2.
    assert_eq!(
        ask(&call(8, 3, 400_200, 1, 0, none(), &[])),
        Some(hexs(&hex(
            "00000008 00000001 00000001 00000000 00000002 00000002"
        )))
    );
    // RPCSEC_GSS (and any flavor but AUTH_NONE and AUTH_SYS): AUTH_BADCRED.
    let gss = opaque_auth {
        flavor: 6,
        body: vec![0; 8],
    };
    assert_eq!(
        ask(&call(9, 2, 400_200, 1, 0, gss, &[])),
        Some(hexs(&hex("00000009 00000001 00000001 00000001 00000001")))
    );
    // A malformed AUTH_SYS body: AUTH_BADCRED too.
    let bad = opaque_auth {
        flavor: 1,
        body: vec![0; 3],
    };
    assert_eq!(
        ask(&call(10, 2, 400_200, 1, 0, bad, &[])),
        Some(hexs(&hex("0000000a 00000001 00000001 00000001 00000001")))
    );
    // No such program, version, procedure; bad arguments; a failure.
    let refused = |xid: u32, stat: &str| {
        Some(hexs(&hex(&format!(
            "{xid:08x} 00000001 00000000 00000000 00000000 {stat}"
        ))))
    };
    assert_eq!(
        ask(&call(11, 2, 999, 1, 0, none(), &[])),
        refused(11, "00000001")
    );
    assert_eq!(
        ask(&call(12, 2, 400_200, 2, 0, none(), &[])),
        refused(12, "00000002 00000001 00000003")
    );
    assert_eq!(
        ask(&call(13, 2, 400_200, 1, 9, none(), &[])),
        refused(13, "00000003")
    );
    assert_eq!(
        ask(&call(14, 2, 400_200, 1, 1, none(), &hex("000000ff"))),
        refused(14, "00000004")
    );
    assert_eq!(
        ask(&call(15, 2, 400_200, 1, 2, none(), &[])),
        refused(15, "00000005")
    );
    // Too short, a reply, nothing: no answer at all.
    assert_eq!(ask(&hex("0000")), None);
    assert_eq!(ask(&hex("00000001 00000001 00000000")), None);
    assert_eq!(ask(&[]), None);
}

#[test]
fn auth_sys_reaches_the_program() {
    let (d, _) = dispatcher();
    let cred = Credential::sys(b"client", 501, 20, &[20, 12])
        .to_opaque()
        .unwrap();
    let r = d
        .handle(
            &call(1, 2, 400_200, 3, 1, cred, &opaque(b"x")),
            peer(),
            Transport::Tcp,
        )
        .unwrap();
    assert_eq!(&r[r.len() - 4..], &501u32.to_be_bytes());
    // Seventeen GIDs cannot be sent.
    assert!(Credential::sys(b"c", 1, 1, &[0; 17]).to_opaque().is_err());
}

#[test]
fn the_duplicate_request_cache() {
    let (mut d, count) = dispatcher();
    d.set_cache(Arc::new(DuplicateCache::new(
        Duration::from_secs(60),
        16,
        |p, _, n| p == 400_200 && n == 1,
    )));
    let m = call(21, 2, 400_200, 1, 1, auth::none(), &opaque(b"once"));
    let first = d.handle(&m, peer(), Transport::Udp).unwrap();
    let again = d.handle(&m, peer(), Transport::Udp).unwrap();
    assert_eq!(first, again);
    assert_eq!(count.0.load(Ordering::SeqCst), 1);
    // Another port is another client.
    let other: SocketAddr = "10.0.2.2:1002".parse().unwrap();
    d.handle(&m, other, Transport::Udp).unwrap();
    assert_eq!(count.0.load(Ordering::SeqCst), 2);
    // A procedure not kept runs each time.
    let n = call(22, 2, 400_200, 1, 0, auth::none(), &[]);
    d.handle(&n, peer(), Transport::Udp);
    d.handle(&n, peer(), Transport::Udp);
    assert_eq!(count.0.load(Ordering::SeqCst), 4);
}

fn tcp_server(d: Dispatcher) -> SocketAddr {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let a = l.local_addr().unwrap();
    let d = Arc::new(d);
    thread::spawn(move || {
        for s in l.incoming().flatten() {
            let d = d.clone();
            thread::spawn(move || {
                let _ = serve_tcp(&s, &d, MAX_RECORD);
            });
        }
    });
    a
}

fn udp_server(d: Dispatcher, drop_first: bool) -> (SocketAddr, Arc<Mutex<Vec<u32>>>) {
    let s = UdpSocket::bind("127.0.0.1:0").unwrap();
    let a = s.local_addr().unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen2 = seen.clone();
    thread::spawn(move || {
        let mut first = drop_first;
        let mut buf = [0u8; 2048];
        loop {
            // Look at the datagram without taking it, to drop the first.
            let Ok((n, _)) = s.peek_from(&mut buf) else {
                return;
            };
            if let Some(x) = buf.get(..4.min(n)).filter(|b| b.len() == 4) {
                seen2
                    .lock()
                    .unwrap()
                    .push(u32::from_be_bytes([x[0], x[1], x[2], x[3]]));
            }
            if first {
                first = false;
                let _ = s.recv_from(&mut buf);
                continue;
            }
            if serve_datagram(&s, &d).is_err() {
                return;
            }
        }
    });
    (a, seen)
}

fn cfg(timeout_ms: u64) -> ClientConfig {
    ClientConfig {
        timeout: Duration::from_millis(timeout_ms),
        ..ClientConfig::default()
    }
}

fn echo(c: &mut RpcClient, what: &[u8]) -> Result<Vec<u8>, RpcError> {
    c.call_raw(
        400_200,
        1,
        1,
        &Credential::sys(b"t", 7, 7, &[]),
        &opaque(what),
    )
}

#[test]
fn tcp_calls_and_reconnection() {
    let (d, _) = dispatcher();
    let a = tcp_server(d);
    let mut c = RpcClient::new(a, Proto::Tcp, cfg(2000));
    for i in 0..20u8 {
        let r = echo(&mut c, &[i; 5]).unwrap();
        assert_eq!(r[..12], opaque(&[i; 5])[..]);
    }
    assert!(matches!(
        c.call_raw(400_200, 1, 9, &Credential::None, &[]),
        Err(RpcError::Refused(AcceptError::ProcUnavail))
    ));
    assert_eq!(
        c.call_raw(400_200, 2, 0, &Credential::None, &[])
            .unwrap_err()
            .mismatch(),
        Some((1, 3))
    );
    // A connection the client closes is opened again on the next call.
    c.disconnect();
    assert!(echo(&mut c, b"again").is_ok());
}

#[test]
fn a_broken_kept_connection_is_retried_once() {
    // A server that answers one call per connection, then closes it.
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let a = l.local_addr().unwrap();
    let (d, count) = dispatcher();
    thread::spawn(move || {
        for mut s in l.incoming().flatten() {
            if let Ok(Some(m)) = read_record(&mut s, MAX_RECORD)
                && let Some(r) = d.handle(&m, peer(), Transport::Tcp)
            {
                let _ = s.write_all(&frame(&r));
            }
        }
    });
    let mut c = RpcClient::new(a, Proto::Tcp, cfg(2000));
    assert!(echo(&mut c, b"one").is_ok());
    // The kept connection is closed: the call goes out again on a new one.
    thread::sleep(Duration::from_millis(50));
    assert!(echo(&mut c, b"two").is_ok());
    assert_eq!(count.0.load(Ordering::SeqCst), 2);
}

#[test]
fn tcp_errors() {
    // A server that reads a call and closes without a reply.
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let a = l.local_addr().unwrap();
    thread::spawn(move || {
        for mut s in l.incoming().flatten() {
            let _ = read_record(&mut s, MAX_RECORD);
            // The second connection gets a record header of 2 GiB.
            let _ = s.write_all(&[0xff; 4]);
        }
    });
    let mut c = RpcClient::new(a, Proto::Tcp, cfg(2000));
    let e = echo(&mut c, b"x").unwrap_err();
    assert!(
        matches!(e, RpcError::TooLarge(_) | RpcError::ConnectionLost(_)),
        "{e}"
    );
    // Nobody listening.
    let gone = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let mut c = RpcClient::new(gone, Proto::Tcp, cfg(500));
    assert!(matches!(echo(&mut c, b"x"), Err(RpcError::Connect(_))));
}

#[test]
fn udp_calls_and_retransmission() {
    let (d, count) = dispatcher();
    let (a, seen) = udp_server(d, true);
    let mut c = RpcClient::new(
        a,
        Proto::Udp,
        ClientConfig {
            retransmits: 3,
            ..cfg(200)
        },
    );
    let t = Instant::now();
    let r = echo(&mut c, b"late").unwrap();
    assert_eq!(r[..8], opaque(b"late")[..]);
    assert!(t.elapsed() >= Duration::from_millis(200));
    // The first datagram was dropped; the second carried the same XID.
    let s = seen.lock().unwrap().clone();
    assert_eq!(s.len(), 2);
    assert_eq!(s[0], s[1]);
    assert_eq!(count.0.load(Ordering::SeqCst), 1);
}

#[test]
fn udp_timeout_and_interrupt() {
    // A socket that never answers.
    let silent = UdpSocket::bind("127.0.0.1:0").unwrap();
    let a = silent.local_addr().unwrap();
    let mut c = RpcClient::new(
        a,
        Proto::Udp,
        ClientConfig {
            retransmits: 2,
            max_timeout: Duration::from_millis(100),
            ..cfg(50)
        },
    );
    let t = Instant::now();
    assert!(matches!(echo(&mut c, b"x"), Err(RpcError::Timeout)));
    // 50, then 100, then 100 (the ceiling).
    assert!(t.elapsed() >= Duration::from_millis(250));
    // An interrupt ends a long wait early.
    let flag = Arc::new(AtomicBool::new(false));
    let mut c = RpcClient::new(
        a,
        Proto::Udp,
        ClientConfig {
            interrupt: Some(flag.clone()),
            ..cfg(30_000)
        },
    );
    let f = flag.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(150));
        f.store(true, Ordering::Relaxed);
    });
    let t = Instant::now();
    assert!(matches!(echo(&mut c, b"x"), Err(RpcError::Interrupted)));
    assert!(t.elapsed() < Duration::from_secs(5));
    drop(silent);
}

#[test]
fn a_denied_reply_is_typed() {
    let (d, _) = dispatcher();
    let a = tcp_server(d);
    let mut c = RpcClient::new(a, Proto::Tcp, cfg(2000));
    let (xid, m) = c.message(400_200, 1, 0, &Credential::None, &[]).unwrap();
    // Send it with a GSS credential by hand.
    let mut body = Decoder::new(&m);
    body.get_u32().unwrap();
    let bad = call(
        xid,
        2,
        400_200,
        1,
        0,
        opaque_auth {
            flavor: 6,
            body: vec![],
        },
        &[],
    );
    let r = c.attempt(xid, &bad, Duration::from_secs(2)).unwrap();
    assert!(matches!(
        nfs::rpc::client::results(&r),
        Err(RpcError::Denied(Denied::Auth(auth_stat::AUTH_BADCRED)))
    ));
}

#[test]
fn replies_out_of_order_are_matched_by_xid() {
    // A server that answers each call twice: first with a stray XID.
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let a = l.local_addr().unwrap();
    let (d, _) = dispatcher();
    thread::spawn(move || {
        for mut s in l.incoming().flatten() {
            while let Ok(Some(m)) = read_record(&mut s, MAX_RECORD) {
                if let Some(mut r) = d.handle(&m, peer(), Transport::Tcp) {
                    let real = r.clone();
                    r[0] ^= 0xff;
                    let _ = s.write_all(&frame(&r));
                    let _ = s.write_all(&frame(&real));
                }
            }
        }
    });
    let mut c = RpcClient::new(a, Proto::Tcp, cfg(2000));
    for i in 0..5u8 {
        assert_eq!(echo(&mut c, &[i]).unwrap()[..8], opaque(&[i])[..]);
    }
    let mut buf = [0u8; 1];
    let _ = (&[0u8][..]).read(&mut buf);
}
