// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The DHCP leases kept across IPLs: one stream file, a `[LINE]` section
//! per line description with the lease's `key=value` lines
//! (`dhcp::Lease::to_text`). `STRTCP` asks for a kept lease again
//! (`INIT-REBOOT`); `ENDTCPIFC` gives it back and drops it here.

use std::collections::BTreeMap;

use crate::ipl::system::System;
use crate::stgsvc::certs::{as_system, read_file, write_file};

/// Where they are kept.
pub const PATH: &str = "/QIBM/UserData/OS400/TCPIP/DHCP/LEASES";

/// The file's text for `leases`.
pub fn to_text(leases: &BTreeMap<String, dhcp::Lease>) -> String {
    let mut s = String::new();
    for (line, l) in leases {
        s.push('[');
        s.push_str(line);
        s.push_str("]\n");
        s.push_str(&l.to_text());
    }
    s
}

/// The leases in a file's text; a section that does not read is skipped.
pub fn from_text(t: &str) -> BTreeMap<String, dhcp::Lease> {
    let mut out = BTreeMap::new();
    let mut name: Option<String> = None;
    let mut body = String::new();
    let flush = |name: Option<&String>, body: &str, out: &mut BTreeMap<String, dhcp::Lease>| {
        if let (Some(n), Some(l)) = (name, dhcp::Lease::from_text(body)) {
            out.insert(n.clone(), l);
        }
    };
    for line in t.lines() {
        if let Some(n) = line
            .trim()
            .strip_prefix('[')
            .and_then(|r| r.strip_suffix(']'))
        {
            flush(name.as_ref(), &body, &mut out);
            name = Some(n.trim().to_string());
            body.clear();
        } else {
            body.push_str(line);
            body.push('\n');
        }
    }
    flush(name.as_ref(), &body, &mut out);
    out
}

/// The kept leases; none when the file or the IFS is not there.
pub fn load(system: &mut System) -> BTreeMap<String, dhcp::Lease> {
    as_system(system, |c| read_file(c, PATH))
        .ok()
        .flatten()
        .map(|b| from_text(&String::from_utf8_lossy(&b)))
        .unwrap_or_default()
}

/// Keep `leases`.
pub fn save(system: &mut System, leases: &BTreeMap<String, dhcp::Lease>) {
    let text = to_text(leases);
    if let Err(e) = as_system(system, |c| write_file(c, PATH, text.as_bytes())) {
        eprintln!("qsstg: DHCP leases not kept: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn sections_round_trip() {
        let l = dhcp::Lease {
            addr: Ipv4Addr::new(10, 0, 2, 15),
            mask: Ipv4Addr::new(255, 255, 255, 0),
            router: Some(Ipv4Addr::new(10, 0, 2, 2)),
            dns: vec![Ipv4Addr::new(10, 0, 2, 3)],
            domain: String::new(),
            server: Ipv4Addr::new(10, 0, 2, 2),
            boot: None,
            obtained: 1_793_750_000,
            secs: 86_400,
            t1: 43_200,
            t2: 75_600,
        };
        let mut m = BTreeMap::new();
        m.insert("ETHLINE".to_string(), l.clone());
        m.insert(
            "ETH2".to_string(),
            dhcp::Lease {
                addr: Ipv4Addr::new(10, 1, 1, 9),
                ..l
            },
        );
        assert_eq!(from_text(&to_text(&m)), m);
        assert!(from_text("[X]\naddr=nonsense\n").is_empty());
    }
}
