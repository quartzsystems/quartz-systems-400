// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A file share over the IFS: [`IfsShare`], the library's `ShareBackend`,
//! with every call made through the job's [`ifs::client::IfsClient`] on
//! the [`IfsWorker`] thread, so the IFS checks and audits each one as the
//! job's profile.
//!
//! - Names: looked up and stored by the IFS's own rules (case-insensitive
//!   and case-preserving on `/` and `/QSYS.LIB`, case-sensitive under
//!   `/QOpenSys`); the share reports the case sensitivity of its root.
//! - Share modes: an open with data access opens its descriptor with the
//!   matching `O_SHARE_*`, so opens in other jobs are refused as the IFS
//!   refuses them; attribute-only opens take no descriptor.
//! - DOS attributes: read-only, hidden, system and archive are the IFS's
//!   `*READONLY`, `*HIDDEN`, `*SYSTEM` and `*PCARCHIVE`.
//! - File IDs: the IFS file ID (number and generation); a handle finds its
//!   object by it after a rename.
//! - No change events: the IFS has none, so clients hear only of changes
//!   made through this job's connections.

mod handle;
pub mod info;
pub mod pattern;
pub mod worker;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use ifs::Errno;
use ifs::client::IfsClient;
use ifs::ops::audit::FileId;
use ifs::ops::flags::{
    O_CREAT, O_EXCL, O_INHERITMODE, O_RDONLY, O_RDWR, O_SHARE_NONE, O_SHARE_RDONLY, O_SHARE_RDWR,
    O_SHARE_WRONLY, O_TRUNC, O_WRONLY, S_IFDIR, S_IFMT,
};
use ifs::ops::{Attr, AttrId, OpenArgs, RenameMode};
use smb_server::{
    BackendCapabilities, Handle, OpenAction, OpenIntent, OpenOptions, ShareBackend, SmbError,
    SmbPath, SmbResult, VolumeInfo,
};

pub use handle::IfsHandle;
use handle::Fd;
use info::smb_error;
pub use worker::{IfsWorker, MakeClient};

/// `FILE_SHARE_READ`.
const SHARE_READ: u32 = 0x1;
/// `FILE_SHARE_WRITE`.
const SHARE_WRITE: u32 = 0x2;

/// A file share rooted at an IFS directory.
pub struct IfsShare {
    io: Arc<IfsWorker>,
    root: String,
    read_only: bool,
    case_sensitive: bool,
}

impl IfsShare {
    /// A share of the directory `root` (an absolute IFS path); `read_only`
    /// is the share's permission, which narrows the IFS's and never widens
    /// it.
    ///
    /// # Errors
    ///
    /// When `root` is not a directory the job can reach.
    pub async fn new(io: Arc<IfsWorker>, root: &str, read_only: bool) -> SmbResult<Self> {
        let root = normalize(root);
        let r = root.clone();
        let case_sensitive = io
            .call(move |c| {
                let st = c.stat(&r, true)?;
                if st.mode & S_IFMT != S_IFDIR {
                    return Err(Errno::ENOTDIR);
                }
                Ok(c.statvfs(&r).is_ok_and(|v| v.case_sensitive))
            })
            .await
            .map_err(smb_error)?;
        Ok(Self { io, root, read_only, case_sensitive })
    }

    /// The IFS path of a share path.
    #[must_use]
    pub fn ifs_path(&self, p: &SmbPath) -> String {
        join(&self.root, p.components())
    }
}

fn normalize(root: &str) -> String {
    let t = root.trim_end_matches('/');
    if t.is_empty() { "/".to_owned() } else { t.to_owned() }
}

fn join(root: &str, parts: &[String]) -> String {
    let mut out = root.to_owned();
    for p in parts {
        if !out.ends_with('/') {
            out.push('/');
        }
        out.push_str(p);
    }
    out
}

fn parent(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) if i > 0 => &path[..i],
        _ => "/",
    }
}

/// The `O_SHARE_*` of a CREATE's `ShareAccess`.
fn share_flag(share: Option<u32>) -> u32 {
    match share.map(|s| (s & SHARE_READ != 0, s & SHARE_WRITE != 0)) {
        None | Some((true, true)) => O_SHARE_RDWR,
        Some((true, false)) => O_SHARE_RDONLY,
        Some((false, true)) => O_SHARE_WRONLY,
        Some((false, false)) => O_SHARE_NONE,
    }
}

/// What a synchronous open found.
struct Opened {
    is_dir: bool,
    fd: Option<Fd>,
    action: OpenAction,
    id: Option<FileId>,
}

fn file_id(c: &dyn IfsClient, path: &str) -> Option<FileId> {
    c.get_attrs(path, &[AttrId::FileId], true).ok()?.into_iter().find_map(|a| match a {
        Attr::FileId(id) => Some(id),
        _ => None,
    })
}

/// A missing object: [`SmbError::PathNotFound`] when its directory is
/// missing too.
fn missing(c: &dyn IfsClient, path: &str) -> SmbError {
    match c.stat(parent(path), true) {
        Ok(st) if st.mode & S_IFMT == S_IFDIR => SmbError::NotFound,
        _ => SmbError::PathNotFound,
    }
}

