// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `/etc` files NFS ships (docs/NFS.md section 6): `/etc/netconfig`
//! (CCSID 819), the transports of the RPC services, and `/etc/vfs`
//! (CCSID 1208), the file system types; the entries are the primary's,
//! the comments Quartz's. The storage service makes each one that is
//! missing when the IFS starts; a changed one is the customer's.

/// `(path, CCSID, text)`.
pub const FILES: &[(&str, u32, &str)] = &[
    (
        "/etc/netconfig",
        819,
        include_str!("../shipped/etc/netconfig"),
    ),
    ("/etc/vfs", 1208, include_str!("../shipped/etc/vfs")),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_primarys_entries() {
        let net = FILES[0].2;
        for t in ["tcp", "tcp_ord", "udp"] {
            assert!(
                net.lines().any(|l| l.split_whitespace().next() == Some(t)),
                "{t}"
            );
        }
        let vfs = FILES[1].2;
        assert!(vfs.contains("%defaultvfs  udfs   nfs3"));
        let numbers: Vec<&str> = vfs
            .lines()
            .filter(|l| !l.starts_with(['#', '%']))
            .filter_map(|l| l.split_whitespace().nth(1))
            .collect();
        assert_eq!(
            numbers,
            [
                "1", "2", "4", "8", "16", "32", "64", "128", "256", "1024", "8192"
            ]
        );
        assert!(!net.contains('\r') && !vfs.contains('\r'));
    }
}
