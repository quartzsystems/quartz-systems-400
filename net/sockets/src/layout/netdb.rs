// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `hostent`, `servent` and `addrinfo` results as relocatable images:
//! the structure, its strings and arrays in one byte run, with the
//! pointer slots recorded by offset. The owner places the image in job
//! storage and writes a pointer of its own kind into every slot; this
//! module never sees a pointer's bytes. Strings are EBCDIC and
//! NUL-terminated, as a C program reads them.

use super::addr::{AddrForm, SockAddr};
use super::{put_i32, put_u32};
use crate::consts::{NETDB_MAX_ALIASES, NETDB_MAX_ARRAY_SIZE, NETDB_MAX_HOST_ADDR_ARRAY_SIZE};

/// An image.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Image {
    /// The bytes; pointer slots are zero.
    pub bytes: Vec<u8>,
    /// `(slot offset, target offset)`: the slot takes a pointer to the
    /// target, both inside `bytes`.
    pub ptrs: Vec<(usize, usize)>,
}

impl Image {
    fn align(&mut self, n: usize) {
        while !self.bytes.len().is_multiple_of(n) {
            self.bytes.push(0);
        }
    }

    fn push(&mut self, b: &[u8]) -> usize {
        let at = self.bytes.len();
        self.bytes.extend_from_slice(b);
        at
    }

    fn cstr(&mut self, s: &str) -> usize {
        let mut b = ccsid::to_ebcdic(s).unwrap_or_default();
        b.push(0);
        self.push(&b)
    }

    /// `n + 1` pointer slots, the last null; their first offset.
    fn ptr_array(&mut self, n: usize) -> usize {
        self.align(16);
        let at = self.bytes.len();
        self.bytes.resize(at + 16 * (n + 1), 0);
        at
    }

    /// Split at `head`: the head's bytes, the rest, and the pointer
    /// slots of each with targets relative to the rest (every target
    /// lies in the rest).
    #[must_use]
    pub fn split(&self, head: usize) -> Split {
        let mut s = Split {
            head: self.bytes[..head].to_vec(),
            data: self.bytes[head..].to_vec(),
            head_ptrs: Vec::new(),
            data_ptrs: Vec::new(),
        };
        for &(slot, target) in &self.ptrs {
            let t = target.saturating_sub(head);
            if slot < head {
                s.head_ptrs.push((slot, t));
            } else {
                s.data_ptrs.push((slot - head, t));
            }
        }
        s
    }
}

/// An image split into a caller's structure and its data area.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Split {
    /// The structure.
    pub head: Vec<u8>,
    /// The data the structure points into.
    pub data: Vec<u8>,
    /// Slots in `head` with targets in `data`.
    pub head_ptrs: Vec<(usize, usize)>,
    /// Slots in `data` with targets in `data`.
    pub data_ptrs: Vec<(usize, usize)>,
}

/// `struct hostent` at offset 0, then its name, aliases and
/// addresses. Aliases past `NETDB_MAX_ARRAY_SIZE` and addresses past
/// `NETDB_MAX_HOST_ADDR_ARRAY_SIZE` are dropped, as IBM's buffers hold
/// no more.
#[must_use]
pub fn hostent(name: &str, aliases: &[String], family: i32, addrs: &[Vec<u8>]) -> Image {
    let mut im = Image::default();
    im.bytes.resize(64, 0);
    let aliases = &aliases[..aliases.len().min(NETDB_MAX_ARRAY_SIZE)];
    let addrs = &addrs[..addrs.len().min(NETDB_MAX_HOST_ADDR_ARRAY_SIZE)];
    let h_length = addrs.first().map_or(if family == crate::consts::AF_INET6 { 16 } else { 4 }, Vec::len);
    put_i32(&mut im.bytes, 32, family);
    put_i32(&mut im.bytes, 36, i32::try_from(h_length).expect("small"));
    let name_at = im.cstr(name);
    im.ptrs.push((0, name_at));
    let alias_strs: Vec<usize> = aliases.iter().map(|a| im.cstr(a)).collect();
    let aliases_at = im.ptr_array(alias_strs.len());
    im.ptrs.push((16, aliases_at));
    for (i, at) in alias_strs.iter().enumerate() {
        im.ptrs.push((aliases_at + 16 * i, *at));
    }
    let list_at = im.ptr_array(addrs.len());
    im.ptrs.push((48, list_at));
    let addr_ats: Vec<usize> = addrs.iter().map(|a| im.push(a)).collect();
    for (i, at) in addr_ats.iter().enumerate() {
        im.ptrs.push((list_at + 16 * i, *at));
    }
    im
}

/// `struct servent` at offset 0, then its name, aliases and protocol.
#[must_use]
pub fn servent(name: &str, aliases: &[String], port: u16, proto: &str) -> Image {
    let mut im = Image::default();
    im.bytes.resize(64, 0);
    put_i32(&mut im.bytes, 32, i32::from(port));
    let name_at = im.cstr(name);
    im.ptrs.push((0, name_at));
    let aliases = &aliases[..aliases.len().min(NETDB_MAX_ALIASES)];
    let alias_strs: Vec<usize> = aliases.iter().map(|a| im.cstr(a)).collect();
    let proto_at = im.cstr(proto);
    im.ptrs.push((48, proto_at));
    let aliases_at = im.ptr_array(alias_strs.len());
    im.ptrs.push((16, aliases_at));
    for (i, at) in alias_strs.iter().enumerate() {
        im.ptrs.push((aliases_at + 16 * i, *at));
    }
    im
}