fn open_sync(c: &dyn IfsClient, path: &str, opts: OpenOptions) -> SmbResult<Opened> {
    let existing = match c.stat(path, true) {
        Ok(st) => Some(st),
        Err(Errno::ENOENT) => None,
        Err(e) => return Err(smb_error(e)),
    };
    let access = match (opts.read, opts.write) {
        (_, false) => O_RDONLY,
        (false, true) => O_WRONLY,
        (true, true) => O_RDWR,
    };
    let share = share_flag(opts.share_access);
    let Some(st) = existing else {
        if matches!(opts.intent, OpenIntent::Open | OpenIntent::Truncate) {
            return Err(missing(c, path));
        }
        if opts.directory {
            c.mkdir(path, 0o777).map_err(|e| if e == Errno::ENOENT { missing(c, path) } else { smb_error(e) })?;
            return Ok(Opened { is_dir: true, fd: None, action: OpenAction::Created, id: file_id(c, path) });
        }
        let oflag = access | share | O_CREAT | O_EXCL | O_INHERITMODE;
        let fd = c
            .open(path, OpenArgs::new(oflag).mode(0o666))
            .map_err(|e| if e == Errno::ENOENT { missing(c, path) } else { smb_error(e) })?;
        let write = opts.write;
        return Ok(Opened { is_dir: false, fd: Some(Fd { fd, write }), action: OpenAction::Created, id: file_id(c, path) });
    };
    if opts.intent == OpenIntent::Create {
        return Err(SmbError::Exists);
    }
    let is_dir = st.mode & S_IFMT == S_IFDIR;
    if is_dir {
        if opts.non_directory {
            return Err(SmbError::IsDirectory);
        }
        return Ok(Opened { is_dir, fd: None, action: OpenAction::Opened, id: file_id(c, path) });
    }
    if opts.directory {
        return Err(SmbError::NotADirectory);
    }
    let truncate = matches!(opts.intent, OpenIntent::Truncate | OpenIntent::OverwriteOrCreate);
    let action = if truncate { OpenAction::Overwritten } else { OpenAction::Opened };
    // An open without data access (attributes, the server's own) takes its
    // descriptor when it first reads.
    if !opts.write && !truncate && opts.share_access.is_none() {
        return Ok(Opened { is_dir, fd: None, action, id: file_id(c, path) });
    }
    let oflag = access | share | if truncate { O_TRUNC } else { 0 };
    let fd = c.open(path, OpenArgs::new(oflag)).map_err(smb_error)?;
    Ok(Opened { is_dir, fd: Some(Fd { fd, write: opts.write }), action, id: file_id(c, path) })
}

#[async_trait]
impl ShareBackend for IfsShare {
    async fn open(&self, path: &SmbPath, opts: OpenOptions) -> SmbResult<(Box<dyn Handle>, OpenAction)> {
        let full = self.ifs_path(path);
        let p = full.clone();
        let opened = self
            .io
            .call(move |c| Ok(open_sync(c, &p, opts)))
            .await
            .map_err(smb_error)??;
        let handle = IfsHandle {
            io: self.io.clone(),
            path: Mutex::new(full),
            id: opened.id,
            is_dir: opened.is_dir,
            fd: Mutex::new(opened.fd),
            root: self.root.clone(),
        };
        Ok((Box::new(handle), opened.action))
    }

    async fn unlink(&self, path: &SmbPath) -> SmbResult<()> {
        let p = self.ifs_path(path);
        self.io
            .call(move |c| {
                let st = c.stat(&p, false)?;
                if st.mode & S_IFMT == S_IFDIR { c.rmdir(&p) } else { c.unlink(&p) }
            })
            .await
            .map_err(smb_error)
    }

    async fn rename(&self, from: &SmbPath, to: &SmbPath, replace_if_exists: bool) -> SmbResult<()> {
        let (f, t) = (self.ifs_path(from), self.ifs_path(to));
        self.io
            .call(move |c| {
                let src = c.stat(&f, false)?;
                let mode = match c.stat(&t, false) {
                    // The same object under another spelling.
                    Ok(dst) if dst.ino == src.ino && dst.dev == src.dev => RenameMode::Keep,
                    Ok(dst) if dst.mode & S_IFMT == S_IFDIR => return Ok(Err(SmbError::AccessDenied)),
                    Ok(_) if !replace_if_exists => return Ok(Err(SmbError::Exists)),
                    Ok(_) => RenameMode::Unlink,
                    Err(_) => RenameMode::Keep,
                };
                Ok(c.rename(&f, &t, mode).map_err(smb_error))
            })
            .await
            .map_err(smb_error)?
    }

    fn capabilities(&self) -> BackendCapabilities {
        let mut caps = BackendCapabilities::default();
        caps.is_read_only = self.read_only;
        caps.case_sensitive = self.case_sensitive;
        caps
    }

    async fn volume_info(&self) -> SmbResult<VolumeInfo> {
        let root = self.root.clone();
        let v = self.io.call(move |c| c.statvfs(&root)).await.map_err(smb_error)?;
        let mut info = VolumeInfo::default();
        let unit = if v.frsize == 0 { v.bsize.max(512) } else { v.frsize };
        info.bytes_per_sector = u32::try_from(unit).unwrap_or(4096);
        info.sectors_per_allocation_unit = 1;
        info.total_allocation_units = v.blocks;
        info.actual_available_allocation_units = v.bfree;
        info.caller_available_allocation_units = v.bavail;
        info.serial_number = v.fsid;
        if v.namemax > 0 {
            info.max_name_length = u32::try_from(v.namemax).unwrap_or(255);
        }
        Ok(info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_and_share_modes() {
        assert_eq!(join("/", &["a".into(), "b".into()]), "/a/b");
        assert_eq!(join("/home/x", &[]), "/home/x");
        assert_eq!(normalize("/home/x/"), "/home/x");
        assert_eq!(parent("/a"), "/");
        assert_eq!(share_flag(None), O_SHARE_RDWR);
        assert_eq!(share_flag(Some(0)), O_SHARE_NONE);
        assert_eq!(share_flag(Some(1)), O_SHARE_RDONLY);
        assert_eq!(share_flag(Some(7)), O_SHARE_RDWR);
    }
}
