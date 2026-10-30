// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The exit point formats of the shipped points: what a server hands
//! its exit programs and how it reads their answer. Structures are
//! [`Layout`]s and parameter lists [`ParmList`]s, both in the tables of
//! `docs/HOSTSVR.md`.

pub mod host;
pub mod tcp;
pub mod telnet;

use crate::spec::Layout;
use crate::text;

/// How a parameter is passed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Usage {
    /// Input.
    Input,
    /// Output.
    Output,
    /// Input and output.
    InOut,
}

impl Usage {
    /// As the tables write it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Input => "Input",
            Self::Output => "Output",
            Self::InOut => "I/O",
        }
    }
}

/// One parameter of a format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Parm {
    /// Its position, from 1.
    pub n: usize,
    /// Usage.
    pub usage: Usage,
    /// IBM's type (`BINARY(4)`, `CHAR(10)`, `CHAR(*)`).
    pub ty: &'static str,
    /// What it is.
    pub name: &'static str,
}

/// A parameter.
#[must_use]
pub const fn parm(n: usize, usage: Usage, ty: &'static str, name: &'static str) -> Parm {
    Parm { n, usage, ty, name }
}

/// A format's parameter list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParmList {
    /// Format name, or `HOST` for the host servers' two parameters.
    pub name: &'static str,
    /// The parameters in order.
    pub parms: &'static [Parm],
}

/// The host servers' two parameters, for every host format.
pub static HOST: ParmList = ParmList {
    name: "HOST",
    parms: &[
        parm(1, Usage::Output, "CHAR(1)", "Return code"),
        parm(2, Usage::Input, "CHAR(*)", "Exit point information"),
    ],
};

/// Every structure layout of the formats.
pub static LAYOUTS: &[&Layout] = &[
    &host::ZSOY0100,
    &host::ZSCL0100,
    &host::ZSCS0100,
    &host::ZSCN0100,
    &host::ZDAI0100,
    &host::ZDAD0100,
    &host::ZDAD0200,
    &host::ZDAR0100,
    &host::ZDAR0200,
    &host::ZDAQ0100,
    &host::ZDAQ0200,
    &host::ZHQ00100,
    &host::PWFS0100,
    &host::PWFS0200,
    &host::ENTR0100,
    &host::SPLF0100,
    &host::CZRC0100,
    &telnet::USER_DESC,
    &telnet::DEVICE_DESC,
    &telnet::DSPD0100,
    &telnet::CONN_DESC,
];

/// Every parameter list.
#[must_use]
pub fn lists() -> Vec<&'static ParmList> {
    let mut v: Vec<&'static ParmList> = vec![&HOST];
    v.extend(tcp::LISTS);
    v.extend(telnet::LISTS);
    v
}

/// The host server formats.
pub const HOST_FORMATS: &[&str] = &[
    "ZSOY0100", "ZSCL0100", "ZSCS0100", "ZSCN0100", "ZDAI0100", "ZDAD0100", "ZDAD0200", "ZDAR0100",
    "ZDAR0200", "ZDAQ0100", "ZDAQ0200", "ZHQ00100", "PWFS0100", "PWFS0200", "ENTR0100", "SPLF0100",
    "CZRC0100",
];

/// Every format this release can call exit programs with.
#[must_use]
pub fn known() -> Vec<&'static str> {
    let mut v = HOST_FORMATS.to_vec();
    v.extend([
        "VLRQ0100", "TCPL0100", "TCPL0200", "TCPL0300", "INIT0100", "TERM0100",
    ]);
    v
}

/// A request for the exit programs of a point, in one of the formats.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExitRequest {
    /// A host server format: its name and the structure the `host`
    /// builders made.
    Host {
        /// `ZSOY0100` ...
        format: String,
        /// The structure.
        info: Vec<u8>,
    },
    /// `VLRQ0100`.
    Validate(tcp::Validate),
    /// `TCPL0100`, `TCPL0200` or `TCPL0300`.
    Logon {
        /// The format.
        format: String,
        /// The sign-on.
        logon: tcp::Logon,
    },
    /// `INIT0100`.
    DevInit(telnet::Session),
    /// `TERM0100`.
    DevTerm(telnet::Session),
}

impl ExitRequest {
    /// A host server request.
    #[must_use]
    pub fn host(format: &str, info: Vec<u8>) -> Self {
        Self::Host {
            format: format.into(),
            info,
        }
    }

    /// The format name.
    #[must_use]
    pub fn format(&self) -> &str {
        match self {
            Self::Host { format, .. } | Self::Logon { format, .. } => format,
            Self::Validate(_) => "VLRQ0100",
            Self::DevInit(_) => "INIT0100",
            Self::DevTerm(_) => "TERM0100",
        }
    }

    /// The parameters an exit program is called with, as buffers it
    /// may change.
    #[must_use]
    pub fn parms(&self) -> Vec<Vec<u8>> {
        match self {
            // The return code starts blank: only `1` allows.
            Self::Host { info, .. } => vec![vec![0x40], info.clone()],
            Self::Validate(v) => tcp::vlrq0100(v),
            Self::Logon { format, logon } => tcp::tcpl(format, logon),
            Self::DevInit(s) => telnet::init0100(s),
            Self::DevTerm(s) => telnet::term0100(s),
        }
    }

    /// Whether the answer in `parms` allows the request.
    #[must_use]
    pub fn allowed(&self, parms: &[Vec<u8>]) -> bool {
        let get = |i: usize| parms.get(i).map_or(&[][..], Vec::as_slice);
        match self {
            Self::Host { .. } => get(0) == text::fixed("1", 1).as_slice(),
            Self::Validate(_) => text::bin4(get(7), 0) >= 1,
            Self::Logon { format, .. } => tcp::logon_out(format, parms).rc != 0,
            Self::DevInit(_) => telnet::init_out(parms).allow,
            Self::DevTerm(_) => true,
        }
    }

    /// The return code as the audit entry's one character.
    #[must_use]
    pub fn rc_char(&self, parms: &[Vec<u8>]) -> String {
        let get = |i: usize| parms.get(i).map_or(&[][..], Vec::as_slice);
        let digit = |v: i32| {
            char::from_digit(u32::try_from(v.clamp(0, 9)).unwrap_or(0), 10)
                .map_or_else(String::new, String::from)
        };
        match self {
            Self::Host { .. } => text::field(get(0)),
            Self::Validate(_) => digit(text::bin4(get(7), 0)),
            Self::Logon { format, .. } => digit(tcp::logon_out(format, parms).rc),
            Self::DevInit(_) => text::field(get(5)),
            Self::DevTerm(_) => String::new(),
        }
    }
}
