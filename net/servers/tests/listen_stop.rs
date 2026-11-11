// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A stopped accept loop closes its port: a server ended with
//! `ENDTCPSVR` (`QAPD` stops its loop before it ends its connections)
//! refuses new connections at connect, and a connection served before
//! the stop runs on.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;

use servers::listen::{self, Conn, Service};

/// Echoes until the peer closes.
struct Echo;

impl Service for Echo {
    fn serve(&self, mut conn: Conn, _peer: SocketAddr) {
        let mut b = [0u8; 16];
        while let Ok(n @ 1..) = conn.read(&mut b) {
            if conn.write_all(&b[..n]).is_err() {
                return;
            }
        }
    }
}

fn echo(s: &mut TcpStream, what: [u8; 2]) {
    s.write_all(&what).unwrap();
    let mut b = [0u8; 2];
    s.read_exact(&mut b).unwrap();
    assert_eq!(b, what);
}

#[test]
fn a_stopped_loop_refuses_connects() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let running = listen::serve(l, None, Arc::new(Echo)).unwrap();
    let addr = running.addr();
    let mut before = TcpStream::connect(addr).unwrap();
    echo(&mut before, *b"hi");
    running.stop();
    assert!(
        TcpStream::connect(addr).is_err(),
        "connected after the stop"
    );
    // The connection served before the stop runs on.
    echo(&mut before, *b"ok");
}

/// Accept blocks instead of polling: connection after connection, each
/// right after the last, is answered without a poll interval's wait.
#[test]
fn a_new_connection_is_taken_at_once() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let running = listen::serve(l, None, Arc::new(Echo)).unwrap();
    let addr = running.addr();
    let rounds = 20u32;
    let t0 = std::time::Instant::now();
    for _ in 0..rounds {
        let mut s = TcpStream::connect(addr).unwrap();
        echo(&mut s, *b"go");
    }
    let each = t0.elapsed() / rounds;
    running.stop();
    assert!(
        each < std::time::Duration::from_millis(20),
        "{each:?} a connection"
    );
}
