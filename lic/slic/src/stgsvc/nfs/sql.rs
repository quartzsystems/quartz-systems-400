// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The export table and the mount table as the `QSYS2` views show them
//! (`NFS_EXPORT_INFO`, `MOUNTED_FS_INFO`; docs/NFS.md section 8).

use nfs::exports::options::print_hosts;
use nfs::exports::{DataCcsid, HostSpec, PathCcsid};
use sql::exec::catalog::services::host::{MountedFs, NfsExport, NfsHostOpt};

use super::table::Entry;
use crate::ipl::system::System;

fn hosts(list: &[HostSpec]) -> Option<String> {
    (!list.is_empty()).then(|| print_hosts(list))
}

/// One entry as the view shows it.
#[must_use]
pub fn row(e: &Entry) -> NfsExport {
    let o = &e.export.options;
    let vers = o.versions();
    let mut versions = Vec::new();
    if vers.v3 {
        versions.push("3");
    }
    if vers.v4 {
        versions.push("4");
    }
    NfsExport {
        id: e.id,
        path: e.export.path.clone(),
        read_only: o.ro,
        rw_hosts: o.rw.as_deref().and_then(hosts),
        access_hosts: o.access.as_deref().and_then(hosts),
        root_hosts: hosts(&o.root_hosts),
        anon_uid: e.anon_uid,
        anon_user: (e.anon_profile != "*NONE").then(|| e.anon_profile.clone()),
        nosuid: o.nosuid,
        flavors: o
            .flavors()
            .iter()
            .map(|_| "SYS")
            .collect::<Vec<_>>()
            .join(":"),
        versions: versions.join(":"),
        public: o.public,
        pseudo_root: o.pseudo_root,
        nfsroot: o.nfsroot.clone(),
        exname: o.exname.clone(),
        hosts: e
            .export
            .hosts
            .iter()
            .map(|h| NfsHostOpt {
                host: print_hosts(std::slice::from_ref(&h.host)),
                // The table holds them resolved; *ASCII and *JOBCCSID
                // left are shown as unknown.
                data_ccsid: match h.data {
                    DataCcsid::Binary => Some(65535),
                    DataCcsid::Ccsid(n) => Some(u32::from(n)),
                    DataCcsid::Ascii | DataCcsid::Job => None,
                },
                path_ccsid: match h.path {
                    PathCcsid::Ccsid(n) => Some(u32::from(n)),
                    PathCcsid::Ascii | PathCcsid::Job => None,
                },
                sync: h.sync,
            })
            .collect(),
        by: e.by.clone(),
        since: e.since,
    }
}

/// `NFS_EXPORT_INFO`'s rows.
pub fn exports(_system: &mut System) -> Vec<NfsExport> {
    super::state().table.entries().iter().map(row).collect()
}

/// `MOUNTED_FS_INFO`'s rows: `None`, the mount table cannot be read
/// from here yet.
// BLOCKED(rel): the IFS mount table (NFS-4): `IfsClient::mounts()` with
// each mount's kind, source, options and CCSIDs, and `statvfs` of the
// local ones.
pub fn mounts(_system: &mut System) -> Option<Result<Vec<MountedFs>, String>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_as_the_view_shows_it() {
        let mut ex = nfs::exports::file::entry(
            1,
            "/home -ro,rw=a:[fd00::1],root=b,nosuid,vers=4 HOSTOPT pc1:819:1208:*ASYNC",
        )
        .expect("an entry");
        ex.hosts
            .push(nfs::exports::HostOpt::new(HostSpec::Name("c".into())));
        let e = Entry {
            id: 7,
            export: ex,
            anon_uid: None,
            anon_profile: "*NONE".into(),
            by: "j".into(),
            since: 5,
        };
        let r = row(&e);
        assert_eq!(r.rw_hosts.as_deref(), Some("a:[fd00::1]"));
        assert_eq!(r.root_hosts.as_deref(), Some("b"));
        assert_eq!(r.access_hosts, None);
        assert_eq!((r.versions.as_str(), r.flavors.as_str()), ("4", "SYS"));
        assert!(r.read_only && r.nosuid && r.anon_user.is_none());
        assert_eq!(r.hosts.len(), 2);
        assert_eq!(
            (
                r.hosts[0].data_ccsid,
                r.hosts[0].path_ccsid,
                r.hosts[0].sync
            ),
            (Some(819), Some(1208), false)
        );
        assert_eq!(r.hosts[1].data_ccsid, Some(65535));
    }
}
