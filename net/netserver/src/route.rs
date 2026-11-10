// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Routing for `QZLSSERVER`: which `QZLSFILE` job a new connection goes
//! to. A client reconnecting to take back its durable opens sends the
//! same `ClientGuid` in its first NEGOTIATE; [`client_guid`] reads it
//! without taking a byte off the socket (the job that serves the
//! connection reads the NEGOTIATE itself), so the listener can hand the
//! socket to the job that holds that client's opens
//! (`docs/NETSERVER.md` section 3).

use std::net::TcpStream;
use std::time::{Duration, Instant};

/// The most bytes looked at.
pub const MAX_PEEK: usize = 64 * 1024;

/// The `ClientGuid` of the SMB2 NEGOTIATE at the start of `stream`, when it
/// offers SMB 2.1 or later and the GUID is not nil. `None` for an SMB1
/// negotiate (the multi-protocol upgrade), a 2.0.2-only negotiate (which
/// carries no GUID that matters), anything else, a message longer than
/// [`MAX_PEEK`], or nothing complete within `wait`. Never consumes
/// bytes; the socket's read timeout is put back.
#[must_use]
pub fn client_guid(stream: &TcpStream, wait: Duration) -> Option<[u8; 16]> {
    let old = stream.read_timeout().ok()?;
    let r = peek_until(stream, Instant::now() + wait);
    let _ = stream.set_read_timeout(old);
    r
}

/// [`client_guid`] over bytes already read: the GUID when `b` starts
/// with a complete NEGOTIATE that has one.
#[must_use]
pub fn guid_in(b: &[u8]) -> Option<[u8; 16]> {
    match parse(b) {
        Parse::Done(g) => g,
        Parse::More => None,
    }
}

fn peek_until(stream: &TcpStream, deadline: Instant) -> Option<[u8; 16]> {
    let mut buf = vec![0u8; MAX_PEEK];
    let mut seen = 0;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return None;
        }
        stream.set_read_timeout(Some(left)).ok()?;
        let n = stream.peek(&mut buf).ok()?;
        if n == 0 {
            return None;
        }
        match parse(&buf[..n]) {
            Parse::Done(r) => return r,
            Parse::More if n == MAX_PEEK => return None,
            Parse::More => {
                // peek answers with what has arrived; wait for more.
                if n == seen {
                    std::thread::sleep(Duration::from_millis(5));
                }
                seen = n;
            }
        }
    }
}

/// What the bytes so far say.
#[derive(Debug, PartialEq, Eq)]
enum Parse {
    More,
    Done(Option<[u8; 16]>),
}

/// The NEGOTIATE's fixed layout ([MS-SMB2] 2.2.3): a 64-byte header,
/// then `StructureSize` 36, `DialectCount`, `SecurityMode`, Reserved,
/// Capabilities, `ClientGuid` (at 12 into the body), then the dialects at
/// 36 into the body.
fn parse(b: &[u8]) -> Parse {
    if b.len() < 4 {
        return Parse::More;
    }
    if b[0] != 0 {
        return Parse::Done(None);
    }
    let len = (usize::from(b[1]) << 16) | (usize::from(b[2]) << 8) | usize::from(b[3]);
    if len + 4 > MAX_PEEK {
        return Parse::Done(None);
    }
    if b.len() < 4 + len {
        // Enough of the start to give up early on what is no SMB2 NEGOTIATE.
        if b.len() >= 8 && &b[4..8] != b"\xfeSMB" {
            return Parse::Done(None);
        }
        return Parse::More;
    }
    Parse::Done(guid(&b[4..4 + len]))
}

fn guid(m: &[u8]) -> Option<[u8; 16]> {
    if m.len() < 64 + 36 || &m[..4] != b"\xfeSMB" || u16::from_le_bytes([m[12], m[13]]) != 0 {
        return None;
    }
    let body = &m[64..];
    if u16::from_le_bytes([body[0], body[1]]) != 36 {
        return None;
    }
    let count = usize::from(u16::from_le_bytes([body[2], body[3]]));
    let dialects = body.get(36..36 + 2 * count)?;
    let best = dialects
        .as_chunks::<2>()
        .0
        .iter()
        .map(|d| u16::from_le_bytes(*d))
        .filter(|d| *d != 0x02FF)
        .max()?;
    let g: [u8; 16] = body[12..28].try_into().ok()?;
    (best >= 0x0210 && g != [0; 16]).then_some(g)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn negotiate(dialects: &[u16], g: [u8; 16]) -> Vec<u8> {
        let mut m = vec![0u8; 64];
        m[..4].copy_from_slice(b"\xfeSMB");
        m[4] = 64;
        let mut body = vec![0u8; 36];
        body[0] = 36;
        body[2..4].copy_from_slice(&u16::try_from(dialects.len()).unwrap().to_le_bytes());
        body[12..28].copy_from_slice(&g);
        for d in dialects {
            body.extend_from_slice(&d.to_le_bytes());
        }
        m.extend_from_slice(&body);
        let mut f = vec![0, 0, 0, 0];
        f[1..].copy_from_slice(&u32::try_from(m.len()).unwrap().to_be_bytes()[1..]);
        f.extend_from_slice(&m);
        f
    }

    fn pair() -> (TcpStream, TcpStream) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let c = TcpStream::connect(l.local_addr().unwrap()).unwrap();
        (l.accept().unwrap().0, c)
    }

    #[test]
    fn reads_the_guid_and_leaves_every_byte() {
        let (mut s, mut c) = pair();
        let f = negotiate(&[0x0202, 0x0210, 0x0311], [7; 16]);
        c.write_all(&f[..10]).unwrap();
        let writer = std::thread::spawn({
            let rest = f[10..].to_vec();
            move || {
                std::thread::sleep(Duration::from_millis(50));
                c.write_all(&rest).unwrap();
                c
            }
        });
        assert_eq!(client_guid(&s, Duration::from_secs(2)), Some([7; 16]));
        let _c = writer.join().unwrap();
        let mut all = vec![0u8; f.len()];
        s.read_exact(&mut all).unwrap();
        assert_eq!(all, f);
    }

    #[test]
    fn no_guid_for_old_dialects_smb1_or_silence() {
        let (s, mut c) = pair();
        c.write_all(&negotiate(&[0x0202], [7; 16])).unwrap();
        assert_eq!(client_guid(&s, Duration::from_secs(2)), None);
        let (s, mut c) = pair();
        c.write_all(&negotiate(&[0x0210], [0; 16])).unwrap();
        assert_eq!(client_guid(&s, Duration::from_secs(2)), None);
        let (s, mut c) = pair();
        let mut smb1 = vec![0, 0, 0, 40, 0xFF, b'S', b'M', b'B', 0x72];
        smb1.resize(44, 0);
        c.write_all(&smb1).unwrap();
        assert_eq!(client_guid(&s, Duration::from_secs(2)), None);
        let (s, _c) = pair();
        let t = Instant::now();
        assert_eq!(client_guid(&s, Duration::from_millis(100)), None);
        assert!(t.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn a_frame_too_long_is_not_waited_for() {
        assert_eq!(parse(&[0, 0x10, 0, 0]), Parse::Done(None));
        assert_eq!(parse(&[0, 0, 1, 0, 0xfe, b'S', b'M', b'B']), Parse::More);
        assert_eq!(
            parse(&[0, 0, 1, 0, 0xff, b'S', b'M', b'B']),
            Parse::Done(None)
        );
    }
}
