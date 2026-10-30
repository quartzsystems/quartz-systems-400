// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The TCP/IP applications' formats: `VLRQ0100` (request validation
//! for the FTP client and server, REXEC and TFTP) and `TCPL0100`,
//! `TCPL0200`, `TCPL0300` (server sign-on). These are parameter lists,
//! not structures.

use super::{ParmList, Usage, parm};
use crate::text;

/// Application identifiers of `VLRQ0100` and `TCPL*`.
pub mod app {
    /// The FTP client.
    pub const FTP_CLIENT: i32 = 0;
    /// The FTP server.
    pub const FTP_SERVER: i32 = 1;
    /// The REXEC server.
    pub const REXEC_SERVER: i32 = 2;
    /// The TFTP server.
    pub const TFTP_SERVER: i32 = 3;
}

/// FTP operation identifiers of `VLRQ0100`.
pub mod op {
    /// Session start.
    pub const SESSION: i32 = 0;
    /// Create a directory or library.
    pub const CREATE_DIR: i32 = 1;
    /// Delete a directory or library.
    pub const DELETE_DIR: i32 = 2;
    /// Set the current directory or library.
    pub const SET_DIR: i32 = 3;
    /// List files.
    pub const LIST: i32 = 4;
    /// Delete a file.
    pub const DELETE_FILE: i32 = 5;
    /// Send a file to the client.
    pub const SEND_FILE: i32 = 6;
    /// Receive a file from the client.
    pub const RECEIVE_FILE: i32 = 7;
    /// Rename a file.
    pub const RENAME: i32 = 8;
    /// Run a CL command.
    pub const COMMAND: i32 = 9;
}

/// `VLRQ0100`.
pub static VLRQ0100: ParmList = ParmList {
    name: "VLRQ0100",
    parms: &[
        parm(1, Usage::Input, "BINARY(4)", "Application identifier"),
        parm(2, Usage::Input, "BINARY(4)", "Operation identifier"),
        parm(3, Usage::Input, "CHAR(10)", "User profile"),
        parm(4, Usage::Input, "CHAR(*)", "Remote IP address"),
        parm(5, Usage::Input, "BINARY(4)", "Length of remote IP address"),
        parm(6, Usage::Input, "CHAR(*)", "Operation-specific information"),
        parm(
            7,
            Usage::Input,
            "BINARY(4)",
            "Length of operation-specific information",
        ),
        parm(8, Usage::Output, "BINARY(4)", "Allow operation"),
    ],
};

/// `TCPL0100`.
pub static TCPL0100: ParmList = ParmList {
    name: "TCPL0100",
    parms: &[
        parm(1, Usage::Input, "BINARY(4)", "Application identifier"),
        parm(2, Usage::Input, "CHAR(*)", "User identifier"),
        parm(3, Usage::Input, "BINARY(4)", "Length of user identifier"),
        parm(4, Usage::Input, "CHAR(*)", "Authentication string"),
        parm(
            5,
            Usage::Input,
            "BINARY(4)",
            "Length of authentication string",
        ),
        parm(6, Usage::Input, "CHAR(*)", "Client IP address"),
        parm(7, Usage::Input, "BINARY(4)", "Length of client IP address"),
        parm(8, Usage::Output, "BINARY(4)", "Return code"),
        parm(9, Usage::Output, "CHAR(10)", "User profile"),
        parm(10, Usage::Output, "CHAR(10)", "Password"),
        parm(11, Usage::Output, "CHAR(10)", "Initial current library"),
    ],
};

