// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! IBM's structure layouts in job storage: integers big-endian,
//! pointers 16 bytes on 16-byte boundaries (`#pragma datamodel(P128)`),
//! `size_t`, `int` and `long` 4 bytes. Two generations of address
//! structures exist: BSD 4.3 (`sa_family` a 2-byte field, the plain
//! names) and BSD 4.4 (`sa_len` then a 1-byte `sa_family`, the
//! `_XOPEN_SOURCE` headers whose calls map to `qso_*98`). The tables in
//! `docs/TCPIP.md` "Sockets" repeat these; `tests/spec.rs` keeps them
//! equal.

pub mod addr;
pub mod misc;
pub mod netdb;

/// A field of a structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    /// Name as the header spells it.
    pub name: &'static str,
    /// Byte offset.
    pub offset: usize,
    /// Bytes.
    pub size: usize,
}

/// A structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    /// Name as the spec heading spells it.
    pub name: &'static str,
    /// `sizeof`.
    pub size: usize,
    /// Fields in offset order.
    pub fields: &'static [Field],
}

const fn f(name: &'static str, offset: usize, size: usize) -> Field {
    Field { name, offset, size }
}

/// `struct sockaddr`, BSD 4.3.
pub const SOCKADDR_43: Layout = Layout {
    name: "sockaddr (BSD 4.3)",
    size: 16,
    fields: &[f("sa_family", 0, 2), f("sa_data", 2, 14)],
};

/// `struct sockaddr`, BSD 4.4.
pub const SOCKADDR_44: Layout = Layout {
    name: "sockaddr (BSD 4.4)",
    size: 16,
    fields: &[f("sa_len", 0, 1), f("sa_family", 1, 1), f("sa_data", 2, 14)],
};

/// `struct sockaddr_in`, BSD 4.3.
pub const SOCKADDR_IN_43: Layout = Layout {
    name: "sockaddr_in (BSD 4.3)",
    size: 16,
    fields: &[
        f("sin_family", 0, 2),
        f("sin_port", 2, 2),
        f("sin_addr", 4, 4),
        f("sin_zero", 8, 8),
    ],
};

/// `struct sockaddr_in`, BSD 4.4.
pub const SOCKADDR_IN_44: Layout = Layout {
    name: "sockaddr_in (BSD 4.4)",
    size: 16,
    fields: &[
        f("sin_len", 0, 1),
        f("sin_family", 1, 1),
        f("sin_port", 2, 2),
        f("sin_addr", 4, 4),
        f("sin_zero", 8, 8),
    ],
};

/// `struct sockaddr_in6`, BSD 4.3.
pub const SOCKADDR_IN6_43: Layout = Layout {
    name: "sockaddr_in6 (BSD 4.3)",
    size: 28,
    fields: &[
        f("sin6_family", 0, 2),
        f("sin6_port", 2, 2),
        f("sin6_flowinfo", 4, 4),
        f("sin6_addr", 8, 16),
        f("sin6_scope_id", 24, 4),
    ],
};

/// `struct sockaddr_in6`, BSD 4.4.
pub const SOCKADDR_IN6_44: Layout = Layout {
    name: "sockaddr_in6 (BSD 4.4)",
    size: 28,
    fields: &[
        f("sin6_len", 0, 1),
        f("sin6_family", 1, 1),
        f("sin6_port", 2, 2),
        f("sin6_flowinfo", 4, 4),
        f("sin6_addr", 8, 16),
        f("sin6_scope_id", 24, 4),
    ],
};

/// `struct sockaddr_un`, BSD 4.3.
pub const SOCKADDR_UN_43: Layout = Layout {
    name: "sockaddr_un (BSD 4.3)",
    size: 128,
    fields: &[f("sun_family", 0, 2), f("sun_path", 2, 126)],
};

/// `struct sockaddr_un`, BSD 4.4.
pub const SOCKADDR_UN_44: Layout = Layout {
    name: "sockaddr_un (BSD 4.4)",
    size: 128,
    fields: &[f("sun_len", 0, 1), f("sun_family", 1, 1), f("sun_path", 2, 126)],
};

