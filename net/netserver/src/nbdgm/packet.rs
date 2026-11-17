// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What travels on UDP 138: `NetBIOS` datagrams (RFC 1002 4.4), the SMB
//! mailslot write inside them ([MS-MAIL] 2.2: an `SMB_COM_TRANSACTION`
//! with the mailslot's name) and the two browser frames the server uses
//! ([MS-BRWS] 2.2): the `HostAnnouncement` it sends and the
//! `AnnouncementRequest` it answers.

use std::net::Ipv4Addr;

use super::NbdgmError;
use crate::nbss::NbName;

/// `DIRECT_UNIQUE DATAGRAM`.
pub const DIRECT_UNIQUE: u8 = 0x10;
/// `DIRECT_GROUP DATAGRAM`.
pub const DIRECT_GROUP: u8 = 0x11;
/// `BROADCAST DATAGRAM`.
pub const BROADCAST: u8 = 0x12;
/// The `F` flag: the first fragment. A B node's `SNT` is 0.
const FIRST: u8 = 0x02;
/// The `M` flag: more fragments follow.
const MORE: u8 = 0x01;
/// The datagram header before the names.
const HEADER: usize = 14;
/// The largest datagram (RFC 1002 4.4.1's 576 less the IP and UDP headers
/// is what a sender keeps to; more is accepted when it arrives).
pub const MAX_DATAGRAM: usize = 1472;

/// The browser's mailslot.
pub const BROWSE: &str = "\\MAILSLOT\\BROWSE";
/// `SMB_COM_TRANSACTION`.
const SMB_TRANSACTION: u8 = 0x25;
/// The SMB header.
const SMB_HEADER: usize = 32;
/// The transaction request's parameter words: 14 and 3 setup words.
const WORDS: u8 = 17;
/// Mailslot write ([MS-MAIL] 2.2.1).
const MAILSLOT_WRITE: u16 = 1;
/// A second-class (unreliable, broadcast) mailslot.
const CLASS_2: u16 = 2;

/// `HostAnnouncement` ([MS-BRWS] 2.2.1).
pub const HOST_ANNOUNCEMENT: u8 = 0x01;
/// `AnnouncementRequest` ([MS-BRWS] 2.2.2).
pub const ANNOUNCEMENT_REQUEST: u8 = 0x02;
/// The signature every announcement ends its fixed part with.
const SIGNATURE: u16 = 0xAA55;
/// The browser protocol version announced ([MS-BRWS] 2.2.1: 15.1).
const BROWSER_VERSION: (u8, u8) = (15, 1);
/// The longest comment an announcement carries, without its terminator.
pub const MAX_COMMENT: usize = 43;

/// `SV_TYPE_WORKSTATION`.
pub const SV_TYPE_WORKSTATION: u32 = 0x0000_0001;
/// `SV_TYPE_SERVER`.
pub const SV_TYPE_SERVER: u32 = 0x0000_0002;
/// `SV_TYPE_PRINTQ_SERVER`.
pub const SV_TYPE_PRINTQ_SERVER: u32 = 0x0000_0200;
/// `SV_TYPE_SERVER_NT`: a server that is not a domain controller.
pub const SV_TYPE_SERVER_NT: u32 = 0x0000_8000;

/// One `NetBIOS` datagram (a direct or broadcast one, unfragmented).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Datagram {
    /// [`DIRECT_UNIQUE`], [`DIRECT_GROUP`] or [`BROADCAST`].
    pub kind: u8,
    /// `DGM_ID`.
    pub id: u16,
    /// The sender's address and port.
    pub source_ip: Ipv4Addr,
    /// The sender's port.
    pub source_port: u16,
    /// Who sent it.
    pub source: NbName,
    /// Whom it is for.
    pub dest: NbName,
    /// What it carries.
    pub data: Vec<u8>,
}

fn u16_at(b: &[u8], at: usize) -> Result<u16, NbdgmError> {
    b.get(at..at + 2)
        .map(|s| u16::from_be_bytes([s[0], s[1]]))
        .ok_or(NbdgmError::Truncated)
}

