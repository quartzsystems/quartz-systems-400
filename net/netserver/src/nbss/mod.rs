// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `NetBIOS` session service on TCP 139 (RFC 1002 4.3): `QZLSSERVER`
//! answers the session request with [`answer`] before it hands the socket
//! over; after it the stream is framed as on 445 (a type byte of 0 and a
//! 24-bit length), and the library skips the keepalives clients send.
//! The name service on UDP 137 is [`crate::nbns`]; the datagram service
//! on 138 is not built (`docs/NETSERVER.md` section 10).

pub mod name;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

pub use name::{NbName, SUFFIX_SERVER, SUFFIX_WORKSTATION};

/// A session message (an SMB message follows).
pub const SESSION_MESSAGE: u8 = 0x00;
/// A session request.
pub const SESSION_REQUEST: u8 = 0x81;
/// The positive response to a session request.
pub const POSITIVE_RESPONSE: u8 = 0x82;
/// The negative response to a session request.
pub const NEGATIVE_RESPONSE: u8 = 0x83;
/// A keepalive.
pub const KEEP_ALIVE: u8 = 0x85;

/// Negative response: not listening on the called name.
pub const NEG_NOT_LISTENING_ON_CALLED: u8 = 0x80;
/// Negative response: called name not present.
pub const NEG_CALLED_NOT_PRESENT: u8 = 0x82;
/// Negative response: unspecified error.
pub const NEG_UNSPECIFIED: u8 = 0x8f;

/// The longest session request body read (two names with scopes).
pub const MAX_REQUEST: usize = 600;

/// Why a session was not set up.
#[derive(Debug, thiserror::Error)]
pub enum NbssError {
    /// A `NetBIOS` name is malformed.
    #[error("malformed NetBIOS name")]
    BadName,
    /// A frame is malformed or of the wrong type.
    #[error("malformed session frame")]
    BadFrame,
    /// A frame announces more than this service reads.
    #[error("frame too long")]
    TooLong,
    /// The client asked for a name this server does not have.
    #[error("called name {0} not served")]
    NotCalled(String),
    /// The client did not send its request in time.
    #[error("no session request in time")]
    Timeout,
    /// The socket failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// One frame taken off the stream before the session starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    /// An SMB message (a client that skipped the session request).
    Message(Vec<u8>),
    /// A session request with the called and calling names.
    SessionRequest {
        /// The name the client asked for.
        called: NbName,
        /// The client's name.
        calling: NbName,
    },
    /// A keepalive.
    KeepAlive,
}

/// Takes one complete frame off the front of `buf`, or `None` when more
/// bytes are needed. `max` bounds the length a client may announce.
///
/// # Errors
///
/// A frame of another type, longer than `max`, or with bad names.
pub fn take_frame(buf: &mut Vec<u8>, max: usize) -> Result<Option<Frame>, NbssError> {
    if buf.len() < 4 {
        return Ok(None);
    }
    let kind = buf[0];
    let len = (usize::from(buf[1]) << 16) | (usize::from(buf[2]) << 8) | usize::from(buf[3]);
    if !matches!(kind, SESSION_MESSAGE | SESSION_REQUEST | KEEP_ALIVE) {
        return Err(NbssError::BadFrame);
    }
    if len > max {
        return Err(NbssError::TooLong);
    }
    if buf.len() < 4 + len {
        return Ok(None);
    }
    let body: Vec<u8> = buf.drain(..4 + len).skip(4).collect();
    Ok(Some(match kind {
        SESSION_MESSAGE => Frame::Message(body),
        KEEP_ALIVE => Frame::KeepAlive,
        _ => {
            let (called, n) = NbName::decode(&body)?;
            let (calling, _) = NbName::decode(&body[n..])?;
            Frame::SessionRequest { called, calling }
        }
    }))
}

/// The positive session response.
#[must_use]
pub fn positive_response() -> [u8; 4] {
    [POSITIVE_RESPONSE, 0, 0, 0]
}

/// A negative session response with `code`.
#[must_use]
pub fn negative_response(code: u8) -> [u8; 5] {
    [NEGATIVE_RESPONSE, 0, 0, 1, code]
}

/// A session request frame (for tests and clients).
#[must_use]
pub fn session_request(called: &NbName, calling: &NbName) -> Vec<u8> {
    let mut body = called.encode();
    body.extend_from_slice(&calling.encode());
    let n = u32::try_from(body.len()).expect("small");
    let mut out = vec![SESSION_REQUEST];
    out.extend_from_slice(&n.to_be_bytes()[1..]);
    out.extend_from_slice(&body);
    out
}

/// Whether a session request for `called` is for this server, `server`:
/// its name or `*SMBSERVER`, with the server suffix.
#[must_use]
pub fn is_for(called: &NbName, server: &str) -> bool {
    called.suffix == SUFFIX_SERVER
        && (called.is(server, SUFFIX_SERVER) || called.is("*SMBSERVER", SUFFIX_SERVER))
}

/// Reads the session request on a socket accepted on 139 and answers it:
/// positive when it calls `server`, else negative (and an error). Waits
/// at most `timeout` in all; keepalives before the request are skipped.
/// Reads only the request's bytes, so what follows is the first SMB
/// message for whoever serves the socket next. Returns the client's name.
///
/// # Errors
///
/// A refused, malformed or missing request, or a socket error.
pub fn answer(
    stream: &mut TcpStream,
    server: &str,
    timeout: Duration,
) -> Result<NbName, NbssError> {
    let deadline = Instant::now() + timeout;
    let old = stream.read_timeout()?;
    let r = answer_until(stream, server, deadline);
    stream.set_read_timeout(old)?;
    r
}

