// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The netlink wire format: a 16-byte `nlmsghdr`, a family header, then
//! 4-byte aligned attributes (`u16` length, `u16` type, payload).
//! Integers are host order (`x86_64`: little-endian) unless a family says
//! otherwise; nftables attributes are big-endian.

mod build;
mod parse;

pub use build::Builder;
pub use parse::{Attr, Attrs, Messages, Msg, ip_from, messages};

/// Size of `nlmsghdr`.
pub const NLMSG_HDRLEN: usize = 16;

/// No-op message.
pub const NLMSG_NOOP: u16 = 1;
/// Error or acknowledgement.
pub const NLMSG_ERROR: u16 = 2;
/// End of a dump.
pub const NLMSG_DONE: u16 = 3;

/// A request.
pub const NLM_F_REQUEST: u16 = 0x01;
/// Part of a multipart reply.
pub const NLM_F_MULTI: u16 = 0x02;
/// Ask for an acknowledgement.
pub const NLM_F_ACK: u16 = 0x04;
/// Echo the request to the sender.
pub const NLM_F_ECHO: u16 = 0x08;
/// The dump was interrupted by a change.
pub const NLM_F_DUMP_INTR: u16 = 0x10;
/// Dump the whole table (`ROOT | MATCH`).
pub const NLM_F_DUMP: u16 = 0x300;
/// Replace an existing object.
pub const NLM_F_REPLACE: u16 = 0x100;
/// Fail if the object exists.
pub const NLM_F_EXCL: u16 = 0x200;
/// Create the object if missing.
pub const NLM_F_CREATE: u16 = 0x400;
/// Append to the end of a list.
pub const NLM_F_APPEND: u16 = 0x800;

/// Attribute flag: the payload is nested attributes.
pub const NLA_F_NESTED: u16 = 0x8000;
/// Attribute flag: the payload is network byte order.
pub const NLA_F_NET_BYTEORDER: u16 = 0x4000;
/// Mask of the attribute type proper.
pub const NLA_TYPE_MASK: u16 = !(NLA_F_NESTED | NLA_F_NET_BYTEORDER);

/// Round up to the 4-byte netlink alignment.
pub const fn align(n: usize) -> usize {
    (n + 3) & !3
}

/// A host-order `u16` at `off`, if in range.
pub fn ne_u16(b: &[u8], off: usize) -> Option<u16> {
    Some(u16::from_ne_bytes(b.get(off..off + 2)?.try_into().ok()?))
}

/// A host-order `u32` at `off`, if in range.
pub fn ne_u32(b: &[u8], off: usize) -> Option<u32> {
    Some(u32::from_ne_bytes(b.get(off..off + 4)?.try_into().ok()?))
}

/// A host-order `u64` at `off`, if in range.
pub fn ne_u64(b: &[u8], off: usize) -> Option<u64> {
    Some(u64::from_ne_bytes(b.get(off..off + 8)?.try_into().ok()?))
}

/// A big-endian `u16` at `off`, if in range.
pub fn be_u16(b: &[u8], off: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(off..off + 2)?.try_into().ok()?))
}

/// A big-endian `u32` at `off`, if in range.
pub fn be_u32(b: &[u8], off: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(off..off + 4)?.try_into().ok()?))
}

#[cfg(test)]
mod tests;