/// `struct sockaddr_storage`.
pub const SOCKADDR_STORAGE: Layout = Layout {
    name: "sockaddr_storage",
    size: 304,
    fields: &[],
};

/// `struct iovec`.
pub const IOVEC: Layout = Layout {
    name: "iovec",
    size: 32,
    fields: &[f("iov_base", 0, 16), f("iov_len", 16, 4)],
};

/// `struct msghdr` (BSD 4.4; the 4.3 form has `msg_accrights` and
/// `msg_accrightslen` at the offsets of `msg_control` and
/// `msg_controllen`).
pub const MSGHDR: Layout = Layout {
    name: "msghdr",
    size: 96,
    fields: &[
        f("msg_name", 0, 16),
        f("msg_namelen", 16, 4),
        f("msg_iov", 32, 16),
        f("msg_iovlen", 48, 4),
        f("msg_control", 64, 16),
        f("msg_controllen", 80, 4),
        f("msg_flags", 84, 4),
    ],
};

/// `struct cmsghdr`.
pub const CMSGHDR: Layout = Layout {
    name: "cmsghdr",
    size: 12,
    fields: &[f("cmsg_len", 0, 4), f("cmsg_level", 4, 4), f("cmsg_type", 8, 4)],
};

/// `struct linger`.
pub const LINGER: Layout = Layout {
    name: "linger",
    size: 8,
    fields: &[f("l_onoff", 0, 4), f("l_linger", 4, 4)],
};

/// `struct timeval`.
pub const TIMEVAL: Layout = Layout {
    name: "timeval",
    size: 8,
    fields: &[f("tv_sec", 0, 4), f("tv_usec", 4, 4)],
};

/// `struct pollfd`.
pub const POLLFD: Layout = Layout {
    name: "pollfd",
    size: 8,
    fields: &[f("fd", 0, 4), f("events", 4, 2), f("revents", 6, 2)],
};

/// `fd_set`.
pub const FD_SET: Layout = Layout {
    name: "fd_set",
    size: 28,
    fields: &[f("fdes", 0, 28)],
};

/// `struct ip_mreq`.
pub const IP_MREQ: Layout = Layout {
    name: "ip_mreq",
    size: 8,
    fields: &[f("imr_multiaddr", 0, 4), f("imr_interface", 4, 4)],
};

/// `struct ipv6_mreq`.
pub const IPV6_MREQ: Layout = Layout {
    name: "ipv6_mreq",
    size: 20,
    fields: &[f("ipv6mr_multiaddr", 0, 16), f("ipv6mr_interface", 16, 4)],
};

/// `struct hostent`.
pub const HOSTENT: Layout = Layout {
    name: "hostent",
    size: 64,
    fields: &[
        f("h_name", 0, 16),
        f("h_aliases", 16, 16),
        f("h_addrtype", 32, 4),
        f("h_length", 36, 4),
        f("h_addr_list", 48, 16),
    ],
};

/// `struct hostent_data`: opaque to callers; what matters is its size.
pub const HOSTENT_DATA: Layout = Layout {
    name: "hostent_data",
    size: 21008,
    fields: &[],
};

/// `struct servent`.
pub const SERVENT: Layout = Layout {
    name: "servent",
    size: 64,
    fields: &[
        f("s_name", 0, 16),
        f("s_aliases", 16, 16),
        f("s_port", 32, 4),
        f("s_proto", 48, 16),
    ],
};

/// `struct addrinfo`.
pub const ADDRINFO: Layout = Layout {
    name: "addrinfo",
    size: 96,
    fields: &[
        f("ai_flags", 0, 4),
        f("ai_family", 4, 4),
        f("ai_socktype", 8, 4),
        f("ai_protocol", 12, 4),
        f("ai_addrlen", 16, 4),
        f("ai_canonname", 32, 16),
        f("ai_addr", 48, 16),
        f("ai_next", 64, 16),
        f("ai_eflags", 80, 4),
    ],
};

