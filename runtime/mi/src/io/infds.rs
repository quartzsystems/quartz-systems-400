// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The file information data structure: the positions the runtime keeps
//! current, as `(offset, length)` from 0. `docs/RPG.md` has the table;
//! `tools/conformance/tests/rpg_layouts.rs` compares the two.

/// The file name.
pub const FILE: (usize, usize) = (0, 8);
/// `1` while the file is open.
pub const OPEN: (usize, usize) = (8, 1);
/// `1` at end of file.
pub const EOF: (usize, usize) = (9, 1);
/// The status code, zoned.
pub const STATUS: (usize, usize) = (10, 5);
/// The operation code: five characters and `F` or `R`.
pub const OPCODE: (usize, usize) = (15, 6);
/// The routine running.
pub const ROUTINE: (usize, usize) = (21, 8);
/// The statement number.
pub const STMT: (usize, usize) = (29, 8);
/// The record format.
pub const RECORD: (usize, usize) = (37, 8);
/// The message identifier of an error.
pub const MSGID: (usize, usize) = (45, 7);
/// The open feedback's file name.
pub const ODP_FILE: (usize, usize) = (82, 10);
/// The open feedback's library.
pub const ODP_LIB: (usize, usize) = (92, 10);
/// The open feedback's record length, binary.
pub const RECLEN: (usize, usize) = (124, 2);
/// The member open.
pub const MEMBER: (usize, usize) = (128, 10);
/// The relative record number, binary.
pub const RRN: (usize, usize) = (396, 4);
/// The key of the last record read, as long as the key.
pub const KEY: (usize, usize) = (400, 0);

/// Every position, by the name the table in `docs/RPG.md` gives it.
pub const FIELDS: &[(&str, (usize, usize))] = &[
    ("file", FILE),
    ("open", OPEN),
    ("eof", EOF),
    ("status", STATUS),
    ("opcode", OPCODE),
    ("routine", ROUTINE),
    ("stmt", STMT),
    ("record", RECORD),
    ("msgid", MSGID),
    ("odpfile", ODP_FILE),
    ("odplib", ODP_LIB),
    ("reclen", RECLEN),
    ("member", MEMBER),
    ("rrn", RRN),
    ("key", KEY),
];
