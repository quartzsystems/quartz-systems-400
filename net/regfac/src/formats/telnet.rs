// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The telnet server's formats (`ETGDEVEX`): `INIT0100` when a session
//! starts and `TERM0100` when it ends. `INIT0100` passes the user
//! description and the device description, which the program may
//! change, the connection description, the environment options, and
//! two answers: allow the connection, allow automatic sign-on.

use super::{ParmList, Usage, parm};
use crate::spec::{Layout, field, read};
use crate::text;

/// The user description (`Qtg_User_Description`).
pub static USER_DESC: Layout = Layout {
    name: "QtgUserDesc",
    size: 44,
    fields: &[
        field("LEN", 0, 4),
        field("USRPRF", 4, 10),
        field("CURLIB", 14, 10),
        field("PGM", 24, 10),
        field("MENU", 34, 10),
    ],
};

/// The device description (`Qtg_Device_Description`); its attributes
/// (`DSPD0100`) follow.
pub static DEVICE_DESC: Layout = Layout {
    name: "QtgDeviceDesc",
    size: 28,
    fields: &[
        field("DEVNAME", 0, 10),
        field("DEVFMT", 10, 8),
        field("RSV", 18, 2),
        field("OFFATTR", 20, 4),
        field("LENATTR", 24, 4),
    ],
};

/// The display attributes (`DSPD0100`).
pub static DSPD0100: Layout = Layout {
    name: "DSPD0100",
    size: 12,
    fields: &[
        field("KBDID", 0, 3),
        field("RSV", 3, 1),
        field("CODEPAGE", 4, 4),
        field("CHARSET", 8, 4),
    ],
};

/// The connection description (`Qtg_Connection_Description`).
pub static CONN_DESC: Layout = Layout {
    name: "QtgConnDesc",
    size: 132,
    fields: &[
        field("LEN", 0, 4),
        field("IPADDR", 4, 20),
        field("SECPWD", 24, 1),
        field("WSTYPE", 25, 14),
        field("SSL", 39, 1),
        field("LCLADDR", 40, 20),
        field("CLTAUTH", 60, 1),
        field("RSV", 61, 3),
        field("CERTRC", 64, 4),
        field("OFFCERT", 68, 4),
        field("CERTLEN", 72, 4),
        field("IPV6ADDR", 76, 28),
        field("LCLIPV6", 104, 28),
    ],
};

/// The telnet layouts.
pub static LAYOUTS: &[&Layout] = &[&USER_DESC, &DEVICE_DESC, &DSPD0100, &CONN_DESC];

/// `INIT0100`.
pub static INIT0100: ParmList = ParmList {
    name: "INIT0100",
    parms: &[
        parm(1, Usage::InOut, "CHAR(*)", "User description information"),
        parm(2, Usage::InOut, "CHAR(*)", "Device description information"),
        parm(
            3,
            Usage::Input,
            "CHAR(*)",
            "Connection description information",
        ),
        parm(4, Usage::Input, "CHAR(*)", "Environment options"),
        parm(
            5,
            Usage::Input,
            "BINARY(4)",
            "Length of environment options",
        ),
        parm(6, Usage::Output, "CHAR(1)", "Allow connection"),
        parm(7, Usage::Output, "CHAR(1)", "Allow automatic sign-on"),
    ],
};

/// `TERM0100`.
pub static TERM0100: ParmList = ParmList {
    name: "TERM0100",
    parms: &[
        parm(1, Usage::Input, "CHAR(*)", "Device description information"),
        parm(
            2,
            Usage::Input,
            "CHAR(*)",
            "Connection description information",
        ),
    ],
};

/// The parameter lists of this module.
pub static LISTS: &[&ParmList] = &[&INIT0100, &TERM0100];