fn read_exact_by(
    stream: &mut TcpStream,
    buf: &mut [u8],
    deadline: Instant,
) -> Result<(), NbssError> {
    let mut got = 0;
    while got < buf.len() {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(NbssError::Timeout);
        }
        stream.set_read_timeout(Some(left))?;
        match stream.read(&mut buf[got..]) {
            Ok(0) => return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof).into()),
            Ok(n) => got += n,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return Err(NbssError::Timeout);
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

fn answer_until(
    stream: &mut TcpStream,
    server: &str,
    deadline: Instant,
) -> Result<NbName, NbssError> {
    loop {
        let mut head = [0u8; 4];
        read_exact_by(stream, &mut head, deadline)?;
        let len = (usize::from(head[1]) << 16) | (usize::from(head[2]) << 8) | usize::from(head[3]);
        match head[0] {
            KEEP_ALIVE if len == 0 => {}
            SESSION_REQUEST if len <= MAX_REQUEST => {
                let mut buf = head.to_vec();
                buf.resize(4 + len, 0);
                read_exact_by(stream, &mut buf[4..], deadline)?;
                let Some(Frame::SessionRequest { called, calling }) =
                    take_frame(&mut buf, MAX_REQUEST)?
                else {
                    return Err(NbssError::BadFrame);
                };
                if is_for(&called, server) {
                    stream.write_all(&positive_response())?;
                    return Ok(calling);
                }
                stream.write_all(&negative_response(NEG_CALLED_NOT_PRESENT))?;
                return Err(NbssError::NotCalled(called.name));
            }
            SESSION_REQUEST => {
                let _ = stream.write_all(&negative_response(NEG_UNSPECIFIED));
                return Err(NbssError::TooLong);
            }
            _ => {
                let _ = stream.write_all(&negative_response(NEG_UNSPECIFIED));
                return Err(NbssError::BadFrame);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn names_round_trip() {
        let mut n = NbName::new("qs400sys", SUFFIX_SERVER);
        n.scope = "corp.example".into();
        let enc = n.encode();
        assert_eq!(enc[0], 32);
        let (d, used) = NbName::decode(&enc).unwrap();
        assert_eq!((d, used), (n, enc.len()));
        assert!(NbName::decode(&[32, b'Z']).is_err());
    }

    #[test]
    fn frames() {
        let req = session_request(&NbName::new("SRV", SUFFIX_SERVER), &NbName::new("PC", 0));
        let mut buf = vec![KEEP_ALIVE, 0, 0, 0];
        buf.extend_from_slice(&req);
        assert_eq!(
            take_frame(&mut buf, MAX_REQUEST).unwrap(),
            Some(Frame::KeepAlive)
        );
        let Some(Frame::SessionRequest { called, .. }) = take_frame(&mut buf, MAX_REQUEST).unwrap()
        else {
            panic!()
        };
        assert!(is_for(&called, "srv"));
        assert!(buf.is_empty());
        assert!(take_frame(&mut vec![0x99, 0, 0, 0], 10).is_err());
        assert!(take_frame(&mut vec![0, 0, 1, 0], 10).is_err());
        assert_eq!(take_frame(&mut vec![0, 0, 0], 10).unwrap(), None);
    }

    fn pair() -> (TcpStream, TcpStream) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let c = TcpStream::connect(l.local_addr().unwrap()).unwrap();
        (l.accept().unwrap().0, c)
    }

    #[test]
    fn answers_a_request_and_leaves_what_follows() {
        let (mut s, mut c) = pair();
        let mut out = vec![KEEP_ALIVE, 0, 0, 0];
        out.extend_from_slice(&session_request(
            &NbName::new("*SMBSERVER", SUFFIX_SERVER),
            &NbName::new("PC1", 0),
        ));
        out.extend_from_slice(&[0, 0, 0, 1, 0xAB]);
        c.write_all(&out).unwrap();
        let calling = answer(&mut s, "QS400", Duration::from_secs(2)).unwrap();
        assert_eq!(calling.name, "PC1");
        let mut resp = [0u8; 4];
        c.read_exact(&mut resp).unwrap();
        assert_eq!(resp, positive_response());
        let mut next = [0u8; 5];
        s.read_exact(&mut next).unwrap();
        assert_eq!(next, [0, 0, 0, 1, 0xAB]);
    }

    #[test]
    fn refuses_another_name_and_times_out() {
        let (mut s, mut c) = pair();
        c.write_all(&session_request(
            &NbName::new("OTHER", SUFFIX_SERVER),
            &NbName::new("PC1", 0),
        ))
        .unwrap();
        assert!(matches!(
            answer(&mut s, "QS400", Duration::from_secs(2)),
            Err(NbssError::NotCalled(_))
        ));
        let mut resp = [0u8; 5];
        c.read_exact(&mut resp).unwrap();
        assert_eq!(resp, negative_response(NEG_CALLED_NOT_PRESENT));
        let (mut s, _c) = pair();
        assert!(matches!(
            answer(&mut s, "QS400", Duration::from_millis(100)),
            Err(NbssError::Timeout)
        ));
    }
}
