// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! FTP throughput: the server over loopback with a `MemHost` (the IFS
//! on a `MemBackend`), a bare client sending a stream file in binary
//! (`STOR`) and reading it back (`RETR`) over passive data connections.
//! Ignored by default; run with
//! `cargo test --release -p ftp --test bench -- --ignored --nocapture`
//! and `QS400_FTP_BENCH_MIB=N` for the size (default 64).

#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::many_single_char_names)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::time::Instant;

use ftp::memhost::MemHost;
use ftp::net::tcp::TcpNet;
use ftp::session::{Config, Session};
use servers::attrs::ftp::FtpAttrs;

const CHUNK: usize = 1 << 20;

fn server() -> SocketAddr {
    let l = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).unwrap();
    let addr = l.local_addr().unwrap();
    std::thread::spawn(move || {
        let (s, _) = l.accept().unwrap();
        let host = MemHost::new(&[("BOB", "SECRET")]);
        let mut a = FtpAttrs::default();
        a.set("NAMEFMT", "*PATH").unwrap();
        a.set("CURDIR", "*HOMEDIR").unwrap();
        let mut net = TcpNet::new(s, None, false, None).unwrap();
        let mut session = Session::new(host, Config::new(a, "QS400T"));
        ftp::server::serve(&mut net, &mut session);
    });
    addr
}

struct Control {
    r: BufReader<TcpStream>,
    w: TcpStream,
}

impl Control {
    /// The final line of a reply: its code and text.
    fn reply(&mut self) -> (u16, String) {
        loop {
            let mut l = String::new();
            assert!(self.r.read_line(&mut l).unwrap() > 0, "control connection closed");
            let l = l.trim_end().to_string();
            if l.len() >= 4 && l.as_bytes()[3] == b' ' && l[..3].bytes().all(|b| b.is_ascii_digit()) {
                return (l[..3].parse().unwrap(), l);
            }
        }
    }

    fn cmd(&mut self, c: &str) -> (u16, String) {
        self.w.write_all(format!("{c}\r\n").as_bytes()).unwrap();
        self.reply()
    }

    /// `PASV`, then the data connection.
    fn pasv(&mut self) -> TcpStream {
        let (code, text) = self.cmd("PASV");
        assert_eq!(code, 227, "{text}");
        let inner = &text[text.find('(').unwrap() + 1..text.find(')').unwrap()];
        let n: Vec<u16> = inner.split(',').map(|x| x.trim().parse().unwrap()).collect();
        let ip = Ipv4Addr::new(n[0] as u8, n[1] as u8, n[2] as u8, n[3] as u8);
        TcpStream::connect((ip, n[4] * 256 + n[5])).unwrap()
    }
}

fn mib_per_s(bytes: usize, secs: f64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0) / secs
}

#[test]
#[ignore = "bench: run in release with --ignored --nocapture"]
fn ftp_binary_stor_retr() {
    let mib: usize = std::env::var("QS400_FTP_BENCH_MIB").ok().and_then(|v| v.parse().ok()).unwrap_or(64);
    let total = mib * CHUNK;
    let block: Vec<u8> = (0..CHUNK).map(|i| (i % 251) as u8).collect();

    let addr = server();
    let s = TcpStream::connect(addr).unwrap();
    s.set_nodelay(true).unwrap();
    let mut c = Control { r: BufReader::new(s.try_clone().unwrap()), w: s };
    assert_eq!(c.reply().0, 220);
    assert_eq!(c.cmd("USER BOB").0, 331);
    assert_eq!(c.cmd("PASS SECRET").0, 230);
    assert_eq!(c.cmd("TYPE I").0, 200);

    let mut d = c.pasv();
    let t = Instant::now();
    let (code, text) = c.cmd("STOR big.bin");
    assert!(code == 150 || code == 125, "{text}");
    for _ in 0..mib {
        d.write_all(&block).unwrap();
    }
    d.shutdown(Shutdown::Write).unwrap();
    drop(d);
    let (code, text) = c.reply();
    let stor = t.elapsed().as_secs_f64();
    assert_eq!(code, 226, "{text}");

    let mut d = c.pasv();
    let t = Instant::now();
    let (code, text) = c.cmd("RETR big.bin");
    assert!(code == 150 || code == 125, "{text}");
    // The server reads the whole file before it answers 150.
    let fetch = t.elapsed().as_secs_f64();
    let mut buf = vec![0u8; CHUNK];
    let mut got = 0usize;
    let mut first = Vec::new();
    loop {
        let n = d.read(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        if first.len() < 1024 {
            first.extend_from_slice(&buf[..n.min(1024 - first.len())]);
        }
        got += n;
    }
    // Close at end of data, as clients do: the server waits for it.
    drop(d);
    let (code, text) = c.reply();
    let retr = t.elapsed().as_secs_f64();
    assert_eq!(code, 226, "{text}");
    assert_eq!(got, total);
    assert_eq!(first[..], block[..first.len()]);
    let _ = c.cmd("QUIT");

    println!("ftp: {mib} MiB stream file, binary, passive, loopback, MemBackend IFS");
    println!("  STOR {stor:.3} s  {:.1} MiB/s", mib_per_s(total, stor));
    println!("  RETR {retr:.3} s  {:.1} MiB/s", mib_per_s(total, retr));
    println!("    of which reading the file before 150: {fetch:.3} s; sending: {:.3} s", retr - fetch);
}
