// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `MOUNT OPTIONS` as the client works with them: the retry policy, and
//! what the IFS is told ([`RemoteInfo`]) with the options in effect as
//! shown.

use std::time::Duration;

use ifs::remote::{AttrTimes, RemoteInfo};

use crate::exports::mount::{MountOptions, NfsVers};
use crate::rpc::Proto;

/// The longest wait for one attempt (the timeout doubles up to it).
pub const MAX_WAIT: Duration = Duration::from_secs(60);

/// How calls are retried (`timeo`, `retrans`, `soft`/`hard`, `intr`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    /// The first attempt's wait.
    pub timeo: Duration,
    /// Retransmissions in one major timeout.
    pub retrans: u32,
    /// Give up after a major timeout (`soft`); else try for ever.
    pub soft: bool,
    /// A waiting call may be interrupted.
    pub intr: bool,
}

impl Policy {
    /// The policy of `o`.
    #[must_use]
    pub fn of(o: &MountOptions) -> Self {
        Self {
            timeo: Duration::from_millis(u64::from(o.timeo) * 100),
            retrans: o.retrans,
            soft: !o.hard,
            intr: o.intr,
        }
    }

    /// The wait of attempt `n` (0 the first): `timeo` doubling to 60 s.
    #[must_use]
    pub fn wait(&self, n: u32) -> Duration {
        let f = 1u32.checked_shl(n.min(20)).unwrap_or(u32::MAX);
        self.timeo.saturating_mul(f).min(MAX_WAIT)
    }

    /// One major timeout: the first send and `retrans` retransmissions.
    #[must_use]
    pub fn major(&self) -> Duration {
        (0..=self.retrans.min(64))
            .map(|n| self.wait(n))
            .fold(Duration::ZERO, Duration::saturating_add)
    }
}

/// The protocol version a mount settled on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
    /// NFS version 3.
    V3,
    /// NFS version 4.0.
    V40,
    /// NFS version 4.1.
    V41,
}

impl Version {
    /// `/etc/vfs`'s name.
    #[must_use]
    pub fn vfs_name(self) -> &'static str {
        match self {
            Self::V3 => "nfs3",
            Self::V40 | Self::V41 => "nfs4",
        }
    }

    /// As `vers=` writes it.
    #[must_use]
    pub fn text(self) -> &'static str {
        match self {
            Self::V3 => "3",
            Self::V40 => "4.0",
            Self::V41 => "4.1",
        }
    }
}

/// What the mount settled on, beside the options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settled {
    /// The version.
    pub version: Version,
    /// The transport.
    pub proto: Proto,
    /// The NFS port.
    pub port: u16,
    /// The MOUNT port (version 3).
    pub mountport: Option<u16>,
    /// Bytes per read.
    pub rsize: u32,
    /// Bytes per write.
    pub wsize: u32,
    /// The server's names compare exactly.
    pub case_sensitive: bool,
    /// Byte-range locks can be had.
    pub locks: bool,
}

/// The options in effect, as `DSPMFSINF` and the mount table show them.
#[must_use]
pub fn shown(o: &MountOptions, s: &Settled) -> String {
    let mut v: Vec<String> = vec![
        if o.read_only { "ro" } else { "rw" }.into(),
        if o.suid { "suid" } else { "nosuid" }.into(),
        format!("vers={}", s.version.text()),
        format!(
            "proto={}",
            match s.proto {
                Proto::Tcp => "tcp",
                Proto::Udp => "udp",
            }
        ),
        format!("port={}", s.port),
    ];
    if let Some(p) = s.mountport {
        v.push(format!("mountport={p}"));
    }
    v.push(format!("rsize={}", s.rsize));
    v.push(format!("wsize={}", s.wsize));
    v.push(format!("timeo={}", o.timeo));
    v.push(format!("retrans={}", o.retrans));
    v.push(format!("retry={}", o.retry));
    v.push(if o.hard { "hard" } else { "soft" }.into());
    v.push(if o.intr { "intr" } else { "nointr" }.into());
    if o.noac {
        v.push("noac".into());
    } else {
        v.push(format!("acregmin={}", o.acregmin));
        v.push(format!("acregmax={}", o.acregmax));
        v.push(format!("acdirmin={}", o.acdirmin));
        v.push(format!("acdirmax={}", o.acdirmax));
    }
    v.push("sec=sys".into());
    v.join(",")
}

/// What the IFS is told about a mount of `source`.
#[must_use]
pub fn info(o: &MountOptions, s: &Settled, source: &str) -> RemoteInfo {
    let secs = |n: u32| Duration::from_secs(u64::from(n));
    let attr_times = if o.noac {
        AttrTimes::NONE
    } else {
        AttrTimes {
            reg_min: secs(o.acregmin),
            reg_max: secs(o.acregmax.max(o.acregmin)),
            dir_min: secs(o.acdirmin),
            dir_max: secs(o.acdirmax.max(o.acdirmin)),
        }
    };
    let v4 = s.version != Version::V3;
    RemoteInfo {
        name: s.version.vfs_name().into(),
        source: source.into(),
        options: shown(o, s),
        read_only: o.read_only,
        nosuid: !o.suid,
        attr_times,
        rsize: s.rsize,
        wsize: s.wsize,
        soft: !o.hard,
        intr: o.intr,
        case_sensitive: s.case_sensitive,
        keeps_open_unlinked: v4,
        stateful_open: v4,
        sync_writes: false,
        locks: s.locks,
        // The IFS reports the server once a call runs this long: one
        // major timeout, as the client's own retry policy counts it.
        slow_after: Policy::of(o).major(),
    }
}

/// The versions `vers=` tries, in order.
#[must_use]
pub fn versions(v: NfsVers) -> &'static [Version] {
    match v {
        NfsVers::V3 => &[Version::V3],
        NfsVers::V4 => &[Version::V41, Version::V40],
        NfsVers::V40 => &[Version::V40],
        NfsVers::V41 => &[Version::V41],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeouts_double_to_sixty_seconds() {
        let p = Policy::of(&crate::exports::mount::parse("timeo=10,retrans=3,soft").unwrap());
        assert_eq!(p.wait(0), Duration::from_secs(1));
        assert_eq!(p.wait(3), Duration::from_secs(8));
        assert_eq!(p.wait(9), MAX_WAIT);
        assert_eq!(p.major(), Duration::from_secs(15));
        assert!(p.soft);
        // IBM's default: 2 s, 5 retransmissions: 2+4+8+16+32+60.
        let d = Policy::of(&MountOptions::default());
        assert_eq!(d.major(), Duration::from_secs(122));
    }
}