/// Every layout, for the spec test.
#[must_use]
pub fn layouts() -> Vec<&'static Layout> {
    vec![
        &SOCKADDR_43,
        &SOCKADDR_44,
        &SOCKADDR_IN_43,
        &SOCKADDR_IN_44,
        &SOCKADDR_IN6_43,
        &SOCKADDR_IN6_44,
        &SOCKADDR_UN_43,
        &SOCKADDR_UN_44,
        &SOCKADDR_STORAGE,
        &IOVEC,
        &MSGHDR,
        &CMSGHDR,
        &LINGER,
        &TIMEVAL,
        &POLLFD,
        &FD_SET,
        &IP_MREQ,
        &IPV6_MREQ,
        &HOSTENT,
        &HOSTENT_DATA,
        &SERVENT,
        &ADDRINFO,
    ]
}

/// A big-endian `i32` at an offset; 0 past the end.
#[must_use]
pub fn i32_at(b: &[u8], off: usize) -> i32 {
    b.get(off..off + 4)
        .map_or(0, |x| i32::from_be_bytes(x.try_into().expect("4 bytes")))
}

/// A big-endian `u32` at an offset; 0 past the end.
#[must_use]
pub fn u32_at(b: &[u8], off: usize) -> u32 {
    i32_at(b, off).cast_unsigned()
}

/// A big-endian `u16` at an offset; 0 past the end.
#[must_use]
pub fn u16_at(b: &[u8], off: usize) -> u16 {
    b.get(off..off + 2)
        .map_or(0, |x| u16::from_be_bytes([x[0], x[1]]))
}

/// A big-endian `i16` at an offset; 0 past the end.
#[must_use]
pub fn i16_at(b: &[u8], off: usize) -> i16 {
    u16_at(b, off).cast_signed()
}

/// Store a big-endian `i32`, if it fits.
pub fn put_i32(b: &mut [u8], off: usize, v: i32) {
    if let Some(x) = b.get_mut(off..off + 4) {
        x.copy_from_slice(&v.to_be_bytes());
    }
}

/// Store a big-endian `u32`, if it fits.
pub fn put_u32(b: &mut [u8], off: usize, v: u32) {
    put_i32(b, off, v.cast_signed());
}

/// Store a big-endian `u16`, if it fits.
pub fn put_u16(b: &mut [u8], off: usize, v: u16) {
    if let Some(x) = b.get_mut(off..off + 2) {
        x.copy_from_slice(&v.to_be_bytes());
    }
}

/// Store a big-endian `i16`, if it fits.
pub fn put_i16(b: &mut [u8], off: usize, v: i16) {
    put_u16(b, off, v.cast_unsigned());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_lie_inside_and_in_order() {
        for l in layouts() {
            let mut last = 0;
            for fl in l.fields {
                assert!(fl.offset >= last, "{} {}", l.name, fl.name);
                assert!(fl.offset + fl.size <= l.size, "{} {}", l.name, fl.name);
                last = fl.offset + fl.size;
            }
        }
    }

    #[test]
    fn integers_round_trip() {
        let mut b = [0u8; 8];
        put_i32(&mut b, 0, -2);
        put_u16(&mut b, 4, 0x1234);
        put_i16(&mut b, 6, -1);
        assert_eq!(b, [0xff, 0xff, 0xff, 0xfe, 0x12, 0x34, 0xff, 0xff]);
        assert_eq!(i32_at(&b, 0), -2);
        assert_eq!(u32_at(&b, 0), 0xffff_fffe);
        assert_eq!(u16_at(&b, 4), 0x1234);
        assert_eq!(i16_at(&b, 6), -1);
        assert_eq!(i32_at(&b, 6), 0, "past the end");
        put_i32(&mut b, 6, 7);
        assert_eq!(i16_at(&b, 6), -1, "a store past the end is dropped");
    }
}
