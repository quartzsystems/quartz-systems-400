// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `nfstest probe`: the binder's table as `rpcinfo -p` prints it and the
//! export list as `showmount -e` does, for a harness to learn the ports.

use std::fmt::Write;
use std::net::SocketAddr;

use crate::ctx::{self, Ctx, prog};
use crate::v3::Mount;

fn service(p: u32) -> &'static str {
    match p {
        prog::PMAP => "portmapper",
        prog::NFS => "nfs",
        prog::MOUNT => "mountd",
        prog::NLM => "nlockmgr",
        prog::NSM => "status",
        _ => "",
    }
}

/// The probe's text. A line `mapped` follows a port the port map changed.
pub fn run(c: &Ctx) -> String {
    let mut out = String::new();
    let mut mount = c.mount;
    match ctx::dump(c) {
        Ok(list) => {
            let _ = writeln!(out, "   program vers proto   port  service");
            for m in &list {
                let proto = match m.prot {
                    6 => "tcp",
                    17 => "udp",
                    _ => "?",
                };
                let port = u16::try_from(m.port).unwrap_or(0);
                let host = c.t.map_port(port);
                let mapped = if host == port {
                    String::new()
                } else {
                    format!("  (host port {host})")
                };
                let _ = writeln!(
                    out,
                    "{:>10} {:>4} {:>5} {:>6}  {}{mapped}",
                    m.prog,
                    m.vers,
                    proto,
                    m.port,
                    service(m.prog)
                );
                if mount.is_none() && m.prog == prog::MOUNT && m.vers == 3 && m.prot == 6 {
                    mount = Some(SocketAddr::new(c.ip, host));
                }
            }
        }
        Err(e) => {
            let _ = writeln!(out, "rpcinfo: the binder at {} did not answer: {e}", c.pmap);
        }
    }
    let Some(addr) = mount else {
        let _ = writeln!(out, "showmount: MOUNT's port is not known");
        return out;
    };
    let mut m = Mount {
        c: c.client_as(addr, crate::rpc::Proto::Tcp, crate::rpc::Cred::None),
    };
    match m.export() {
        Ok(list) => {
            let _ = writeln!(out, "Export list for {}:", c.t.host);
            for (d, g) in list {
                let groups: Vec<String> = g
                    .iter()
                    .map(|x| String::from_utf8_lossy(x).into_owned())
                    .collect();
                let groups = if groups.is_empty() {
                    "(everyone)".to_string()
                } else {
                    groups.join(",")
                };
                let _ = writeln!(out, "{:<30} {groups}", String::from_utf8_lossy(&d));
            }
        }
        Err(e) => {
            let _ = writeln!(out, "showmount: EXPORT failed: {e}");
        }
    }
    out
}