/// One `addrinfo` result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddrInfo {
    /// `ai_flags`.
    pub flags: i32,
    /// `ai_socktype`.
    pub socktype: i32,
    /// `ai_protocol`.
    pub protocol: i32,
    /// The address; `ai_family` and `ai_addrlen` come from it.
    pub addr: SockAddr,
    /// `ai_canonname`, on the first entry only.
    pub canonname: Option<String>,
}

/// A chain of `struct addrinfo`, the first at offset 0, each
/// `ai_next` pointing at the following one; the addresses in `form`.
#[must_use]
pub fn addrinfo(list: &[AddrInfo], form: AddrForm) -> Image {
    let mut im = Image::default();
    im.bytes.resize(96 * list.len(), 0);
    for (i, e) in list.iter().enumerate() {
        let at = 96 * i;
        put_i32(&mut im.bytes, at, e.flags);
        put_i32(&mut im.bytes, at + 4, e.addr.family());
        put_i32(&mut im.bytes, at + 8, e.socktype);
        put_i32(&mut im.bytes, at + 12, e.protocol);
        put_u32(&mut im.bytes, at + 16, u32::try_from(e.addr.size()).expect("small"));
        if i + 1 < list.len() {
            im.ptrs.push((at + 64, at + 96));
        }
    }
    for (i, e) in list.iter().enumerate() {
        let at = 96 * i;
        im.align(16);
        let sa = im.push(&e.addr.to_bytes(form));
        im.ptrs.push((at + 48, sa));
        if let Some(c) = &e.canonname {
            let s = im.cstr(c);
            im.ptrs.push((at + 32, s));
        }
    }
    im
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, SocketAddrV4};

    use super::*;

    fn target(im: &Image, slot: usize) -> usize {
        im.ptrs
            .iter()
            .find(|(s, _)| *s == slot)
            .map_or_else(|| panic!("no pointer at {slot}"), |(_, t)| *t)
    }

    #[test]
    fn hostent_image() {
        let im = hostent(
            "host.example",
            &["h".into(), "host".into()],
            2,
            &[vec![10, 0, 0, 1], vec![10, 0, 0, 2]],
        );
        assert_eq!(&im.bytes[32..40], &[0, 0, 0, 2, 0, 0, 0, 4]);
        let name = target(&im, 0);
        assert_eq!(im.bytes[name], 0x88, "EBCDIC h");
        let aliases = target(&im, 16);
        assert!(aliases.is_multiple_of(16));
        let a0 = target(&im, aliases);
        assert_eq!(&im.bytes[a0..a0 + 2], &[0x88, 0]);
        assert_eq!(&im.bytes[aliases + 32..aliases + 48], &[0; 16], "null end");
        assert!(im.ptrs.iter().all(|(s, _)| *s != aliases + 32));
        let list = target(&im, 48);
        let p1 = target(&im, list + 16);
        assert_eq!(&im.bytes[p1..p1 + 4], &[10, 0, 0, 2]);
        let s = im.split(64);
        assert_eq!(s.head.len(), 64);
        assert_eq!(s.head_ptrs.len(), 3);
        assert_eq!(s.head_ptrs[0], (0, name - 64));
        assert!(s.data_ptrs.iter().all(|(slot, t)| *slot < s.data.len() && *t < s.data.len()));
    }

    #[test]
    fn servent_and_addrinfo_images() {
        let im = servent("http", &["www".into()], 80, "tcp");
        assert_eq!(&im.bytes[32..36], &[0, 0, 0, 80]);
        let proto = target(&im, 48);
        assert_eq!(ccsid::from_ebcdic(&im.bytes[proto..proto + 3]), "tcp");
        let entries = vec![
            AddrInfo {
                flags: 0,
                socktype: 1,
                protocol: 6,
                addr: SockAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 80)),
                canonname: Some("localhost".into()),
            },
            AddrInfo {
                flags: 0,
                socktype: 2,
                protocol: 17,
                addr: SockAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 80)),
                canonname: None,
            },
        ];
        let im = addrinfo(&entries, AddrForm::Bsd43);
        assert_eq!(target(&im, 64), 96, "ai_next");
        assert!(im.ptrs.iter().all(|(s, _)| *s != 96 + 64), "last ai_next null");
        assert_eq!(&im.bytes[96 + 4..96 + 20], &[0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 17, 0, 0, 0, 16]);
        let sa = target(&im, 48);
        assert_eq!(&im.bytes[sa..sa + 8], &[0, 2, 0, 80, 127, 0, 0, 1]);
        assert!(im.ptrs.iter().any(|(s, _)| *s == 32));
        assert!(im.ptrs.iter().all(|(s, _)| *s != 96 + 32));
    }
}