fn le16(b: &[u8], at: usize) -> Result<u16, NbdgmError> {
    b.get(at..at + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or(NbdgmError::Truncated)
}

impl Datagram {
    /// The bytes on the wire.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let (src, dst) = (self.source.encode(), self.dest.encode());
        let len = src.len() + dst.len() + self.data.len();
        let mut out = Vec::with_capacity(HEADER + len);
        out.extend([self.kind, FIRST]);
        out.extend(self.id.to_be_bytes());
        out.extend(self.source_ip.octets());
        out.extend(self.source_port.to_be_bytes());
        out.extend(u16::try_from(len).unwrap_or(u16::MAX).to_be_bytes());
        out.extend(0u16.to_be_bytes());
        out.extend(src);
        out.extend(dst);
        out.extend(&self.data);
        out
    }

    /// Reads a direct or broadcast datagram that is not a fragment.
    ///
    /// # Errors
    ///
    /// [`NbdgmError::NotData`] for another message type (errors, queries)
    /// or a fragment; the others for malformed bytes.
    pub fn parse(b: &[u8]) -> Result<Self, NbdgmError> {
        if b.len() > MAX_DATAGRAM {
            return Err(NbdgmError::TooLong);
        }
        if b.len() < HEADER {
            return Err(NbdgmError::Truncated);
        }
        let kind = b[0];
        if !matches!(kind, DIRECT_UNIQUE | DIRECT_GROUP | BROADCAST) {
            return Err(NbdgmError::NotData);
        }
        if b[1] & MORE != 0 || b[1] & FIRST == 0 || u16_at(b, 12)? != 0 {
            return Err(NbdgmError::NotData);
        }
        let id = u16_at(b, 2)?;
        let source_ip = Ipv4Addr::new(b[4], b[5], b[6], b[7]);
        let source_port = u16_at(b, 8)?;
        let len = usize::from(u16_at(b, 10)?);
        let body = b.get(HEADER..HEADER + len).ok_or(NbdgmError::Truncated)?;
        let (source, n) = NbName::decode(body).map_err(|_| NbdgmError::BadName)?;
        let (dest, m) = NbName::decode(&body[n..]).map_err(|_| NbdgmError::BadName)?;
        Ok(Self {
            kind,
            id,
            source_ip,
            source_port,
            source,
            dest,
            data: body[n + m..].to_vec(),
        })
    }
}

/// An SMB mailslot write: the mailslot's name and the data written.
#[must_use]
pub fn mailslot_write(mailslot: &str, data: &[u8]) -> Vec<u8> {
    let name_len = mailslot.len() + 1;
    // The data follows the name directly, as Windows and Samba send it.
    let offset = SMB_HEADER + 1 + 2 * usize::from(WORDS) + 2 + name_len;
    let off = u16::try_from(offset).unwrap_or(u16::MAX);
    let count = u16::try_from(data.len()).unwrap_or(u16::MAX);
    let mut out = Vec::with_capacity(offset + data.len());
    out.extend([0xFF, b'S', b'M', b'B', SMB_TRANSACTION]);
    out.resize(SMB_HEADER, 0);
    out.push(WORDS);
    for w in [0, count, 0, 0] {
        out.extend(w.to_le_bytes());
    }
    // MaxSetupCount, Reserved1, Flags, Timeout, Reserved2.
    out.extend([0u8; 10]);
    for w in [0, off, count, off] {
        out.extend(w.to_le_bytes());
    }
    // SetupCount 3, Reserved3; the setup words.
    out.extend([3, 0]);
    for w in [MAILSLOT_WRITE, 1, CLASS_2] {
        out.extend(w.to_le_bytes());
    }
    let bytes = u16::try_from(name_len + data.len()).unwrap_or(u16::MAX);
    out.extend(bytes.to_le_bytes());
    out.extend(mailslot.as_bytes());
    out.push(0);
    out.extend(data);
    out
}

/// The mailslot a mailslot write is for, and its data.
///
/// # Errors
///
/// [`NbdgmError::NotMailslot`] for other SMB messages; the others for
/// malformed bytes.
pub fn parse_mailslot(b: &[u8]) -> Result<(String, &[u8]), NbdgmError> {
    if b.len() < SMB_HEADER + 1 {
        return Err(NbdgmError::Truncated);
    }
    if b[..4] != [0xFF, b'S', b'M', b'B'] || b[4] != SMB_TRANSACTION {
        return Err(NbdgmError::NotMailslot);
    }
    let words = usize::from(b[SMB_HEADER]);
    if words < usize::from(WORDS) {
        return Err(NbdgmError::NotMailslot);
    }
    let w = SMB_HEADER + 1;
    if le16(b, w + 28)? != MAILSLOT_WRITE {
        return Err(NbdgmError::NotMailslot);
    }
    let count = usize::from(le16(b, w + 22)?);
    let offset = usize::from(le16(b, w + 24)?);
    let name_at = w + 2 * words + 2;
    let rest = b.get(name_at..).ok_or(NbdgmError::Truncated)?;
    let end = rest
        .iter()
        .position(|c| *c == 0)
        .ok_or(NbdgmError::Truncated)?;
    let name = String::from_utf8_lossy(&rest[..end]).into_owned();
    let data = b.get(offset..offset + count).ok_or(NbdgmError::Truncated)?;
    Ok((name, data))
}

