// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Socket addresses between Rust values and IBM's structures.

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};

use super::{put_u16, put_u32, u16_at, u32_at};
use crate::consts::{AF_INET, AF_INET6, AF_UNIX, AF_UNSPEC};
use crate::Errno;

/// Which generation of address structure a call reads and writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddrForm {
    /// `sa_family` is two bytes at offset 0 (the plain call names).
    Bsd43,
    /// `sa_len` at 0, `sa_family` one byte at 1 (`qso_*98`).
    Bsd44,
}

/// A socket address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SockAddr {
    /// `AF_INET`.
    V4(SocketAddrV4),
    /// `AF_INET6`.
    V6(SocketAddrV6),
    /// `AF_UNIX`: an IFS path.
    Unix(String),
    /// `AF_UNSPEC`: a structure with no address (what `connect` uses
    /// to dissolve a datagram association).
    Unspec,
}

/// `sun_path` holds this many bytes.
pub const SUN_PATH_LEN: usize = 126;

impl SockAddr {
    /// The family number.
    #[must_use]
    pub fn family(&self) -> i32 {
        match self {
            SockAddr::V4(_) => AF_INET,
            SockAddr::V6(_) => AF_INET6,
            SockAddr::Unix(_) => AF_UNIX,
            SockAddr::Unspec => AF_UNSPEC,
        }
    }

    /// The structure's size for this family.
    #[must_use]
    pub fn size(&self) -> usize {
        match self {
            SockAddr::V4(_) | SockAddr::Unspec => 16,
            SockAddr::V6(_) => 28,
            SockAddr::Unix(_) => 128,
        }
    }

    /// The port, 0 for a Unix address.
    #[must_use]
    pub fn port(&self) -> u16 {
        match self {
            SockAddr::V4(a) => a.port(),
            SockAddr::V6(a) => a.port(),
            _ => 0,
        }
    }

    /// The IP address, if any.
    #[must_use]
    pub fn ip(&self) -> Option<std::net::IpAddr> {
        match self {
            SockAddr::V4(a) => Some((*a.ip()).into()),
            SockAddr::V6(a) => Some((*a.ip()).into()),
            _ => None,
        }
    }

    /// From a Rust socket address.
    #[must_use]
    pub fn from_std(a: std::net::SocketAddr) -> Self {
        match a {
            std::net::SocketAddr::V4(v) => SockAddr::V4(v),
            std::net::SocketAddr::V6(v) => SockAddr::V6(v),
        }
    }

    /// Write the structure into `out`, which may be shorter (the bytes
    /// that fit are written, as `getsockname` truncates); the full size.
    pub fn encode(&self, form: AddrForm, out: &mut [u8]) -> usize {
        let mut b = vec![0u8; self.size()];
        let fam = u8::try_from(self.family()).unwrap_or(0);
        match form {
            AddrForm::Bsd43 => put_u16(&mut b, 0, u16::from(fam)),
            AddrForm::Bsd44 => {
                b[0] = u8::try_from(self.size()).unwrap_or(0);
                b[1] = fam;
            }
        }
        match self {
            SockAddr::V4(a) => {
                put_u16(&mut b, 2, a.port());
                b[4..8].copy_from_slice(&a.ip().octets());
            }
            SockAddr::V6(a) => {
                put_u16(&mut b, 2, a.port());
                put_u32(&mut b, 4, a.flowinfo());
                b[8..24].copy_from_slice(&a.ip().octets());
                put_u32(&mut b, 24, a.scope_id());
            }
            SockAddr::Unix(path) => {
                let p = ccsid::to_ebcdic(path).unwrap_or_default();
                let n = p.len().min(SUN_PATH_LEN - 1);
                b[2..2 + n].copy_from_slice(&p[..n]);
            }
            SockAddr::Unspec => {}
        }
        let n = out.len().min(b.len());
        out[..n].copy_from_slice(&b[..n]);
        b.len()
    }

    /// The structure's bytes.
    #[must_use]
    pub fn to_bytes(&self, form: AddrForm) -> Vec<u8> {
        let mut b = vec![0u8; self.size()];
        self.encode(form, &mut b);
        b
    }

