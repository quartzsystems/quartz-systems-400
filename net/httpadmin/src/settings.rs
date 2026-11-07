// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Global settings (`/www/admin/conf/global.json`): defaults for new
//! instances, limits, the administration server's own rules, the audit
//! flag.

use httpd::files::Files;
use serde::{Deserialize, Serialize};

/// Where the settings live.
pub const PATH: &str = "/www/admin/conf/global.json";

/// The settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Server user profile of new instances.
    pub default_user: String,
    /// The directory new server roots go under.
    pub default_root: String,
    /// `ThreadsPerChild` of new instances.
    pub default_threads: u32,
    /// Most instances the system may have.
    pub max_instances: u32,
    /// Most threads one instance may have.
    pub max_threads: u32,
    /// Port 2001 redirects to the TLS port.
    pub admin_tls: bool,
    /// The administration server's TLS port.
    pub admin_tls_port: u16,
    /// Minutes a session may idle.
    pub session_minutes: u32,
    /// Networks administration is allowed from (`10.0.0.0/8`); empty
    /// allows every address.
    pub allow_from: Vec<String>,
    /// Members of this group profile may sign on without `*IOSYSCFG`
    /// (`*NONE` for none).
    pub admin_group: String,
    /// Every change is written to `QAUDJRN` as a `ZC` entry.
    pub audit: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_user: "QTMHHTTP".into(),
            default_root: "/www/".into(),
            default_threads: 40,
            max_instances: 50,
            max_threads: 200,
            admin_tls: false,
            admin_tls_port: 2010,
            session_minutes: 30,
            allow_from: Vec::new(),
            admin_group: "*NONE".into(),
            audit: true,
        }
    }
}

impl Settings {
    /// Read the settings; defaults when the file is absent or damaged.
    pub fn load(files: &dyn Files) -> Self {
        files
            .read(PATH)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    /// Write the settings.
    pub fn save(&self, files: &dyn Files) -> std::io::Result<()> {
        files.mkdir_all("/www/admin/conf")?;
        let mut t = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        t.push('\n');
        files.write_atomic(PATH, t.as_bytes())
    }

    /// Whether administration is allowed from `addr`.
    pub fn allows(&self, addr: std::net::IpAddr) -> bool {
        self.allow_from.is_empty() || self.allow_from.iter().any(|n| in_net(addr, n))
    }
}

/// Whether `addr` is in network `net` (`10.0.0.0/8`, `::1/128`, or a
/// single address).
pub fn in_net(addr: std::net::IpAddr, net: &str) -> bool {
    use std::net::IpAddr;
    let (a, bits) = net.trim().split_once('/').unwrap_or((net.trim(), ""));
    let Ok(base) = a.parse::<IpAddr>() else {
        return false;
    };
    match (addr, base) {
        (IpAddr::V4(x), IpAddr::V4(y)) => {
            let bits: u32 = bits.parse().unwrap_or(32).min(32);
            let mask = if bits == 0 {
                0
            } else {
                u32::MAX << (32 - bits)
            };
            u32::from(x) & mask == u32::from(y) & mask
        }
        (IpAddr::V6(x), IpAddr::V6(y)) => {
            let bits: u32 = bits.parse().unwrap_or(128).min(128);
            let mask = if bits == 0 {
                0
            } else {
                u128::MAX << (128 - bits)
            };
            u128::from(x) & mask == u128::from(y) & mask
        }
        (IpAddr::V6(x), IpAddr::V4(_)) => x
            .to_ipv4_mapped()
            .is_some_and(|v4| in_net(IpAddr::V4(v4), net)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpd::files::MemFiles;

    #[test]
    fn networks_and_storage() {
        let s = Settings {
            allow_from: vec!["10.0.0.0/8".into(), "172.16.0.0/12".into()],
            ..Settings::default()
        };
        assert!(s.allows("10.1.2.3".parse().unwrap()));
        assert!(s.allows("172.31.0.1".parse().unwrap()));
        assert!(!s.allows("192.168.1.1".parse().unwrap()));
        assert!(s.allows("::ffff:10.0.0.1".parse().unwrap()));
        assert!(Settings::default().allows("192.168.1.1".parse().unwrap()));
        let f = MemFiles::new();
        assert_eq!(Settings::load(&f), Settings::default());
        s.save(&f).unwrap();
        assert_eq!(Settings::load(&f), s);
    }
}
