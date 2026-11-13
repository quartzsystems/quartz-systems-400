// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The export table and the mount table as the `QSYS2` views show them
//! (`NFS_EXPORT_INFO`, `MOUNTED_FS_INFO`; docs/NFS.md section 8).

use ifs::remote::MountInfo;
use nfs::exports::options::print_hosts;
use nfs::exports::{DataCcsid, HostSpec, PathCcsid};
use sql::exec::catalog::services::host::{FsFigures, MountedFs, NfsExport, NfsHostOpt};

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

/// One mount as the view shows it; `figures` its `statvfs`, read only
/// for a local file system.
#[must_use]
pub fn mount_row(m: &MountInfo, figures: Option<FsFigures>) -> MountedFs {
    let remote = m.responding.is_some();
    MountedFs {
        id: m.id,
        kind: stg::nfs::vfs_name(&m.kind, &m.options),
        at: m.at.clone(),
        source: m.source.clone(),
        options: m.options.clone(),
        read_only: m.read_only,
        case_sensitive: m.case_sensitive,
        responding: m.responding,
        data_ccsid: remote.then_some(u32::from(m.data_ccsid)),
        path_ccsid: remote.then_some(u32::from(m.path_ccsid)),
        figures,
    }
}

/// `MOUNTED_FS_INFO`'s rows: the IFS mount table, `statvfs` of the local
/// file systems (a remote one's would wait for its server).
pub fn mounts(system: &mut System) -> Result<Vec<MountedFs>, String> {
    super::super::certs::as_system(system, |c| {
        let list = c.mounts().map_err(|e| e.text().to_string())?;
        Ok(list
            .iter()
            .map(|m| {
                let figures = m
                    .responding
                    .is_none()
                    .then(|| c.statvfs(&m.at).ok())
                    .flatten()
                    .map(|s| FsFigures {
                        block_size: s.bsize,
                        blocks: s.blocks,
                        free: s.bfree,
                        available: s.bavail,
                        files: s.files,
                        files_free: s.ffree,
                        name_max: u32::try_from(s.namemax).unwrap_or(u32::MAX),
                    });
                mount_row(m, figures)
            })
            .collect())
    })
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

    #[test]
    fn mounts_as_the_view_shows_them() {
        let nfs = MountInfo {
            id: 101,
            kind: "*NFS".into(),
            at: "/mnt/a".into(),
            source: "srv:/x".into(),
            options: "rw,vers=4,hard".into(),
            responding: Some(false),
            data_ccsid: 819,
            path_ccsid: 819,
            ..MountInfo::default()
        };
        let r = mount_row(&nfs, None);
        assert_eq!((r.kind.as_str(), r.data_ccsid), ("nfs4", Some(819)));
        let root = MountInfo {
            id: 1,
            kind: "root".into(),
            at: "/".into(),
            data_ccsid: 1208,
            ..MountInfo::default()
        };
        let r = mount_row(&root, Some(FsFigures::default()));
        assert_eq!((r.kind.as_str(), r.data_ccsid), ("root", None));
        assert!(r.figures.is_some() && r.responding.is_none());
    }
}
