// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The configuration's tables and the rules the commands apply to them.

pub mod attr;
pub mod dmn;
pub mod host;
pub mod ifc;
pub mod port;
pub mod rte;
pub mod svr;
pub mod tables;

use std::fmt;

use crate::addr::bad;
use crate::error::Result;

/// `TOS`: the type of service.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Tos {
    /// `*NORMAL`.
    #[default]
    Normal,
    /// `*MINDELAY`.
    MinDelay,
    /// `*MAXTHRPUT`.
    MaxThrput,
    /// `*MAXRLB`.
    MaxRlb,
    /// `*MINCOST`.
    MinCost,
}

impl Tos {
    /// From the special value or the number the definition maps it to.
    pub fn parse(s: &str) -> Result<Self> {
        Ok(match crate::addr::unquote(s).to_ascii_uppercase().as_str() {
            "" | "*NORMAL" | "6" => Self::Normal,
            "*MINDELAY" | "2" => Self::MinDelay,
            "*MAXTHRPUT" | "3" => Self::MaxThrput,
            "*MAXRLB" | "4" => Self::MaxRlb,
            "*MINCOST" | "5" => Self::MinCost,
            other => return Err(bad("TOS", other)),
        })
    }

    /// The special value.
    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "*NORMAL",
            Self::MinDelay => "*MINDELAY",
            Self::MaxThrput => "*MAXTHRPUT",
            Self::MaxRlb => "*MAXRLB",
            Self::MinCost => "*MINCOST",
        }
    }

    /// The IP header's TOS bits (RFC 1349).
    pub fn bits(self) -> u8 {
        match self {
            Self::Normal => 0,
            Self::MinDelay => 0x10,
            Self::MaxThrput => 0x08,
            Self::MaxRlb => 0x04,
            Self::MinCost => 0x02,
        }
    }
}

impl fmt::Display for Tos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// `LIND`: the line an interface runs over.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LineRef {
    /// A line description.
    Line(String),
    /// `*LOOPBACK`.
    Loopback,
    /// `*VIRTUALIP`: an address on no physical line.
    VirtualIp,
    /// `*OPC`: `OptiConnect` (stored, never started).
    Opc,
}

impl LineRef {
    /// From the command's text.
    pub fn parse(s: &str) -> Result<Self> {
        let t = crate::addr::unquote(s).to_ascii_uppercase();
        Ok(match t.as_str() {
            "*LOOPBACK" => Self::Loopback,
            "*VIRTUALIP" => Self::VirtualIp,
            "*OPC" => Self::Opc,
            "" => return Err(bad("LIND", "")),
            n if n.starts_with('*') => return Err(bad("LIND", n)),
            n => {
                if n.len() > 10 {
                    return Err(bad("LIND", n));
                }
                Self::Line(n.to_string())
            }
        })
    }

    /// The name as the panels show it.
    pub fn name(&self) -> &str {
        match self {
            Self::Line(n) => n,
            Self::Loopback => "*LOOPBACK",
            Self::VirtualIp => "*VIRTUALIP",
            Self::Opc => "*OPC",
        }
    }
}

impl fmt::Display for LineRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A transport protocol.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Proto {
    /// `*TCP`.
    #[default]
    Tcp,
    /// `*UDP`.
    Udp,
}

impl Proto {
    /// From `*TCP`/`*UDP` (or `TCP`, `tcp`).
    pub fn parse(s: &str) -> Result<Self> {
        match crate::addr::unquote(s).trim_start_matches('*').to_ascii_uppercase().as_str() {
            "TCP" => Ok(Self::Tcp),
            "UDP" => Ok(Self::Udp),
            other => Err(bad("PROTOCOL", other)),
        }
    }

    /// `*TCP` or `*UDP`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Tcp => "*TCP",
            Self::Udp => "*UDP",
        }
    }
}

/// `*YES`/`*NO` and the one-letter forms definitions map them to.
pub fn yes_no(s: &str, kwd: &str) -> Result<bool> {
    match crate::addr::unquote(s).to_ascii_uppercase().as_str() {
        "*YES" | "Y" | "1" => Ok(true),
        "*NO" | "N" | "0" => Ok(false),
        other => Err(bad(kwd, other)),
    }
}

/// `*YES` or `*NO`.
pub fn yes(b: bool) -> &'static str {
    if b { "*YES" } else { "*NO" }
}

/// Whether a value means "keep what is there".
pub fn is_same(s: &str) -> bool {
    let t = crate::addr::unquote(s).to_ascii_uppercase();
    t.is_empty() || t == "*SAME" || t == "S"
}

/// A `TEXT` value: `*BLANK` and nothing are empty.
pub fn text_of(s: &str) -> String {
    let t = s.trim();
    let t = t.strip_prefix('\'').and_then(|x| x.strip_suffix('\'')).unwrap_or(t);
    if t.eq_ignore_ascii_case("*BLANK") {
        String::new()
    } else {
        t.replace("''", "'").trim_end().to_string()
    }
}