    /// Read a structure of `len` bytes. `EAFNOSUPPORT` for a family the
    /// API does not take, `EINVAL` when too short for its family.
    pub fn decode(b: &[u8], form: AddrForm) -> Result<Self, Errno> {
        let fam = match form {
            AddrForm::Bsd43 => i32::from(u16_at(b, 0)),
            AddrForm::Bsd44 => i32::from(*b.get(1).ok_or(Errno::EINVAL)?),
        };
        match fam {
            AF_INET => {
                if b.len() < 8 {
                    return Err(Errno::EINVAL);
                }
                let ip = Ipv4Addr::new(b[4], b[5], b[6], b[7]);
                Ok(SockAddr::V4(SocketAddrV4::new(ip, u16_at(b, 2))))
            }
            AF_INET6 => {
                if b.len() < 24 {
                    return Err(Errno::EINVAL);
                }
                let octets: [u8; 16] = b[8..24].try_into().expect("16 bytes");
                Ok(SockAddr::V6(SocketAddrV6::new(
                    Ipv6Addr::from(octets),
                    u16_at(b, 2),
                    u32_at(b, 4),
                    u32_at(b, 24),
                )))
            }
            AF_UNIX => {
                let path = b.get(2..).unwrap_or(&[]);
                let path = &path[..path.len().min(SUN_PATH_LEN)];
                let end = path.iter().position(|&c| c == 0).unwrap_or(path.len());
                Ok(SockAddr::Unix(ccsid::from_ebcdic(&path[..end])))
            }
            AF_UNSPEC => Ok(SockAddr::Unspec),
            _ => Err(Errno::EAFNOSUPPORT),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v4_both_forms() {
        let a = SockAddr::V4(SocketAddrV4::new(Ipv4Addr::new(10, 1, 2, 3), 8080));
        let b43 = a.to_bytes(AddrForm::Bsd43);
        assert_eq!(&b43[..8], &[0, 2, 0x1f, 0x90, 10, 1, 2, 3]);
        assert_eq!(b43.len(), 16);
        let b44 = a.to_bytes(AddrForm::Bsd44);
        assert_eq!(&b44[..8], &[16, 2, 0x1f, 0x90, 10, 1, 2, 3]);
        assert_eq!(SockAddr::decode(&b43, AddrForm::Bsd43).unwrap(), a);
        assert_eq!(SockAddr::decode(&b44, AddrForm::Bsd44).unwrap(), a);
        let mut short = [0u8; 4];
        assert_eq!(a.encode(AddrForm::Bsd43, &mut short), 16);
        assert_eq!(short, [0, 2, 0x1f, 0x90]);
        assert_eq!(SockAddr::decode(&b43[..6], AddrForm::Bsd43), Err(Errno::EINVAL));
        assert_eq!(SockAddr::decode(&[0, 6, 0, 0], AddrForm::Bsd43), Err(Errno::EAFNOSUPPORT));
    }

    #[test]
    fn v6_and_unix() {
        let a = SockAddr::V6(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 443, 7, 2));
        let b = a.to_bytes(AddrForm::Bsd44);
        assert_eq!(b.len(), 28);
        assert_eq!(b[0], 28);
        assert_eq!(b[1], 24);
        assert_eq!(&b[2..8], &[1, 0xbb, 0, 0, 0, 7]);
        assert_eq!(b[23], 1);
        assert_eq!(&b[24..], &[0, 0, 0, 2]);
        assert_eq!(SockAddr::decode(&b, AddrForm::Bsd44).unwrap(), a);
        let u = SockAddr::Unix("/tmp/qs.sock".into());
        let b = u.to_bytes(AddrForm::Bsd43);
        assert_eq!(b.len(), 128);
        assert_eq!(&b[..2], &[0, 1]);
        assert_eq!(b[2], 0x61, "EBCDIC slash");
        assert_eq!(SockAddr::decode(&b, AddrForm::Bsd43).unwrap(), u);
        assert_eq!(u.family(), AF_UNIX);
        assert_eq!(SockAddr::decode(&[0, 0], AddrForm::Bsd43).unwrap(), SockAddr::Unspec);
    }
}
