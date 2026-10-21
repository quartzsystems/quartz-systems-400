// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Netlink wire format: `nlmsghdr`, `ifinfomsg`, `rtattr`.

use rustix::io::Errno;

/// `NLMSG_ERROR`.
pub const NLMSG_ERROR: u16 = 2;
/// `NLMSG_DONE`.
pub const NLMSG_DONE: u16 = 3;
/// `RTM_NEWLINK`.
pub const RTM_NEWLINK: u16 = 16;
/// `RTM_GETLINK`.
pub const RTM_GETLINK: u16 = 18;
/// `NLM_F_REQUEST`.
pub const NLM_F_REQUEST: u16 = 0x01;
/// `NLM_F_ACK`.
pub const NLM_F_ACK: u16 = 0x04;
/// `NLM_F_ROOT | NLM_F_MATCH`.
pub const NLM_F_DUMP: u16 = 0x300;
/// `IFLA_IFNAME`.
pub const IFLA_IFNAME: u16 = 3;
/// `IFF_UP`.
pub const IFF_UP: u32 = 1;

const NLMSG_HDRLEN: usize = 16;
const IFINFO_LEN: usize = 16;

/// Build `nlmsghdr` + `ifinfomsg` for a link request.
pub fn link_request(
    kind: u16,
    flags: u16,
    seq: u32,
    index: i32,
    ifflags: u32,
    change: u32,
) -> Vec<u8> {
    let len = u32::try_from(NLMSG_HDRLEN + IFINFO_LEN).expect("small");
    let mut m = Vec::with_capacity(len as usize);
    m.extend_from_slice(&len.to_ne_bytes());
    m.extend_from_slice(&kind.to_ne_bytes());
    m.extend_from_slice(&flags.to_ne_bytes());
    m.extend_from_slice(&seq.to_ne_bytes());
    m.extend_from_slice(&0u32.to_ne_bytes()); // pid: kernel fills
    // ifinfomsg: family, pad, type, index, flags, change
    m.push(0); // AF_UNSPEC
    m.push(0);
    m.extend_from_slice(&0u16.to_ne_bytes());
    m.extend_from_slice(&index.to_ne_bytes());
    m.extend_from_slice(&ifflags.to_ne_bytes());
    m.extend_from_slice(&change.to_ne_bytes());
    m
}

/// One received message.
pub struct Message<'a> {
    /// `nlmsg_type`.
    pub kind: u16,
    /// Payload after the header.
    pub payload: &'a [u8],
}

impl Message<'_> {
    /// For `NLMSG_ERROR`: the errno, or `None` for an ack (error 0).
    pub fn error(&self) -> Option<Errno> {
        let code = i32::from_ne_bytes(self.payload.get(..4)?.try_into().ok()?);
        if code == 0 {
            None
        } else {
            Some(Errno::from_raw_os_error(-code))
        }
    }

    /// For `RTM_NEWLINK`: `(index, flags)` from `ifinfomsg`.
    pub fn ifinfo(&self) -> Option<(i32, u32)> {
        let p = self.payload.get(..IFINFO_LEN)?;
        let index = i32::from_ne_bytes(p[4..8].try_into().ok()?);
        let flags = u32::from_ne_bytes(p[8..12].try_into().ok()?);
        Some((index, flags))
    }

    /// Value of the first `rtattr` with `want` after `ifinfomsg`.
    pub fn attr(&self, want: u16) -> Option<&[u8]> {
        let mut rest = self.payload.get(IFINFO_LEN..)?;
        while rest.len() >= 4 {
            let len = usize::from(u16::from_ne_bytes(rest[..2].try_into().ok()?));
            let kind = u16::from_ne_bytes(rest[2..4].try_into().ok()?);
            if len < 4 || len > rest.len() {
                return None;
            }
            if kind == want {
                return Some(&rest[4..len]);
            }
            rest = rest.get(align4(len)..).unwrap_or(&[]);
        }
        None
    }
}

/// Split a receive buffer into messages.
pub fn messages(buf: &[u8]) -> Vec<Message<'_>> {
    let mut out = Vec::new();
    let mut rest = buf;
    while rest.len() >= NLMSG_HDRLEN {
        let Ok(len_bytes) = rest[..4].try_into() else {
            break;
        };
        let len = u32::from_ne_bytes(len_bytes) as usize;
        let Ok(kind_bytes) = rest[4..6].try_into() else {
            break;
        };
        let kind = u16::from_ne_bytes(kind_bytes);
        if len < NLMSG_HDRLEN || len > rest.len() {
            break;
        }
        out.push(Message {
            kind,
            payload: &rest[NLMSG_HDRLEN..len],
        });
        rest = &rest[align4(len).min(rest.len())..];
    }
    out
}

fn align4(n: usize) -> usize {
    (n + 3) & !3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_layout() {
        let m = link_request(RTM_GETLINK, NLM_F_REQUEST | NLM_F_DUMP, 1, 0, 0, 0);
        assert_eq!(m.len(), 32);
        assert_eq!(u32::from_ne_bytes(m[..4].try_into().unwrap()), 32);
        assert_eq!(u16::from_ne_bytes(m[4..6].try_into().unwrap()), RTM_GETLINK);
    }

    #[test]
    fn parses_a_newlink_with_name() {
        let mut p = vec![0u8; IFINFO_LEN];
        p[4..8].copy_from_slice(&3i32.to_ne_bytes());
        p[8..12].copy_from_slice(&IFF_UP.to_ne_bytes());
        // rtattr IFLA_IFNAME "lo\0" (len 7, padded to 8)
        p.extend_from_slice(&7u16.to_ne_bytes());
        p.extend_from_slice(&IFLA_IFNAME.to_ne_bytes());
        p.extend_from_slice(b"lo\0\0");
        let mut m = Vec::new();
        let len = u32::try_from(NLMSG_HDRLEN + p.len()).unwrap();
        m.extend_from_slice(&len.to_ne_bytes());
        m.extend_from_slice(&RTM_NEWLINK.to_ne_bytes());
        m.extend_from_slice(&[0u8; 10]);
        m.extend_from_slice(&p);
        let msgs = messages(&m);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].ifinfo(), Some((3, IFF_UP)));
        assert_eq!(msgs[0].attr(IFLA_IFNAME), Some(&b"lo\0"[..]));
    }
}
