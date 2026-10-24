// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/proc/net/{tcp,tcp6,udp,udp6}` and the socket inode to process map
//! from `/proc/<pid>/fd`, which NETSTAT uses to find a connection's job.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::Path;

/// One of the socket tables.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Table {
    /// `/proc/net/tcp`.
    Tcp,
    /// `/proc/net/tcp6`.
    Tcp6,
    /// `/proc/net/udp`.
    Udp,
    /// `/proc/net/udp6`.
    Udp6,
}

impl Table {
    /// File name under `/proc/net`.
    pub fn file(self) -> &'static str {
        match self {
            Table::Tcp => "tcp",
            Table::Tcp6 => "tcp6",
            Table::Udp => "udp",
            Table::Udp6 => "udp6",
        }
    }

    fn v6(self) -> bool {
        matches!(self, Table::Tcp6 | Table::Udp6)
    }
}

/// One line of a socket table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcSock {
    /// Local address.
    pub local: SocketAddr,
    /// Remote address.
    pub remote: SocketAddr,
    /// State (`TCP_*` numbering, as in [`crate::diag::TcpState`]).
    pub state: u8,
    /// Send queue bytes.
    pub tx_queue: u32,
    /// Receive queue bytes.
    pub rx_queue: u32,
    /// Owner uid.
    pub uid: u32,
    /// Socket inode.
    pub inode: u64,
}

/// Read a table from `/proc/net`.
pub fn read(table: Table) -> io::Result<Vec<ProcSock>> {
    read_in(Path::new("/proc/net"), table)
}

/// Read a table from a directory laid out like `/proc/net`.
pub fn read_in(dir: &Path, table: Table) -> io::Result<Vec<ProcSock>> {
    Ok(parse(&fs::read_to_string(dir.join(table.file()))?, table.v6()))
}

/// Parse a table's text. Malformed lines are skipped.
pub fn parse(text: &str, v6: bool) -> Vec<ProcSock> {
    text.lines().skip(1).filter_map(|l| parse_line(l, v6)).collect()
}

fn parse_line(line: &str, v6: bool) -> Option<ProcSock> {
    let f: Vec<&str> = line.split_whitespace().collect();
    if f.len() < 10 {
        return None;
    }
    let (tx, rx) = f[4].split_once(':')?;
    Some(ProcSock {
        local: parse_addr(f[1], v6)?,
        remote: parse_addr(f[2], v6)?,
        state: u8::from_str_radix(f[3], 16).ok()?,
        tx_queue: u32::from_str_radix(tx, 16).ok()?,
        rx_queue: u32::from_str_radix(rx, 16).ok()?,
        uid: f[7].parse().ok()?,
        inode: f[9].parse().ok()?,
    })
}

/// `0100007F:0050` (each 32-bit word in host order) to an address.
fn parse_addr(s: &str, v6: bool) -> Option<SocketAddr> {
    let (a, p) = s.split_once(':')?;
    let port = u16::from_str_radix(p, 16).ok()?;
    let word = |i: usize| -> Option<[u8; 4]> {
        let w = u32::from_str_radix(a.get(i * 8..i * 8 + 8)?, 16).ok()?;
        Some(w.to_le_bytes())
    };
    let ip = if v6 {
        if a.len() != 32 {
            return None;
        }
        let mut b = [0u8; 16];
        for i in 0..4 {
            b[i * 4..i * 4 + 4].copy_from_slice(&word(i)?);
        }
        IpAddr::V6(Ipv6Addr::from(b))
    } else {
        if a.len() != 8 {
            return None;
        }
        IpAddr::V4(Ipv4Addr::from(word(0)?))
    };
    Some(SocketAddr::new(ip, port))
}

/// The inode in a `/proc/<pid>/fd` link target (`socket:[12345]`).
pub fn socket_inode(target: &str) -> Option<u64> {
    target
        .strip_prefix("socket:[")?
        .strip_suffix(']')?
        .parse()
        .ok()
}

/// Socket inode to the lowest pid holding it, from `/proc`.
pub fn socket_owners() -> HashMap<u64, u32> {
    socket_owners_in(Path::new("/proc"))
}

/// Socket inode to the lowest pid holding it, from a `/proc`-like tree.
/// Processes that vanish or deny access are skipped.
pub fn socket_owners_in(proc_root: &Path) -> HashMap<u64, u32> {
    let mut out: HashMap<u64, u32> = HashMap::new();
    let Ok(rd) = fs::read_dir(proc_root) else {
        return out;
    };
    for e in rd.filter_map(Result::ok) {
        let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        let Ok(fds) = fs::read_dir(e.path().join("fd")) else {
            continue;
        };
        for fd in fds.filter_map(Result::ok) {
            let Ok(t) = fs::read_link(fd.path()) else {
                continue;
            };
            if let Some(ino) = t.to_str().and_then(socket_inode) {
                out.entry(ino)
                    .and_modify(|p| *p = (*p).min(pid))
                    .or_insert(pid);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // In the kernel's format; tests/fixtures.rs reads real captures.
    const TCP: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 0100007F:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 41871 1 0000000000000000 100 0 0 10 0
   1: 0100007F:1F90 0100007F:D2F4 01 00000000:00000000 00:00000000 00000000  1000        0 41872 1 0000000000000000 20 4 30 10 -1
";
    const TCP6: &str = "  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000000000000000000001000000:1F91 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 50001 1 0000000000000000 100 0 0 10 0
";

    #[test]
    fn parses_v4_lines() {
        let s = parse(TCP, false);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].local, "127.0.0.1:8080".parse().unwrap());
        assert_eq!(s[0].state, 10);
        assert_eq!(s[0].uid, 1000);
        assert_eq!(s[0].inode, 41871);
        assert_eq!(s[1].remote, "127.0.0.1:54004".parse().unwrap());
        assert_eq!(s[1].state, 1);
    }

    #[test]
    fn parses_v6_lines() {
        let s = parse(TCP6, true);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].local, "[::1]:8081".parse().unwrap());
        assert_eq!(s[0].inode, 50001);
    }

    #[test]
    fn reads_socket_link_targets() {
        assert_eq!(socket_inode("socket:[41871]"), Some(41871));
        assert_eq!(socket_inode("pipe:[3]"), None);
        assert_eq!(socket_inode("/dev/null"), None);
    }
}