/// `TCPL0200`.
pub static TCPL0200: ParmList = ParmList {
    name: "TCPL0200",
    parms: &[
        parm(1, Usage::Input, "BINARY(4)", "Application identifier"),
        parm(2, Usage::Input, "CHAR(*)", "User identifier"),
        parm(3, Usage::Input, "BINARY(4)", "Length of user identifier"),
        parm(4, Usage::Input, "CHAR(*)", "Authentication string"),
        parm(
            5,
            Usage::Input,
            "BINARY(4)",
            "Length of authentication string",
        ),
        parm(6, Usage::Input, "CHAR(*)", "Client IP address"),
        parm(7, Usage::Input, "BINARY(4)", "Length of client IP address"),
        parm(8, Usage::Output, "BINARY(4)", "Return code"),
        parm(9, Usage::Output, "CHAR(10)", "User profile"),
        parm(10, Usage::Output, "CHAR(10)", "Password"),
        parm(11, Usage::Output, "CHAR(10)", "Initial current library"),
        parm(12, Usage::Output, "CHAR(*)", "Initial home directory"),
        parm(
            13,
            Usage::InOut,
            "BINARY(4)",
            "Length of initial home directory",
        ),
        parm(
            14,
            Usage::InOut,
            "CHAR(*)",
            "Application-specific information",
        ),
        parm(
            15,
            Usage::Input,
            "BINARY(4)",
            "Length of application-specific information",
        ),
    ],
};

/// `TCPL0300`.
pub static TCPL0300: ParmList = ParmList {
    name: "TCPL0300",
    parms: &[
        parm(1, Usage::Input, "BINARY(4)", "Application identifier"),
        parm(2, Usage::Input, "CHAR(*)", "User identifier"),
        parm(3, Usage::Input, "BINARY(4)", "Length of user identifier"),
        parm(4, Usage::Input, "CHAR(*)", "Authentication string"),
        parm(
            5,
            Usage::Input,
            "BINARY(4)",
            "Length of authentication string",
        ),
        parm(
            6,
            Usage::Input,
            "BINARY(4)",
            "CCSID of authentication string",
        ),
        parm(7, Usage::Input, "CHAR(*)", "Client IP address"),
        parm(8, Usage::Input, "BINARY(4)", "Length of client IP address"),
        parm(9, Usage::Output, "BINARY(4)", "Return code"),
        parm(10, Usage::Output, "CHAR(10)", "User profile"),
        parm(11, Usage::Output, "CHAR(*)", "Password"),
        parm(12, Usage::InOut, "BINARY(4)", "Length of password"),
        parm(13, Usage::Output, "BINARY(4)", "CCSID of password"),
        parm(14, Usage::Output, "CHAR(10)", "Initial current library"),
        parm(15, Usage::Output, "CHAR(*)", "Initial home directory"),
        parm(
            16,
            Usage::InOut,
            "BINARY(4)",
            "Length of initial home directory",
        ),
        parm(
            17,
            Usage::InOut,
            "BINARY(4)",
            "CCSID of initial home directory",
        ),
        parm(
            18,
            Usage::InOut,
            "CHAR(*)",
            "Application-specific information",
        ),
        parm(
            19,
            Usage::Input,
            "BINARY(4)",
            "Length of application-specific information",
        ),
    ],
};

/// The parameter lists of this module.
pub static LISTS: &[&ParmList] = &[&VLRQ0100, &TCPL0100, &TCPL0200, &TCPL0300];

/// Room the exit program gets for a home directory or password it
/// returns.
pub const OUT_ROOM: usize = 1024;

fn b4(v: i32) -> Vec<u8> {
    v.to_be_bytes().to_vec()
}

fn n32(n: usize) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

/// A request to validate (`VLRQ0100`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Validate {
    /// `app`.
    pub app: i32,
    /// `op`.
    pub op: i32,
    /// The user profile of the session.
    pub user: String,
    /// The remote IP address, as text.
    pub remote: String,
    /// The operation's information (a file name, a command ...), in
    /// the job's CCSID.
    pub info: Vec<u8>,
}

/// `VLRQ0100`'s parameters; the allow operation starts at 0 (reject).
#[must_use]
pub fn vlrq0100(v: &Validate) -> Vec<Vec<u8>> {
    let ip = text::ebcdic(&v.remote);
    vec![
        b4(v.app),
        b4(v.op),
        text::fixed(&v.user, 10),
        ip.clone(),
        b4(n32(ip.len())),
        v.info.clone(),
        b4(n32(v.info.len())),
        b4(0),
    ]
}

/// A server sign-on (`TCPL0100`, `TCPL0200`, `TCPL0300`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Logon {
    /// `app`.
    pub app: i32,
    /// The user identifier the client sent.
    pub user_id: String,
    /// The authentication string (password), as sent.
    pub auth: Vec<u8>,
    /// Its CCSID (`TCPL0300`).
    pub auth_ccsid: i32,
    /// The client's IP address, as text.
    pub client: String,
    /// Application-specific information (`TCPL0200`, `TCPL0300`).
    pub app_info: Vec<u8>,
}

