// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The DHCP message: BOOTP's fixed part (RFC 951, RFC 2131 §2), the
//! magic cookie and the options ([`options`]).

pub mod options;

use std::net::Ipv4Addr;

pub use options::Options;

use crate::Error;

/// The server's port.
pub const SERVER_PORT: u16 = 67;
/// The client's port.
pub const CLIENT_PORT: u16 = 68;
/// The magic cookie before the options.
pub const MAGIC: [u8; 4] = [99, 130, 83, 99];
/// `op`: a request.
pub const BOOTREQUEST: u8 = 1;
/// `op`: a reply.
pub const BOOTREPLY: u8 = 2;
/// The fixed part's length.
const FIXED: usize = 236;
/// The shortest message BOOTP relays accept.
const MIN_LEN: usize = 300;

/// The DHCP message type (option 53).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(missing_docs)] // RFC 2132 §9.6's names
pub enum Kind {
    Discover = 1,
    Offer = 2,
    Request = 3,
    Decline = 4,
    Ack = 5,
    Nak = 6,
    Release = 7,
    Inform = 8,
}

impl Kind {
    /// From the option's byte.
    pub fn from_u8(b: u8) -> Option<Self> {
        Some(match b {
            1 => Self::Discover,
            2 => Self::Offer,
            3 => Self::Request,
            4 => Self::Decline,
            5 => Self::Ack,
            6 => Self::Nak,
            7 => Self::Release,
            8 => Self::Inform,
            _ => return None,
        })
    }
}

/// One message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    /// [`BOOTREQUEST`] or [`BOOTREPLY`].
    pub op: u8,
    /// The transaction ID.
    pub xid: u32,
    /// Seconds since the client began.
    pub secs: u16,
    /// The broadcast flag: answer to the broadcast address.
    pub broadcast: bool,
    /// The client's address, when it has one.
    pub ciaddr: Ipv4Addr,
    /// The address offered.
    pub yiaddr: Ipv4Addr,
    /// The next server (TFTP).
    pub siaddr: Ipv4Addr,
    /// The relay agent.
    pub giaddr: Ipv4Addr,
    /// The client's Ethernet address.
    pub chaddr: [u8; 6],
    /// The boot file name.
    pub file: String,
    /// The options.
    pub options: Options,
}

impl Message {
    /// A request from `mac` in transaction `xid`.
    pub fn request(kind: Kind, xid: u32, mac: [u8; 6]) -> Self {
        Self {
            op: BOOTREQUEST,
            xid,
            secs: 0,
            broadcast: false,
            ciaddr: Ipv4Addr::UNSPECIFIED,
            yiaddr: Ipv4Addr::UNSPECIFIED,
            siaddr: Ipv4Addr::UNSPECIFIED,
            giaddr: Ipv4Addr::UNSPECIFIED,
            chaddr: mac,
            file: String::new(),
            options: Options {
                kind: Some(kind),
                ..Options::default()
            },
        }
    }

    /// The message type.
    pub fn kind(&self) -> Option<Kind> {
        self.options.kind
    }

    /// The bytes on the wire (padded to BOOTP's 300).
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(MIN_LEN);
        b.extend([self.op, 1, 6, 0]);
        b.extend(self.xid.to_be_bytes());
        b.extend(self.secs.to_be_bytes());
        b.extend(if self.broadcast { 0x8000u16 } else { 0 }.to_be_bytes());
        for a in [self.ciaddr, self.yiaddr, self.siaddr, self.giaddr] {
            b.extend(a.octets());
        }
        let mut ch = [0u8; 16];
        ch[..6].copy_from_slice(&self.chaddr);
        b.extend(ch);
        b.extend([0u8; 64]);
        let mut file = [0u8; 128];
        let f = self.file.as_bytes();
        let n = f.len().min(127);
        file[..n].copy_from_slice(&f[..n]);
        b.extend(file);
        debug_assert_eq!(b.len(), FIXED);
        b.extend(MAGIC);
        self.options.encode(&mut b);
        if b.len() < MIN_LEN {
            b.resize(MIN_LEN, 0);
        }
        b
    }

    /// A message from its bytes.
    pub fn decode(b: &[u8]) -> Result<Self, Error> {
        if b.len() < FIXED + MAGIC.len() {
            return Err(Error::Malformed("shorter than the fixed part"));
        }
        if b[FIXED..FIXED + 4] != MAGIC {
            return Err(Error::Malformed("no magic cookie"));
        }
        if b[1] != 1 || b[2] != 6 {
            return Err(Error::Malformed("not Ethernet"));
        }
        let be32 = |i: usize| u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        let ip = |i: usize| Ipv4Addr::from(be32(i));
        let mut chaddr = [0u8; 6];
        chaddr.copy_from_slice(&b[28..34]);
        let file_raw = &b[108..236];
        let end = file_raw.iter().position(|&c| c == 0).unwrap_or(file_raw.len());
        let mut options = Options::decode(&b[FIXED + 4..])?;
        // Option 52: the file and sname fields hold more options.
        if let Some(over) = options.overload {
            if over & 1 != 0 {
                options.merge(Options::decode(&b[108..236])?);
            }
            if over & 2 != 0 {
                options.merge(Options::decode(&b[44..108])?);
            }
        }
        Ok(Self {
            op: b[0],
            xid: be32(4),
            secs: u16::from_be_bytes([b[8], b[9]]),
            broadcast: b[10] & 0x80 != 0,
            ciaddr: ip(12),
            yiaddr: ip(16),
            siaddr: ip(20),
            giaddr: ip(24),
            chaddr,
            file: if options.overload.is_some_and(|o| o & 1 != 0) {
                String::new()
            } else {
                String::from_utf8_lossy(&file_raw[..end]).into_owned()
            },
            options,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAC: [u8; 6] = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];

    #[test]
    fn round_trip() {
        let mut m = Message::request(Kind::Discover, 0x1234_5678, MAC);
        m.broadcast = true;
        m.options.params = vec![1, 3, 6, 15, 51, 54, 58, 59];
        m.options.client_id = Some(vec![1, 0x52, 0x54, 0, 0x12, 0x34, 0x56]);
        m.options.host_name = Some("qs400".into());
        let b = m.encode();
        assert_eq!(b.len(), 300);
        assert_eq!(&b[236..240], &MAGIC);
        assert_eq!(b[10], 0x80);
        assert_eq!(Message::decode(&b).unwrap(), m);
    }

    #[test]
    fn refuses_what_is_not_dhcp() {
        assert!(Message::decode(&[0; 100]).is_err());
        let mut b = Message::request(Kind::Discover, 1, MAC).encode();
        b[236] = 0;
        assert!(Message::decode(&b).is_err());
    }

    #[test]
    fn overloaded_file_field() {
        let mut m = Message::request(Kind::Offer, 9, MAC);
        m.op = BOOTREPLY;
        m.options.overload = Some(1);
        let mut b = m.encode();
        // Option 51 in the file field.
        b[108..115].copy_from_slice(&[51, 4, 0, 0, 0x0e, 0x10, 255]);
        let d = Message::decode(&b).unwrap();
        assert_eq!(d.options.lease_secs, Some(3600));
        assert!(d.file.is_empty());
    }
}