/// A session as the telnet server describes it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Session {
    /// User profile, current library, program, menu (from the client's
    /// automatic sign-on, blank otherwise).
    pub user: (String, String, String, String),
    /// The device name and its keyboard, code page and character set.
    pub device: String,
    /// Keyboard identifier.
    pub keyboard: String,
    /// Code page and character set.
    pub code_page: (i32, i32),
    /// The client's address, the local address, as text.
    pub addrs: (String, String),
    /// Workstation type (`IBM-3179-2` ...).
    pub ws_type: String,
    /// Whether a valid secure password substitute came, and TLS.
    pub secure: (bool, bool),
    /// Environment options as the client sent them.
    pub env: Vec<u8>,
}

fn yes(b: bool) -> char {
    if b { '1' } else { '0' }
}

fn n32(n: usize) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

/// The user description.
#[must_use]
pub fn user_desc(s: &Session) -> Vec<u8> {
    let mut r = USER_DESC.blank();
    r.bin4("LEN", n32(USER_DESC.size))
        .text("USRPRF", &s.user.0)
        .text("CURLIB", &s.user.1)
        .text("PGM", &s.user.2)
        .text("MENU", &s.user.3);
    r.into_bytes()
}

/// The device description with its `DSPD0100` attributes.
#[must_use]
pub fn device_desc(s: &Session) -> Vec<u8> {
    let mut a = DSPD0100.blank();
    a.text("KBDID", &s.keyboard)
        .bytes("RSV", &[])
        .bin4("CODEPAGE", s.code_page.0)
        .bin4("CHARSET", s.code_page.1);
    let mut r = DEVICE_DESC.blank();
    r.text("DEVNAME", &s.device)
        .text("DEVFMT", "DSPD0100")
        .bytes("RSV", &[])
        .bin4("OFFATTR", n32(DEVICE_DESC.size))
        .bin4("LENATTR", n32(DSPD0100.size));
    let mut b = r.into_bytes();
    b.extend(a.into_bytes());
    b
}

/// The connection description.
#[must_use]
pub fn conn_desc(s: &Session) -> Vec<u8> {
    let mut r = CONN_DESC.blank();
    r.bin4("LEN", n32(CONN_DESC.size))
        .text("IPADDR", &s.addrs.0)
        .ch("SECPWD", yes(s.secure.0))
        .text("WSTYPE", &s.ws_type)
        .ch("SSL", yes(s.secure.1))
        .text("LCLADDR", &s.addrs.1)
        .ch("CLTAUTH", '0')
        .bytes("RSV", &[])
        .bin4("CERTRC", 0)
        .bin4("OFFCERT", 0)
        .bin4("CERTLEN", 0)
        .bytes("IPV6ADDR", &[])
        .bytes("LCLIPV6", &[]);
    r.into_bytes()
}

/// `INIT0100`'s parameters; both answers start as `0`.
#[must_use]
pub fn init0100(s: &Session) -> Vec<Vec<u8>> {
    vec![
        user_desc(s),
        device_desc(s),
        conn_desc(s),
        s.env.clone(),
        n32(s.env.len()).to_be_bytes().to_vec(),
        text::fixed("0", 1),
        text::fixed("0", 1),
    ]
}

/// `TERM0100`'s parameters.
#[must_use]
pub fn term0100(s: &Session) -> Vec<Vec<u8>> {
    vec![device_desc(s), conn_desc(s)]
}

/// What an `INIT0100` program returned.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InitOut {
    /// Allow the connection.
    pub allow: bool,
    /// Sign on automatically as `user`.
    pub autosignon: bool,
    /// User profile, current library, program, menu as returned.
    pub user: (String, String, String, String),
    /// The device name as returned.
    pub device: String,
}

/// `INIT0100`'s answers in `parms`.
#[must_use]
pub fn init_out(parms: &[Vec<u8>]) -> InitOut {
    let get = |i: usize| parms.get(i).map_or(&[][..], Vec::as_slice);
    let u = get(0);
    let f = |n: &str| text::field(read(&USER_DESC, u, n));
    InitOut {
        allow: text::field(get(5)) == "1",
        autosignon: text::field(get(6)) == "1",
        user: (f("USRPRF"), f("CURLIB"), f("PGM"), f("MENU")),
        device: text::field(read(&DEVICE_DESC, get(1), "DEVNAME")),
    }
}
