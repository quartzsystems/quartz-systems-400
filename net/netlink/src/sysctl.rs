// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/proc/sys/net` and the `CHGTCPA` parameters that map onto it.
//! Parameters without a kernel equivalent are stored by TCP/IP
//! configuration and have no entry here.

use std::fs;
use std::path::{Path, PathBuf};

use crate::{Error, Result};

/// How a `CHGTCPA` value becomes a sysctl value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    /// The same number.
    Same,
    /// IBM minutes to kernel seconds.
    MinutesToSeconds,
    /// IBM seconds to kernel milliseconds.
    SecondsToMillis,
    /// `*YES`/`*NO` as 1/0.
    YesNo,
    /// The middle value of a `min default max` triple.
    TripleDefault,
}

/// One `CHGTCPA` parameter with a kernel equivalent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mapping {
    /// The `CHGTCPA` keyword.
    pub param: &'static str,
    /// Paths under `/proc/sys/net` written with the value.
    pub paths: &'static [&'static str],
    /// The conversion.
    pub unit: Unit,
}

/// The `CHGTCPA` parameters that have a kernel equivalent.
pub const CHGTCPA: &[Mapping] = &[
    Mapping {
        param: "TCPKEEPALV",
        paths: &["ipv4/tcp_keepalive_time"],
        unit: Unit::MinutesToSeconds,
    },
    Mapping {
        param: "TCPURGPTR",
        paths: &["ipv4/tcp_stdurg"],
        unit: Unit::YesNo,
    },
    Mapping {
        param: "TCPRCVBUF",
        paths: &["ipv4/tcp_rmem"],
        unit: Unit::TripleDefault,
    },
    Mapping {
        param: "TCPSNDBUF",
        paths: &["ipv4/tcp_wmem"],
        unit: Unit::TripleDefault,
    },
    Mapping {
        param: "TCPR1CNT",
        paths: &["ipv4/tcp_retries1"],
        unit: Unit::Same,
    },
    Mapping {
        param: "TCPR2CNT",
        paths: &["ipv4/tcp_retries2"],
        unit: Unit::Same,
    },
    Mapping {
        param: "TCPCLOTIMO",
        paths: &["ipv4/tcp_fin_timeout"],
        unit: Unit::Same,
    },
    Mapping {
        param: "IPDTGFWD",
        paths: &["ipv4/ip_forward", "ipv6/conf/all/forwarding"],
        unit: Unit::YesNo,
    },
    Mapping {
        param: "IPSRCRTG",
        paths: &["ipv4/conf/all/accept_source_route"],
        unit: Unit::YesNo,
    },
    Mapping {
        param: "IPRSBTIMO",
        paths: &["ipv4/ipfrag_time", "ipv6/ip6frag_time"],
        unit: Unit::Same,
    },
    Mapping {
        param: "IPTTL",
        paths: &["ipv4/ip_default_ttl", "ipv6/conf/default/hop_limit"],
        unit: Unit::Same,
    },
    Mapping {
        param: "ARPTIMO",
        paths: &["ipv4/neigh/default/gc_stale_time"],
        unit: Unit::MinutesToSeconds,
    },
    Mapping {
        param: "ECN",
        paths: &["ipv4/tcp_ecn"],
        unit: Unit::YesNo,
    },
];

/// The mapping for a `CHGTCPA` keyword.
pub fn mapping(param: &str) -> Option<&'static Mapping> {
    CHGTCPA.iter().find(|m| m.param.eq_ignore_ascii_case(param))
}

/// Sysctls under a root (`/proc/sys/net` on the system, a directory in
/// tests).
#[derive(Clone, Debug)]
pub struct Sysctl {
    root: PathBuf,
}

impl Default for Sysctl {
    fn default() -> Self {
        Self::at("/proc/sys/net")
    }
}

impl Sysctl {
    /// Sysctls under `root`.
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn path(&self, key: &str) -> Result<PathBuf> {
        if key.is_empty() || key.split('/').any(|c| c.is_empty() || c == "." || c == "..") {
            return Err(Error::Invalid(format!("sysctl key {key}")));
        }
        Ok(self.root.join(key))
    }

    /// The value of `key` (`ipv4/ip_forward`), trimmed.
    pub fn read(&self, key: &str) -> Result<String> {
        Ok(fs::read_to_string(self.path(key)?)?.trim().to_string())
    }

    /// The value of `key` as a number.
    pub fn read_u64(&self, key: &str) -> Result<u64> {
        let v = self.read(key)?;
        v.parse()
            .map_err(|_| Error::Invalid(format!("{key} is not a number: {v}")))
    }

    /// Write `value` to `key`.
    pub fn write(&self, key: &str, value: &str) -> Result<()> {
        Ok(fs::write(self.path(key)?, format!("{value}\n"))?)
    }

    /// Apply a `CHGTCPA` value through its mapping to every path.
    pub fn apply(&self, m: &Mapping, value: u64) -> Result<()> {
        for key in m.paths {
            let text = match m.unit {
                Unit::Same => value.to_string(),
                Unit::MinutesToSeconds => (value * 60).to_string(),
                Unit::SecondsToMillis => (value * 1000).to_string(),
                Unit::YesNo => u64::from(value != 0).to_string(),
                Unit::TripleDefault => {
                    let cur = self.read(key)?;
                    let f: Vec<&str> = cur.split_whitespace().collect();
                    if f.len() != 3 {
                        return Err(Error::Invalid(format!("{key} is not a triple: {cur}")));
                    }
                    format!("{}\t{value}\t{}", f[0], f[2])
                }
            };
            self.write(key, &text)?;
        }
        Ok(())
    }

    /// The current `CHGTCPA` value through its mapping (first path).
    pub fn current(&self, m: &Mapping) -> Result<u64> {
        let key = m.paths[0];
        Ok(match m.unit {
            Unit::Same | Unit::YesNo => self.read_u64(key)?,
            Unit::MinutesToSeconds => self.read_u64(key)? / 60,
            Unit::SecondsToMillis => self.read_u64(key)? / 1000,
            Unit::TripleDefault => {
                let cur = self.read(key)?;
                cur.split_whitespace()
                    .nth(1)
                    .and_then(|v| v.parse().ok())
                    .ok_or_else(|| Error::Invalid(format!("{key} is not a triple: {cur}")))?
            }
        })
    }

    /// The root.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_and_reads_back_through_mappings() {
        let dir = std::env::temp_dir().join(format!("qs400-sysctl-{}", std::process::id()));
        fs::create_dir_all(dir.join("ipv4")).unwrap();
        fs::create_dir_all(dir.join("ipv6/conf/all")).unwrap();
        fs::write(dir.join("ipv4/tcp_rmem"), "4096\t131072\t6291456\n").unwrap();
        let s = Sysctl::at(&dir);

        s.apply(mapping("tcprcvbuf").unwrap(), 65536).unwrap();
        assert_eq!(s.read("ipv4/tcp_rmem").unwrap(), "4096\t65536\t6291456");
        assert_eq!(s.current(mapping("TCPRCVBUF").unwrap()).unwrap(), 65536);

        s.apply(mapping("TCPKEEPALV").unwrap(), 120).unwrap();
        assert_eq!(s.read_u64("ipv4/tcp_keepalive_time").unwrap(), 7200);
        assert_eq!(s.current(mapping("TCPKEEPALV").unwrap()).unwrap(), 120);

        s.apply(mapping("IPDTGFWD").unwrap(), 1).unwrap();
        assert_eq!(s.read("ipv6/conf/all/forwarding").unwrap(), "1");

        assert!(s.read("../etc/passwd").is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
