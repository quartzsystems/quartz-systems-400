// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SFTP throughput: the server over loopback TCP (`serve_tcp`), the
//! crate's client signed on with a password, a file written and read
//! back on the IFS in memory. The client keeps one 32 KiB request in
//! flight, so this measures the server and the transport per round
//! trip, not a pipelining client. Ignored by default; run with
//! `cargo test --release -p sshd --test bench -- --ignored --nocapture`
//! and `QS400_SFTP_BENCH_MIB=N` for the size (default 64).

#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::many_single_char_names)]

mod common;

use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::time::Instant;

use common::{PASSWORD, world};
use sshd::client::sftp::Sftp;
use sshd::client::{Client, Options};

const SFTP: &str = "Subsystem sftp /QOpenSys/QIBM/ProdData/SC1/OpenSSH/libexec/sftp-server\n";

#[test]
#[ignore = "bench: run in release with --ignored --nocapture"]
fn sftp_put_get() {
    let mib: usize = std::env::var("QS400_SFTP_BENCH_MIB").ok().and_then(|v| v.parse().ok()).unwrap_or(64);
    let total = mib << 20;
    let data: Vec<u8> = (0..total).map(|i| (i % 251) as u8).collect();

    let w = world(SFTP);
    let l = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).unwrap();
    let addr = l.local_addr().unwrap();
    let server = w.server.clone();
    std::thread::spawn(move || {
        let (s, _) = l.accept().unwrap();
        let _ = server.serve_tcp(&s);
    });

    let s = TcpStream::connect(addr).unwrap();
    s.set_nodelay(true).unwrap();
    let t = Instant::now();
    let mut c = Client::connect(s.try_clone().unwrap(), s, &Options::default()).unwrap();
    assert!(c.auth_password("qstest", PASSWORD).unwrap());
    let mut f = Sftp::start(&mut c).unwrap();
    let setup = t.elapsed().as_secs_f64();

    let t = Instant::now();
    f.put("big.bin", &data).unwrap().unwrap();
    let put = t.elapsed().as_secs_f64();

    let t = Instant::now();
    let back = f.get("big.bin").unwrap().unwrap();
    let get = t.elapsed().as_secs_f64();
    assert!(back == data, "the file came back different");
    f.finish().unwrap();

    let rate = |s: f64| total as f64 / (1024.0 * 1024.0) / s;
    println!("sftp: {mib} MiB, loopback TCP, one 32 KiB request in flight, IFS in memory");
    println!("  key exchange + password sign-on + sftp start {setup:.3} s");
    println!("  put {put:.3} s  {:.1} MiB/s", rate(put));
    println!("  get {get:.3} s  {:.1} MiB/s", rate(get));
}
