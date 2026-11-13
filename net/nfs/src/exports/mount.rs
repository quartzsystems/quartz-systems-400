// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `MOUNT TYPE(*NFS) OPTIONS` (docs/NFS.md section 7) and the form of
//! `MFS`, `host:path`.

use super::error::{MfsError, OptError};
use super::list;

/// `OPTIONS(*DFT)`: IBM's default list.
pub const DFT: &str = "rw,suid,retry=5,rsize=32768,wsize=32768,timeo=20,retrans=5,acregmin=30,acregmax=60,acdirmin=30,acdirmax=60,hard";

/// The protocol version a mount asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NfsVers {
    /// `vers=3`.
    V3,
    /// `vers=4`: 4.1 when the server has it, else 4.0.
    V4,
    /// `vers=4.0`.
    V40,
    /// `vers=4.1`.
    V41,
}

/// The transport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Proto {
    /// `proto=tcp`.
    Tcp,
    /// `proto=udp`.
    Udp,
}

/// A mount's options.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // the options are flags
pub struct MountOptions {
    /// `ro`.
    pub read_only: bool,
    /// `suid` (`nosuid` false).
    pub suid: bool,
    /// `hard` (`soft` false).
    pub hard: bool,
    /// `intr`.
    pub intr: bool,
    /// `retry=`: how many times the mount itself is tried.
    pub retry: u32,
    /// `rsize=`, bytes.
    pub rsize: u32,
    /// `wsize=`, bytes.
    pub wsize: u32,
    /// `timeo=`, tenths of a second.
    pub timeo: u32,
    /// `retrans=`.
    pub retrans: u32,
    /// `acregmin=`, seconds.
    pub acregmin: u32,
    /// `acregmax=`, seconds.
    pub acregmax: u32,
    /// `acdirmin=`, seconds.
    pub acdirmin: u32,
    /// `acdirmax=`, seconds.
    pub acdirmax: u32,
    /// `noac`: no attribute cache.
    pub noac: bool,
    /// `vers=`.
    pub vers: NfsVers,
    /// `proto=`; `None` TCP, then UDP for version 3.
    pub proto: Option<Proto>,
    /// `port=`; `None` the binder's answer (2049 for version 4).
    pub port: Option<u16>,
    /// `mountport=`; `None` the binder's answer.
    pub mountport: Option<u16>,
}

impl Default for MountOptions {
    /// What [`DFT`] says.
    fn default() -> Self {
        Self {
            read_only: false,
            suid: true,
            hard: true,
            intr: false,
            retry: 5,
            rsize: 32768,
            wsize: 32768,
            timeo: 20,
            retrans: 5,
            acregmin: 30,
            acregmax: 60,
            acdirmin: 30,
            acdirmax: 60,
            noac: false,
            vers: NfsVers::V3,
            proto: None,
            port: None,
            mountport: None,
        }
    }
}

/// The ranges of the numeric options.
pub const RANGES: &[(&str, u32, u32)] = &[
    ("retry", 0, 10_000),
    ("rsize", 512, 1_048_576),
    ("wsize", 512, 1_048_576),
    ("timeo", 1, 36_000),
    ("retrans", 0, 10_000),
    ("acregmin", 0, 3600),
    ("acregmax", 0, 3600),
    ("acdirmin", 0, 3600),
    ("acdirmax", 0, 3600),
    ("actimeo", 0, 3600),
    ("port", 1, 65_535),
    ("mountport", 1, 65_535),
];

/// Parse `MOUNT OPTIONS`; `*DFT` is [`DFT`]. Options start from the
/// defaults; `rw`/`ro`, `suid`/`nosuid`, `hard`/`soft` and
/// `intr`/`nointr` are each one option given once.
#[allow(clippy::too_many_lines)] // one arm per option
pub fn parse(text: &str) -> Result<MountOptions, OptError> {
    let text = if text.trim().eq_ignore_ascii_case("*DFT") {
        DFT
    } else {
        text
    };
    let mut o = MountOptions::default();
    let mut seen: Vec<&str> = Vec::new();
    for it in list::split(text)? {
        let bad = || OptError::BadValue {
            name: it.name.clone(),
            value: it.value.unwrap_or("").to_string(),
        };
        // Opposites are one option.
        let key = match it.name.as_str() {
            "rw" | "ro" => "rw",
            "suid" | "nosuid" => "suid",
            "hard" | "soft" => "hard",
            "intr" | "nointr" => "intr",
            n => n,
        };
        let key: &'static str = match RANGES.iter().find(|(n, _, _)| *n == key) {
            Some((n, _, _)) => n,
            None => match key {
                "rw" => "rw",
                "suid" => "suid",
                "hard" => "hard",
                "intr" => "intr",
                "noac" => "noac",
                "vers" => "vers",
                "proto" => "proto",
                "sec" => "sec",
                // `bg` and anything else: not taken (docs/NFS.md
                // section 10).
                _ => return Err(OptError::Unknown { pos: it.pos }),
            },
        };
        if seen.contains(&key) {
            return Err(OptError::Duplicate {
                name: it.name.clone(),
            });
        }
        seen.push(key);
        let flag = |v: bool| -> Result<bool, OptError> {
            if it.value.is_some() {
                Err(bad())
            } else {
                Ok(v)
            }
        };
        match it.name.as_str() {
            "rw" => o.read_only = flag(false)?,
            "ro" => o.read_only = flag(true)?,
            "suid" => o.suid = flag(true)?,
            "nosuid" => o.suid = flag(false)?,
            "hard" => o.hard = flag(true)?,
            "soft" => o.hard = flag(false)?,
            "intr" => o.intr = flag(true)?,
            "nointr" => o.intr = flag(false)?,
            "noac" => o.noac = flag(true)?,
            "vers" => {
                o.vers = match it.value.ok_or_else(bad)? {
                    "3" => NfsVers::V3,
                    "4" => NfsVers::V4,
                    "4.0" => NfsVers::V40,
                    "4.1" => NfsVers::V41,
                    _ => return Err(bad()),
                }
            }
            "proto" => {
                o.proto = match it.value.map(str::to_ascii_lowercase).as_deref() {
                    Some("tcp") => Some(Proto::Tcp),
                    Some("udp") => Some(Proto::Udp),
                    _ => return Err(bad()),
                }
            }
            "sec" => {
                if !it.value.is_some_and(|v| v.eq_ignore_ascii_case("sys")) {
                    return Err(bad());
                }
            }
            name => {
                let (_, lo, hi) = RANGES
                    .iter()
                    .find(|(n, _, _)| *n == name)
                    .copied()
                    .ok_or(OptError::Unknown { pos: it.pos })?;
                let v: u32 = it.value.ok_or_else(bad)?.parse().map_err(|_| bad())?;
                if !(lo..=hi).contains(&v) {
                    return Err(bad());
                }
                let port = || u16::try_from(v).ok();
                match name {
                    "retry" => o.retry = v,
                    "rsize" => o.rsize = v,
                    "wsize" => o.wsize = v,
                    "timeo" => o.timeo = v,
                    "retrans" => o.retrans = v,
                    "acregmin" => o.acregmin = v,
                    "acregmax" => o.acregmax = v,
                    "acdirmin" => o.acdirmin = v,
                    "acdirmax" => o.acdirmax = v,
                    "actimeo" => {
                        o.acregmin = v;
                        o.acregmax = v;
                        o.acdirmin = v;
                        o.acdirmax = v;
                    }
                    "port" => o.port = port(),
                    _ => o.mountport = port(),
                }
            }
        }
    }
    // Version 4 runs over TCP only.
    if o.proto == Some(Proto::Udp) && o.vers != NfsVers::V3 {
        return Err(OptError::BadValue {
            name: "proto".into(),
            value: "udp".into(),
        });
    }
    Ok(o)
}

