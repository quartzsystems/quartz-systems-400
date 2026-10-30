// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The file server's attribute replies: a listing entry (`0x8005`, a
//! 93-byte template and the name), an open (`0x8002`, 77 bytes), and
//! the object attribute structures OA1 and OA2 for a handle.

use hostsvr::file::id;
use hostsvr::frame::{Frame, Param};
use ifs::ops::Stat;
use ifs::ops::flags::{S_IFDIR, S_IFLNK, S_IFMT};

use super::cp;
use crate::{reply, text};

/// A time as the file server sends it: seconds and microseconds.
pub fn time(ns: i64) -> [u8; 8] {
    let secs = u32::try_from(ns.div_euclid(1_000_000_000)).unwrap_or(0);
    let us = u32::try_from(ns.rem_euclid(1_000_000_000) / 1000).unwrap_or(0);
    let mut b = [0u8; 8];
    b[..4].copy_from_slice(&secs.to_be_bytes());
    b[4..].copy_from_slice(&us.to_be_bytes());
    b
}

fn is_dir(st: &Stat) -> bool {
    st.mode & S_IFMT == S_IFDIR
}

fn is_link(st: &Stat) -> bool {
    st.mode & S_IFMT == S_IFLNK
}

/// The DOS-style fixed attributes: directory `0x10`, archive `0x20`.
pub fn fixed(st: &Stat) -> u32 {
    if is_dir(st) { 0x10 } else { 0x20 }
}

/// The object type: 1 file, 2 directory, 3 symbolic link.
pub fn object_type(st: &Stat) -> u16 {
    if is_dir(st) {
        2
    } else if is_link(st) {
        3
    } else {
        1
    }
}

fn file_id(st: &Stat) -> u32 {
    u32::try_from(st.ino & 0xFFFF_FFFF).unwrap_or(0)
}

fn size32(st: &Stat) -> u32 {
    u32::try_from(st.size).unwrap_or(u32::MAX)
}

fn ccsid(st: &Stat) -> u16 {
    if st.ccsid == 0 { 1200 } else { st.ccsid }
}

/// One listing entry; `more` sets the chain indicator.
pub fn entry_frame(f: &Frame, name: &str, st: &Stat, restart: u32, more: bool) -> Vec<u8> {
    let mut t = u16::from(more).to_be_bytes().to_vec();
    t.extend(time(st.ctime));
    t.extend(time(st.mtime));
    t.extend(time(st.atime));
    t.extend(size32(st).to_be_bytes());
    t.extend(fixed(st).to_be_bytes());
    t.extend(object_type(st).to_be_bytes());
    t.extend([0u8; 10]);
    t.extend(file_id(st).to_be_bytes());
    t.extend([0u8; 3]);
    t.extend(ccsid(st).to_be_bytes());
    t.extend(500u16.to_be_bytes());
    t.extend(restart.to_be_bytes());
    t.extend(st.size.to_be_bytes());
    t.extend(0u16.to_be_bytes());
    t.push(u8::from(is_link(st)));
    t.resize(93, 0);
    reply::frame(
        &f.header,
        id::LIST_ATTRIBUTES_REPLY,
        t,
        vec![Param::new(cp::NAME, text::utf16(name))],
    )
}

/// The open reply; `action` is 1 opened, 4 created.
pub fn open_frame(f: &Frame, handle: u32, st: &Stat, action: u16) -> Vec<u8> {
    let mut t = 0u16.to_be_bytes().to_vec();
    t.extend(handle.to_be_bytes());
    t.extend(file_id(st).to_be_bytes());
    t.extend(1u32.to_be_bytes());
    t.extend(ccsid(st).to_be_bytes());
    t.extend(action.to_be_bytes());
    t.extend(time(st.ctime));
    t.extend(time(st.mtime));
    t.extend(time(st.atime));
    // The 32-bit size stays zero: the 64-bit one follows.
    t.extend(0u32.to_be_bytes());
    t.extend(fixed(st).to_be_bytes());
    t.extend([0u8; 12]);
    t.extend(file_id(st).to_be_bytes());
    t.extend([0u8; 3]);
    t.extend(st.size.to_be_bytes());
    reply::frame(&f.header, id::OPEN_REPLY, t, Vec::new())
}

/// OA1 (448 bytes): the creation time and the owner.
pub fn oa1(st: &Stat) -> Vec<u8> {
    let mut b = vec![0u8; 448];
    b[128..132].copy_from_slice(&0x800u32.to_be_bytes());
    b[136..140].copy_from_slice(&0x800u32.to_be_bytes());
    b[160..164].copy_from_slice(&0x1000_0000u32.to_be_bytes());
    b[180..188].copy_from_slice(&time(st.ctime));
    b[196..200].copy_from_slice(&4u32.to_be_bytes());
    b[224..234].copy_from_slice(&text::name10(&st.owner));
    b
}

/// OA2 (160 bytes): the times, mode, file ID, size, type and CCSID.
pub fn oa2(st: &Stat) -> Vec<u8> {
    let mut b = vec![0u8; 160];
    b[32..40].copy_from_slice(&time(st.atime));
    b[40..48].copy_from_slice(&time(st.mtime));
    b[48..56].copy_from_slice(&time(st.ctime));
    b[56..60].copy_from_slice(&st.mode.to_be_bytes());
    b[60..64].copy_from_slice(&0x8000u32.to_be_bytes());
    b[76..80].copy_from_slice(&file_id(st).to_be_bytes());
    b[80..88].copy_from_slice(&st.size.to_be_bytes());
    b[88..92].copy_from_slice(&0x8000u32.to_be_bytes());
    b[92..96].copy_from_slice(&st.nlink.max(1).to_be_bytes());
    b[108..112].copy_from_slice(&st.blksize.max(0x2000).to_be_bytes());
    let objtype = if st.objtype.is_empty() {
        if is_dir(st) { "*DIR" } else { "*STMF" }
    } else {
        st.objtype.as_str()
    };
    b[114..124].copy_from_slice(&text::name10(objtype));
    b[134..136].copy_from_slice(&ccsid(st).to_be_bytes());
    b[140..142].copy_from_slice(&0xFFFFu16.to_be_bytes());
    b[142..144].copy_from_slice(&ccsid(st).to_be_bytes());
    b[144..148].copy_from_slice(&0x8040u32.to_be_bytes());
    b[148..152].copy_from_slice(&1u32.to_be_bytes());
    b
}

/// The attributes of an open file as OA1 and OA2.
pub fn object_attrs_frame(f: &Frame, st: &Stat) -> Vec<u8> {
    reply::frame(
        &f.header,
        id::LIST_ATTRIBUTES_REPLY,
        vec![0; 8],
        vec![Param::new(cp::OA1, oa1(st)), Param::new(cp::OA2, oa2(st))],
    )
}
