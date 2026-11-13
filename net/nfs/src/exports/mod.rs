// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The export table's sources and option lists (docs/NFS.md sections
//! 5, 6.1 and 7), with no I/O:
//!
//! - [`model`], an export entry, its options and host options.
//! - [`options`], the export options of `EXPORTFS -O` and `/etc/exports`.
//! - [`file`], `/etc/exports`, read, printed and edited.
//! - [`flags`], the flags of `EXPORTFS OPTIONS` and what they ask for.
//! - [`netgroup`], `/etc/netgroup`.
//! - [`hosts`]: whether a client is one of a host list, and what an
//!   export lets it do, over the caller's [`hosts::Resolver`].
//! - [`mount`], `MOUNT TYPE(*NFS) OPTIONS` and `MFS`.
//! - [`api`], the `QZNFRTVE` entries.

pub mod api;
pub mod error;
pub mod file;
pub mod flags;
pub mod hosts;
mod lines;
mod list;
pub mod model;
pub mod mount;
pub mod netgroup;
pub mod options;

pub use error::{EntryError, FlagError, MfsError, NetgroupError, OptError};
pub use flags::{Flags, Request};
pub use hosts::{Access, Client, NoNames, Resolver};
pub use model::{DataCcsid, Export, ExportOptions, Flavor, HostOpt, HostSpec, PathCcsid, Versions};
pub use mount::{MountOptions, NfsVers, Proto};
pub use netgroup::Netgroups;

/// The fuzz target `nfsexports`: the first byte picks the parser, the
/// rest is its input. Nothing may panic.
pub fn fuzz(data: &[u8]) {
    let Some((&which, rest)) = data.split_first() else {
        return;
    };
    let text = String::from_utf8_lossy(rest);
    match which % 5 {
        0 => {
            for e in file::parse(&text).into_iter().flatten() {
                let _ = file::entry(1, &file::print_entry(&e));
                let _ = file::upsert(&text, &e);
            }
            let _ = file::remove(&text, "/a");
        }
        1 => {
            if let Ok(f) = flags::parse(&text) {
                let _ = f.request(Some("/x"));
                let _ = f.request(None);
            }
        }
        2 => {
            if let Ok(g) = Netgroups::parse(&text) {
                let c = Client::new(std::net::Ipv4Addr::LOCALHOST.into(), &NoNames);
                let _ = g.contains("a", &c);
            }
        }
        3 => {
            let _ = mount::parse(&text);
            let _ = mount::parse_mfs(&text);
        }
        _ => {
            if let Ok(o) = options::parse(&text) {
                let _ = options::parse(&options::print(&o));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    /// Every prefix of every seed, and each byte flipped, through every
    /// parser: nothing panics.
    #[test]
    fn fuzz_entry_survives_cut_and_flipped_input() {
        let seeds: [&[u8]; 5] = [
            b"\x00/home/joe -ro, access=a:b HOSTOPT [fd00::1]:819:*ASCII:*ASYNC \\n /x -rw=@[fd00::]/64\n",
            b"\x01-I -F -O 'ro,anon=0' -V -E",
            b"\x02lab (gamma,,) servers\nservers (-,root,) lab\n",
            b"\x03rw,soft,vers=4.1,actimeo=10,proto=udp",
            b"\x04ro,rw=a:@10.0.0.0/255.0.0.0,root,root=b,anon=-1,vers=3:4,exname=/e",
        ];
        for s in seeds {
            for n in 0..=s.len() {
                super::fuzz(&s[..n]);
            }
            for i in 1..s.len() {
                let mut v = s.to_vec();
                v[i] ^= 0x5A;
                super::fuzz(&v);
            }
        }
    }
}