/// `MFS` for `TYPE(*NFS)`: `host:path`, the host a name, an IPv4
/// address or a bracketed IPv6 address, the path absolute.
pub fn parse_mfs(mfs: &str) -> Result<(String, String), MfsError> {
    let bad = || MfsError(mfs.to_string());
    let t = mfs.trim();
    let (host, path) = if let Some(rest) = t.strip_prefix('[') {
        let close = rest.find(']').ok_or_else(bad)?;
        let path = rest[close + 1..].strip_prefix(':').ok_or_else(bad)?;
        (rest[..close].to_string(), path)
    } else {
        let (h, p) = t.split_once(':').ok_or_else(bad)?;
        (h.to_string(), p)
    };
    if host.is_empty() || !path.starts_with('/') || host.contains(char::is_whitespace) {
        return Err(bad());
    }
    Ok((host, path.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_ibms_list() {
        assert_eq!(parse("*DFT").unwrap(), MountOptions::default());
        assert_eq!(parse(DFT).unwrap(), MountOptions::default());
        assert_eq!(parse("").unwrap(), MountOptions::default());
    }

    #[test]
    fn options() {
        let o = parse("ro,nosuid,soft,intr,vers=4.1,proto=tcp,port=2049,actimeo=10,noac,sec=sys")
            .unwrap();
        assert!(o.read_only && !o.suid && !o.hard && o.intr && o.noac);
        assert_eq!(o.vers, NfsVers::V41);
        assert_eq!(o.port, Some(2049));
        assert_eq!((o.acregmin, o.acdirmax), (10, 10));
        let o = parse("rsize=1048576,wsize=512,mountport=635,proto=UDP").unwrap();
        assert_eq!((o.rsize, o.wsize, o.mountport), (1_048_576, 512, Some(635)));
    }

    #[test]
    fn refusals() {
        let id = |s: &str| parse(s).unwrap_err().msgid();
        assert_eq!(parse("rw,bg"), Err(OptError::Unknown { pos: 4 }));
        assert_eq!(id("fg"), "CPFA1B0");
        assert_eq!(id("rw,ro"), "CPFA1B1");
        assert_eq!(id("hard,hard"), "CPFA1B1");
        assert_eq!(id("vers=2"), "CPFA1BB");
        assert_eq!(id("rsize=100"), "CPFA1BB");
        assert_eq!(id("timeo=0"), "CPFA1BB");
        assert_eq!(id("sec=krb5"), "CPFA1BB");
        assert_eq!(id("ro=1"), "CPFA1BB");
        assert_eq!(id("vers=4,proto=udp"), "CPFA1BB");
        assert_eq!(id("port=0"), "CPFA1BB");
        assert_eq!(
            parse("retry=x"),
            Err(OptError::BadValue {
                name: "retry".into(),
                value: "x".into()
            })
        );
    }

    #[test]
    fn mfs() {
        assert_eq!(
            parse_mfs("srv:/export/a").unwrap(),
            ("srv".into(), "/export/a".into())
        );
        assert_eq!(parse_mfs("10.1.1.1:/x").unwrap().0, "10.1.1.1");
        assert_eq!(
            parse_mfs("[fd00::1]:/x").unwrap(),
            ("fd00::1".into(), "/x".into())
        );
        for bad in [
            "/dev/QASP01/a.udfs",
            "srv:",
            "srv:rel",
            ":/x",
            "[fd00::1]/x",
        ] {
            assert!(parse_mfs(bad).is_err(), "{bad}");
        }
    }
}
