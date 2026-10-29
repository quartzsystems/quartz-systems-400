// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! IR programs calling the socket procedures as an RPG HTTP client
//! does: `socket`, `htons` and `inet_addr` into a `sockaddr_in`,
//! `connect`, `send`, a `recv` loop, `close`, and `errno` through
//! `__errno()`; a UDP exchange with `sendto` and `recvfrom`, whose
//! pointer arguments travel on the stack. Linux only: the sockets are
//! the kernel's.

#![cfg(target_os = "linux")]

use std::cell::RefCell;
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::{TcpListener, UdpSocket};
use std::rc::Rc;

use mi::exec::Program;
use sysapi::MemProvider;

fn hex(b: &[u8]) -> String {
    b.iter().fold(String::new(), |mut s, c| {
        let _ = write!(s, "{c:02X}");
        s
    })
}

fn run(text: &str) -> Program {
    let mut p = Program::from_text(text).unwrap_or_else(|e| panic!("{e}"));
    sysapi::install(p.job(), Rc::new(RefCell::new(MemProvider::default())));
    p.call_entry().unwrap_or_else(|e| panic!("{e}"));
    p
}

fn bin4(p: &Program, name: &str) -> i32 {
    let b = p.static_bytes(name).unwrap();
    i32::from_be_bytes(b[..4].try_into().unwrap())
}

const PROTOS: &str = "
extern proc socket(byval $d: i32, byval $t: i32, byval $p: i32) -> i32
extern proc connect(byval $sd: i32, byval $a: ptr, byval $l: i32) -> i32
extern proc send(byval $sd: i32, byval $b: ptr, byval $l: i32, byval $f: i32) -> i32
extern proc recv(byval $sd: i32, byval $b: ptr, byval $l: i32, byval $f: i32) -> i32
extern proc sendto(byval $sd: i32, byval $b: ptr, byval $l: i32, byval $f: i32, byval $to: ptr, byval $tl: i32) -> i32
extern proc recvfrom(byval $sd: i32, byval $b: ptr, byval $l: i32, byval $f: i32, byval $fr: ptr, byval $fl: ptr) -> i32
extern proc close(byval $sd: i32) -> i32
extern proc htons(byval $v: u16) -> u16
extern proc inet_addr(byval $cp: ptr) -> u32
extern proc __errno() -> ptr
type Sin = struct(16) { fam: u16 @0, port: u16 @2, addr: u32 @4 }
static $host: bytes(10) inz(x\"F1F2F74BF04BF04BF100\")
static $sin: Sin
";

fn client(port: u16, request: &[u8]) -> String {
    format!(
        "
module HTTPC
{PROTOS}
static $req: bytes({len}) inz(x\"{req}\")
static $resp: bytes(512)
static $got: i32 inz(0)
static $rc: i32 inz(99)
static $err: i32 inz(0)
static $closed: i32 inz(99)
proc HTTPC() entry {{
L0:
  %sd = callb socket(2, 1, 0)
  store $sin.fam, 2
  %port = callb htons({port})
  store $sin.port, %port
  %h = addr $host
  %a = callb inet_addr(%h)
  store $sin.addr, %a
  %sa = addr $sin
  %rc = callb connect(%sd, %sa, 16)
  store $rc, %rc
  %ok = cmp.eq %rc, 0
  brif %ok, SEND, FAIL
SEND:
  %rq = addr $req
  %n = callb send(%sd, %rq, {len}, 0)
  br READ
READ:
  %g = load $got
  %room = sub 512, %g
  %base = addr $resp
  %g64 = cvt.i64 %g
  %at = ptradd %base, %g64
  %r = callb recv(%sd, %at, %room, 0)
  %more = cmp.gt %r, 0
  brif %more, ADD, DONE
ADD:
  %g2 = add %g, %r
  store $got, %g2
  br READ
DONE:
  %c = callb close(%sd)
  store $closed, %c
  ret
FAIL:
  %ep = callb __errno()
  %e = load [%ep]:i32
  store $err, %e
  %c2 = callb close(%sd)
  store $closed, %c2
  ret
}}
",
        len = request.len(),
        req = hex(request),
    )
}

#[test]
fn an_http_client_exchange_through_the_bound_procedures() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let request = b"GET /index.html HTTP/1.0\r\nHost: localhost\r\n\r\n";
    let server = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut got = Vec::new();
        let mut buf = [0u8; 256];
        while !got.ends_with(b"\r\n\r\n") {
            let n = s.read(&mut buf).unwrap();
            assert!(n > 0, "the request ended early");
            got.extend_from_slice(&buf[..n]);
        }
        s.write_all(b"HTTP/1.0 200 OK\r\nContent-Length: 5\r\n\r\n").unwrap();
        s.write_all(b"hello").unwrap();
        got
    });
    let p = run(&client(port, request));
    let seen = server.join().unwrap();
    assert_eq!(seen, request, "the server saw the request bytes");
    assert_eq!(bin4(&p, "rc"), 0, "connect: errno {}", bin4(&p, "err"));
    let got = usize::try_from(bin4(&p, "got")).unwrap();
    let resp = p.static_bytes("resp").unwrap();
    let text = String::from_utf8_lossy(&resp[..got]);
    assert!(text.starts_with("HTTP/1.0 200 OK\r\n"), "{text}");
    assert!(text.ends_with("\r\n\r\nhello"), "{text}");
    assert_eq!(bin4(&p, "closed"), 0);
    let sin = p.static_bytes("sin").unwrap();
    assert_eq!(&sin[..8], &[0, 2, port.to_be_bytes()[0], port.to_be_bytes()[1], 127, 0, 0, 1], "network order in storage");
}