/// A `HostAnnouncement` frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Announcement {
    /// Milliseconds until the next one.
    pub periodicity: u32,
    /// The server's `NetBIOS` name.
    pub server: String,
    /// The operating system's version, major and minor.
    pub os_version: (u8, u8),
    /// The `SV_TYPE_*` bits; 0 when the server goes away.
    pub server_type: u32,
    /// The server's text.
    pub comment: String,
}

impl Announcement {
    /// The frame's bytes. The name is cut to 15 characters and upper-cased,
    /// the comment cut to [`MAX_COMMENT`] bytes of printable ASCII.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![HOST_ANNOUNCEMENT, 0];
        out.extend(self.periodicity.to_le_bytes());
        let mut name = [0u8; 16];
        for (d, s) in name
            .iter_mut()
            .zip(self.server.to_ascii_uppercase().bytes().take(15))
        {
            *d = s;
        }
        out.extend(name);
        out.extend([self.os_version.0, self.os_version.1]);
        out.extend(self.server_type.to_le_bytes());
        out.extend([BROWSER_VERSION.0, BROWSER_VERSION.1]);
        out.extend(SIGNATURE.to_le_bytes());
        out.extend(
            self.comment
                .bytes()
                .map(|c| if (0x20..0x7F).contains(&c) { c } else { b'?' })
                .take(MAX_COMMENT),
        );
        out.push(0);
        out
    }

    /// Reads a `HostAnnouncement` frame.
    ///
    /// # Errors
    ///
    /// [`NbdgmError::NotData`] for another frame; [`NbdgmError::Truncated`]
    /// for a short one.
    pub fn parse(b: &[u8]) -> Result<Self, NbdgmError> {
        if b.first() != Some(&HOST_ANNOUNCEMENT) {
            return Err(NbdgmError::NotData);
        }
        let f = b.get(..32).ok_or(NbdgmError::Truncated)?;
        let name = &f[6..22];
        let end = name.iter().position(|c| *c == 0).unwrap_or(16);
        let rest = &b[32..];
        let cend = rest.iter().position(|c| *c == 0).unwrap_or(rest.len());
        Ok(Self {
            periodicity: u32::from_le_bytes([f[2], f[3], f[4], f[5]]),
            server: String::from_utf8_lossy(&name[..end]).into_owned(),
            os_version: (f[22], f[23]),
            server_type: u32::from_le_bytes([f[24], f[25], f[26], f[27]]),
            comment: String::from_utf8_lossy(&rest[..cend]).into_owned(),
        })
    }
}

