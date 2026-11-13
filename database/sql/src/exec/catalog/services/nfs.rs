// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `NFS_EXPORT_INFO` and `MOUNTED_FS_INFO`, QS/400's own views (no
//! release of IBM i has either; docs/NFS.md section 8). An export is
//! one row per host option, or one row with the host columns null when
//! it has none.

use store::txn::Txn;

use super::super::super::SqlContext;
use super::{Out, ask};
use crate::error::{Result, SqlError};

fn coded(what: String) -> SqlError {
    SqlError::Coded {
        code: -443,
        state: "38501",
        what,
    }
}

fn opt_text(out: &mut Out, col: &str, v: Option<&str>) {
    out.text_or_null(col, v.unwrap_or(""));
}

fn opt_int(out: &mut Out, col: &str, v: Option<u32>) {
    if let Some(n) = v {
        out.int(col, n);
    }
}

/// `NFS_EXPORT_INFO`.
pub fn exports(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    let list = ask(txn, ctx, "the NFS export table", |h| h.nfs_exports())?.map_err(coded)?;
    for e in &list {
        let hosts: Vec<Option<&super::host::NfsHostOpt>> = if e.hosts.is_empty() {
            vec![None]
        } else {
            e.hosts.iter().map(Some).collect()
        };
        for h in hosts {
            out.row();
            out.int("EXPORT_ID", e.id);
            out.text("PATH_NAME", e.path.as_str());
            out.yes("READ_ONLY", e.read_only);
            opt_text(out, "READ_WRITE_HOSTS", e.rw_hosts.as_deref());
            opt_text(out, "ACCESS_HOSTS", e.access_hosts.as_deref());
            opt_text(out, "ROOT_HOSTS", e.root_hosts.as_deref());
            opt_int(out, "ANONYMOUS_UID", e.anon_uid);
            opt_text(out, "ANONYMOUS_USER", e.anon_user.as_deref());
            out.yes("NOSUID", e.nosuid);
            out.text("SECURITY_FLAVORS", e.flavors.as_str());
            out.text("NFS_VERSIONS", e.versions.as_str());
            out.yes("PUBLIC_FILE_HANDLE", e.public);
            out.yes("PSEUDO_ROOT", e.pseudo_root);
            opt_text(out, "NFS_ROOT", e.nfsroot.as_deref());
            opt_text(out, "EXPORT_NAME", e.exname.as_deref());
            if let Some(h) = h {
                out.text("HOST_OPTION_HOST", h.host.as_str());
                opt_int(out, "HOST_DATA_CCSID", h.data_ccsid);
                opt_int(out, "HOST_PATH_CCSID", h.path_ccsid);
                out.text("HOST_WRITE_MODE", if h.sync { "SYNC" } else { "ASYNC" });
            }
            out.text("EXPORTED_BY", e.by.as_str());
            out.stamp("EXPORT_TIMESTAMP", e.since);
        }
    }
    Ok(())
}

/// `MOUNTED_FS_INFO`.
pub fn mounts(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    let list = ask(txn, ctx, "the mount table", |h| h.mounted_file_systems())?.map_err(coded)?;
    for m in &list {
        out.row();
        out.int("MOUNT_ID", m.id);
        out.text("FILE_SYSTEM_TYPE", m.kind.as_str());
        out.text("MOUNTED_OVER_PATH", m.at.as_str());
        out.text_or_null("MOUNTED_FILE_SYSTEM", &m.source);
        out.text_or_null("MOUNT_OPTIONS", &m.options);
        out.yes("READ_ONLY", m.read_only);
        out.yes("CASE_SENSITIVE", m.case_sensitive);
        if let Some(r) = m.responding {
            out.yes("RESPONDING", r);
        }
        opt_int(out, "DATA_CCSID", m.data_ccsid);
        opt_int(out, "PATH_CCSID", m.path_ccsid);
        if let Some(f) = m.figures {
            out.int("BLOCK_SIZE", f.block_size);
            out.int("TOTAL_BLOCKS", f.blocks);
            out.int("FREE_BLOCKS", f.free);
            out.int("AVAILABLE_BLOCKS", f.available);
            out.int("TOTAL_FILES", f.files);
            out.int("FREE_FILES", f.files_free);
            out.int("MAXIMUM_NAME_LENGTH", f.name_max);
        }
    }
    Ok(())
}