#[test]
fn a_refused_connection_sets_errno() {
    let port = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let p = run(&client(port, b"x"));
    assert_eq!(bin4(&p, "rc"), -1);
    assert_eq!(bin4(&p, "err"), 3425, "ECONNREFUSED");
    assert_eq!(bin4(&p, "closed"), 0);
}

#[test]
fn a_datagram_round_trip_with_sendto_and_recvfrom() {
    let echo = UdpSocket::bind("127.0.0.1:0").unwrap();
    let port = echo.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let mut buf = [0u8; 64];
        let (n, from) = echo.recv_from(&mut buf).unwrap();
        echo.send_to(&buf[..n], from).unwrap();
        (buf[..n].to_vec(), from.port())
    });
    let text = format!(
        "
module UDPC
{PROTOS}
static $msg: bytes(4) inz(x\"50494E47\")
static $back: bytes(64)
static $from: Sin
static $fromlen: i32 inz(16)
static $n: i32 inz(-9)
proc UDPC() entry {{
L0:
  %sd = callb socket(2, 2, 0)
  store $sin.fam, 2
  %port = callb htons({port})
  store $sin.port, %port
  %h = addr $host
  %a = callb inet_addr(%h)
  store $sin.addr, %a
  %sa = addr $sin
  %m = addr $msg
  %s = callb sendto(%sd, %m, 4, 0, %sa, 16)
  %b = addr $back
  %f = addr $from
  %fl = addr $fromlen
  %r = callb recvfrom(%sd, %b, 64, 0, %f, %fl)
  store $n, %r
  %c = callb close(%sd)
  ret
}}
"
    );
    let p = run(&text);
    let (seen, _) = server.join().unwrap();
    assert_eq!(seen, b"PING");
    assert_eq!(bin4(&p, "n"), 4);
    assert_eq!(&p.static_bytes("back").unwrap()[..4], b"PING");
    let from = p.static_bytes("from").unwrap();
    assert_eq!(&from[..8], &[0, 2, port.to_be_bytes()[0], port.to_be_bytes()[1], 127, 0, 0, 1], "the sender's address");
    assert_eq!(bin4(&p, "fromlen"), 16);
}
