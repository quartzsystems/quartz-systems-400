// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A TFTP read (RFC 1350, octet mode, 512-byte blocks): what a PXE
//! boot's server (QEMU's built-in one among them) offers beside the boot
//! file. Each block is acknowledged; a lost one is asked for again by
//! resending the last packet, five times at most.

use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::Duration;

use super::MAX_BYTES;

const RRQ: u16 = 1;
const DATA: u16 = 3;
const ACK: u16 = 4;
const ERROR: u16 = 5;
const BLOCK: usize = 512;

/// The read request for `file`.
pub fn request(file: &str) -> Vec<u8> {
    let mut p = RRQ.to_be_bytes().to_vec();
    p.extend(file.as_bytes());
    p.push(0);
    p.extend(b"octet");
    p.push(0);
    p
}

/// The file `path` (leading `/` dropped) from the server at `host:port`.
pub fn get(host: &str, port: u16, path: &str, wait: Duration) -> Result<Vec<u8>, String> {
    let server: SocketAddr = (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("{host}: {e}"))?
        .next()
        .ok_or_else(|| format!("{host}: no address"))?;
    let bind = if server.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" };
    let s = UdpSocket::bind(bind).map_err(|e| e.to_string())?;
    s.set_read_timeout(Some(wait)).map_err(|e| e.to_string())?;
    let file = path.trim_start_matches('/');
    let mut last = request(file);
    s.send_to(&last, server).map_err(|e| e.to_string())?;
    let mut peer: Option<SocketAddr> = None;
    let mut out = Vec::new();
    let mut expect: u16 = 1;
    let mut tries = 0;
    let mut buf = [0u8; 4 + BLOCK];
    loop {
        let (n, from) = match s.recv_from(&mut buf) {
            Ok(x) => x,
            Err(_) if tries < 5 => {
                tries += 1;
                s.send_to(&last, peer.unwrap_or(server)).map_err(|e| e.to_string())?;
                continue;
            }
            Err(e) => return Err(format!("tftp://{host}/{file}: {e}")),
        };
        // The server answers from a port of its own (its transfer ID).
        if from.ip() != server.ip() || peer.is_some_and(|p| p != from) || n < 4 {
            continue;
        }
        peer = Some(from);
        let op = u16::from_be_bytes([buf[0], buf[1]]);
        let num = u16::from_be_bytes([buf[2], buf[3]]);
        match op {
            DATA if num == expect => {
                out.extend_from_slice(&buf[4..n]);
                if out.len() > MAX_BYTES {
                    return Err("the response file is larger than 1 MiB".into());
                }
                last = [ACK.to_be_bytes(), num.to_be_bytes()].concat();
                s.send_to(&last, from).map_err(|e| e.to_string())?;
                tries = 0;
                if n - 4 < BLOCK {
                    return Ok(out);
                }
                expect = expect.wrapping_add(1);
            }
            DATA => {
                // A block again: its ACK was lost.
                let ack = [ACK.to_be_bytes(), num.to_be_bytes()].concat();
                s.send_to(&ack, from).map_err(|e| e.to_string())?;
            }
            ERROR => {
                let text = String::from_utf8_lossy(&buf[4..n]).trim_end_matches('\0').to_string();
                return Err(format!("tftp://{host}/{file}: {text} (error {num})"));
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-file server on loopback, from a port of its own as RFC 1350
    /// says; it drops the first ACK of block 1 once.
    fn serve(content: Vec<u8>) -> u16 {
        let listen = UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = listen.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let mut buf = [0u8; 600];
            let (n, client) = listen.recv_from(&mut buf).unwrap();
            assert_eq!(&buf[..n], &request("qs400/INSTALL.RSP")[..]);
            let t = UdpSocket::bind("127.0.0.1:0").unwrap();
            t.set_read_timeout(Some(Duration::from_millis(300))).unwrap();
            let mut dropped = false;
            for (i, chunk) in content.chunks(BLOCK).chain(std::iter::once(&[][..])).enumerate() {
                if i > 0 && chunk.is_empty() && !content.len().is_multiple_of(BLOCK) {
                    break;
                }
                let block = u16::try_from(i + 1).unwrap();
                let pkt = [&DATA.to_be_bytes()[..], &block.to_be_bytes(), chunk].concat();
                loop {
                    t.send_to(&pkt, client).unwrap();
                    match t.recv_from(&mut buf) {
                        Ok((4, _)) if !dropped && block == 1 => dropped = true,
                        Ok((4, _)) if buf[2..4] == block.to_be_bytes() => break,
                        _ => {}
                    }
                }
            }
        });
        port
    }

    #[test]
    fn reads_a_file_of_several_blocks() {
        let content: Vec<u8> = (0..1300u32).map(|i| u8::try_from(i % 251).unwrap()).collect();
        let port = serve(content.clone());
        let got = get("127.0.0.1", port, "/qs400/INSTALL.RSP", Duration::from_millis(200)).unwrap();
        assert_eq!(got, content);
    }

    #[test]
    fn an_exact_multiple_ends_with_an_empty_block() {
        let content = vec![b'x'; 1024];
        let port = serve(content.clone());
        assert_eq!(get("127.0.0.1", port, "qs400/INSTALL.RSP", Duration::from_millis(200)).unwrap(), content);
    }
}