/// What a sign-on exit program returned.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LogonOut {
    /// The return code: 0 rejects; the others say how the session
    /// signs on (IBM's table).
    pub rc: i32,
    /// User profile.
    pub user: String,
    /// Password as returned, trailing blanks dropped (CP037 but for
    /// `TCPL0300`, where `password_ccsid` says).
    pub password: Vec<u8>,
    /// The password's CCSID (`TCPL0300`).
    pub password_ccsid: i32,
    /// Initial current library.
    pub curlib: String,
    /// Initial home directory (`TCPL0200`, `TCPL0300`).
    pub home: String,
}

/// The sign-on parameters for `format`.
#[must_use]
pub fn tcpl(format: &str, l: &Logon) -> Vec<Vec<u8>> {
    let uid = text::ebcdic(&l.user_id);
    let ip = text::ebcdic(&l.client);
    let blank10 = vec![0x40u8; 10];
    match format {
        "TCPL0100" => vec![
            b4(l.app),
            uid.clone(),
            b4(n32(uid.len())),
            l.auth.clone(),
            b4(n32(l.auth.len())),
            ip.clone(),
            b4(n32(ip.len())),
            b4(0),
            blank10.clone(),
            blank10.clone(),
            blank10,
        ],
        "TCPL0200" => vec![
            b4(l.app),
            uid.clone(),
            b4(n32(uid.len())),
            l.auth.clone(),
            b4(n32(l.auth.len())),
            ip.clone(),
            b4(n32(ip.len())),
            b4(0),
            blank10.clone(),
            blank10.clone(),
            blank10,
            vec![0x40; OUT_ROOM],
            b4(n32(OUT_ROOM)),
            l.app_info.clone(),
            b4(n32(l.app_info.len())),
        ],
        _ => vec![
            b4(l.app),
            uid.clone(),
            b4(n32(uid.len())),
            l.auth.clone(),
            b4(n32(l.auth.len())),
            b4(l.auth_ccsid),
            ip.clone(),
            b4(n32(ip.len())),
            b4(0),
            blank10.clone(),
            vec![0x40; OUT_ROOM],
            b4(n32(OUT_ROOM)),
            b4(0),
            blank10,
            vec![0x40; OUT_ROOM],
            b4(n32(OUT_ROOM)),
            b4(0),
            l.app_info.clone(),
            b4(n32(l.app_info.len())),
        ],
    }
}

fn trimmed(b: &[u8]) -> Vec<u8> {
    let n = b.iter().rposition(|&c| c != 0x40).map_or(0, |i| i + 1);
    b[..n].to_vec()
}

fn cut(b: &[u8], len: &[u8]) -> Vec<u8> {
    let n = usize::try_from(text::bin4(len, 0))
        .unwrap_or(0)
        .min(b.len());
    b[..n].to_vec()
}

/// What the sign-on program returned in `parms` (as `tcpl` laid them out).
#[must_use]
pub fn logon_out(format: &str, parms: &[Vec<u8>]) -> LogonOut {
    let get = |i: usize| parms.get(i).map_or(&[][..], Vec::as_slice);
    match format {
        "TCPL0100" => LogonOut {
            rc: text::bin4(get(7), 0),
            user: text::field(get(8)),
            password: trimmed(get(9)),
            password_ccsid: 0,
            curlib: text::field(get(10)),
            home: String::new(),
        },
        "TCPL0200" => LogonOut {
            rc: text::bin4(get(7), 0),
            user: text::field(get(8)),
            password: trimmed(get(9)),
            password_ccsid: 0,
            curlib: text::field(get(10)),
            home: text::field(&cut(get(11), get(12))),
        },
        _ => LogonOut {
            rc: text::bin4(get(8), 0),
            user: text::field(get(9)),
            password: cut(get(10), get(11)),
            password_ccsid: text::bin4(get(12), 0),
            curlib: text::field(get(13)),
            home: text::field(&cut(get(14), get(15))),
        },
    }
}