/// Whether `frame` is an `AnnouncementRequest`.
#[must_use]
pub fn is_announcement_request(frame: &[u8]) -> bool {
    frame.first() == Some(&ANNOUNCEMENT_REQUEST)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dgm(data: Vec<u8>) -> Datagram {
        Datagram {
            kind: DIRECT_GROUP,
            id: 0x1234,
            source_ip: Ipv4Addr::new(10, 1, 2, 3),
            source_port: 138,
            source: NbName::new("QS400", 0x00),
            dest: NbName::new("WORKGROUP", 0x1D),
            data,
        }
    }

    #[test]
    fn datagram_header_as_rfc_1002_lays_it_out() {
        let d = dgm(vec![1, 2, 3]);
        let b = d.encode();
        assert_eq!(b[0], DIRECT_GROUP);
        assert_eq!(b[1], FIRST);
        assert_eq!(&b[2..4], &[0x12, 0x34]);
        assert_eq!(&b[4..8], &[10, 1, 2, 3]);
        assert_eq!(&b[8..10], &[0, 138]);
        // Two names of 34 bytes and the data.
        assert_eq!(&b[10..12], &[0, 71]);
        assert_eq!(&b[12..14], &[0, 0]);
        assert_eq!(b.len(), 14 + 71);
        // "QS400" padded with spaces, suffix 00: Q is 0x51 -> "FB".
        assert_eq!(&b[14..17], &[32, b'F', b'B']);
        assert_eq!(Datagram::parse(&b).unwrap(), d);
    }

    #[test]
    fn fragments_errors_and_short_datagrams_are_not_read() {
        let mut b = dgm(vec![0; 4]).encode();
        b[1] |= MORE;
        assert_eq!(Datagram::parse(&b), Err(NbdgmError::NotData));
        let mut b = dgm(vec![0; 4]).encode();
        b[0] = 0x13;
        assert_eq!(Datagram::parse(&b), Err(NbdgmError::NotData));
        let b = dgm(vec![0; 4]).encode();
        assert_eq!(Datagram::parse(&b[..40]), Err(NbdgmError::Truncated));
        assert_eq!(Datagram::parse(&b[..5]), Err(NbdgmError::Truncated));
        assert_eq!(
            Datagram::parse(&vec![DIRECT_GROUP; MAX_DATAGRAM + 1]),
            Err(NbdgmError::TooLong)
        );
        let mut b = dgm(vec![0; 4]).encode();
        b[14] = 7;
        assert_eq!(Datagram::parse(&b), Err(NbdgmError::BadName));
    }

    #[test]
    fn mailslot_writes_round_trip_with_ms_mail_offsets() {
        let a = Announcement {
            periodicity: 60_000,
            server: "qs400".into(),
            os_version: (6, 1),
            server_type: SV_TYPE_WORKSTATION | SV_TYPE_SERVER,
            comment: "Quartz".into(),
        };
        let frame = a.encode();
        let smb = mailslot_write(BROWSE, &frame);
        // WordCount 17, then TotalDataCount, the offsets at 86 (the data
        // right after "\MAILSLOT\BROWSE" and its terminator).
        assert_eq!(smb[32], 17);
        assert_eq!(
            usize::from(u16::from_le_bytes([smb[35], smb[36]])),
            frame.len()
        );
        assert_eq!(u16::from_le_bytes([smb[53], smb[54]]), 86);
        assert_eq!(u16::from_le_bytes([smb[57], smb[58]]), 86);
        assert_eq!(&smb[59..61], &[3, 0]);
        assert_eq!(&smb[61..67], &[1, 0, 1, 0, 2, 0]);
        assert_eq!(&smb[69..86], b"\\MAILSLOT\\BROWSE\0");
        let (name, data) = parse_mailslot(&smb).unwrap();
        assert_eq!(name, BROWSE);
        assert_eq!(data, &frame[..]);
        let back = Announcement::parse(data).unwrap();
        assert_eq!(back.server, "QS400");
        assert_eq!(back.server_type, 3);
        assert_eq!(back.periodicity, 60_000);
        assert_eq!(back.comment, "Quartz");
        assert_eq!(&frame[28..32], &[15, 1, 0x55, 0xAA]);
    }

    #[test]
    fn other_smb_messages_and_bad_offsets_are_refused() {
        let mut smb = mailslot_write(BROWSE, &[2, 0, b'X', 0]);
        assert!(is_announcement_request(parse_mailslot(&smb).unwrap().1));
        smb[4] = 0x72;
        assert_eq!(parse_mailslot(&smb), Err(NbdgmError::NotMailslot));
        let mut smb = mailslot_write(BROWSE, &[2, 0]);
        smb[57] = 0xFF;
        assert_eq!(parse_mailslot(&smb), Err(NbdgmError::Truncated));
        assert_eq!(parse_mailslot(&[0xFF; 10]), Err(NbdgmError::Truncated));
    }

    #[test]
    fn comments_are_cut_and_kept_printable() {
        let a = Announcement {
            periodicity: 0,
            server: "A-VERY-LONG-SERVER-NAME".into(),
            os_version: (6, 1),
            server_type: 0,
            comment: format!("caf\u{e9} {}", "x".repeat(80)),
        };
        let b = a.encode();
        assert_eq!(b.len(), 32 + MAX_COMMENT + 1);
        let back = Announcement::parse(&b).unwrap();
        assert_eq!(back.server, "A-VERY-LONG-SER");
        assert!(back.comment.starts_with("caf??"));
    }
}
