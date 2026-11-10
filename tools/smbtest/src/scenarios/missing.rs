// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Scenarios the client crate (smb 0.12.1) cannot express. They are listed
//! as `skip` lines, never faked; `tests/interop.rs` covers locks and printing
//! with Samba's client and the kernel's.

/// Name and why.
pub const MISSING: &[(&str, &str)] = &[
    (
        "lock-conflict",
        "smb 0.12.1 has no LOCK request (no byte-range lock API); covered by tests/interop.rs over the kernel CIFS client",
    ),
    (
        "lease-break",
        "smb 0.12.1 sends every CREATE with no lease context and cannot acknowledge a break",
    ),
    (
        "oplock-break",
        "smb 0.12.1 sends every CREATE with oplock level none",
    ),
    (
        "encrypt-each-cipher",
        "smb 0.12.1 offers all four ciphers in one negotiate and has no setting to offer one; the server's choice is printed by encrypt-3.1.1",
    ),
    (
        "print-write",
        "smb 0.12.1 panics on an open of a print share (`unimplemented!` for ShareType::Print); covered by tests/interop.rs with smbclient's print",
    ),
];
