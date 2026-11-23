// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One open of a file or directory: an IFS descriptor for a file's data
//! (opened at CREATE when the open has data access, else on first use),
//! and the object's file ID, which finds it again after a rename.
//!
//! Each call is as few requests to the storage service as it can be: a
//! stat checks the object is still at the path opened and answers in the
//! same request ([`locate_stat`]); a listing is one request
//! (`read_dir_stat`); the attributes the open read answer the CREATE's
//! own stat ([`IfsHandle::primed`]).

use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use bytes::Bytes;
use ifs::Errno;
use ifs::client::{CHUNK, IfsClient};
use ifs::lock::range::RangeKind;
use ifs::ops::audit::FileId;
use ifs::ops::flags::{F_SETLK, O_RDONLY, O_SHARE_RDWR, SEEK_SET};
use ifs::ops::{Attr, AttrId, OpenArgs, Stat};
use smb_server::{DirEntry, DirWatch, FileInfo, FileTimes, Handle, LeaseKey, SmbError, SmbResult};

use super::info::{ATTRS, WANTED, attr_changes, file_info, nanos, smb_error};
use super::lease::LeaseLink;
use super::pattern::matches;
use super::short;
use super::text::Xlate;
use super::worker::IfsWorker;

/// A descriptor and what it may do.
#[derive(Debug, Clone, Copy)]
pub(super) struct Fd {
    pub(super) fd: i32,
    pub(super) write: bool,
}

/// An open the backend gave.
pub struct IfsHandle {
    pub(super) io: Arc<IfsWorker>,
    /// The path it was opened by; the file ID is tried first.
    pub(super) path: Mutex<String>,
    pub(super) id: Option<FileId>,
    pub(super) is_dir: bool,
    pub(super) fd: Mutex<Option<Fd>>,
    /// Its share's root, to name the root itself.
    pub(super) root: String,
    /// The job's leases in the IFS's table, when the share has leases.
    pub(super) leases: Option<Arc<LeaseLink>>,
    /// Leases this open reserved there.
    pub(super) reserved: Mutex<Vec<[u8; 32]>>,
    /// Its data converted as text, by its share ([`super::text`]).
    pub(super) xlate: Option<Arc<Xlate>>,
    /// What the open read of it, for the first `stat` when nothing else
    /// came first (the CREATE's own); any other call drops it.
    pub(super) primed: Mutex<Option<FileInfo>>,
}

/// Whether `st` is the object `id`.
fn is(id: FileId, st: &Stat) -> bool {
    st.ino == id.ino && st.dev == id.fs && (id.generation == 0 || st.ino_gen == id.generation)
}

/// Where the object is now: the path opened while it still names the
/// object (one `stat`), else found by file ID (a walk of its file
/// system, only after a rename), else the path opened.
pub(super) fn locate(c: &dyn IfsClient, id: Option<FileId>, path: &str) -> String {
    let Some(id) = id else {
        return path.to_owned();
    };
    if c.stat(path, false).is_ok_and(|st| is(id, &st)) {
        return path.to_owned();
    }
    c.path_from_file_id(id).unwrap_or_else(|_| path.to_owned())
}

/// Where the object is now, its `stat()` (following a last symbolic
/// link) and its attributes `ids`: one request while the path opened
/// still names the object; else found by file ID as [`locate`] finds it.
///
/// # Errors
///
/// The stat's.
pub(super) fn locate_stat(
    c: &dyn IfsClient,
    id: Option<FileId>,
    path: &str,
    ids: &[AttrId],
) -> ifs::Result<(String, Stat, Vec<Attr>)> {
    let first = c.stat_attrs(path, ids, true);
    let Some(id) = id else {
        return first.map(|(s, a)| (path.to_owned(), s, a));
    };
    if let Ok((st, _)) = &first
        && is(id, st)
    {
        return first.map(|(s, a)| (path.to_owned(), s, a));
    }
    match c.path_from_file_id(id) {
        Ok(p) if p != path => {
            let (s, a) = c.stat_attrs(&p, ids, true)?;
            Ok((p, s, a))
        }
        _ => first.map(|(s, a)| (path.to_owned(), s, a)),
    }
}

/// The last component of `path`, or empty for the share root.
pub(super) fn leaf<'a>(path: &'a str, root: &str) -> &'a str {
    if path == root {
        ""
    } else {
        path.rsplit('/').next().unwrap_or("")
    }
}

fn err<T>(r: ifs::Result<T>) -> SmbResult<T> {
    r.map_err(smb_error)
}

/// An SMB byte range as the IFS's `l_start` and `l_len`, `None` when it
/// locks nothing or starts past what the IFS addresses. A range that runs
/// past `i64::MAX` (Linux sends `flock` as 0 for 2^63) runs to the end.
fn ifs_range(offset: u64, len: u64) -> Option<(i64, i64)> {
    let start = i64::try_from(offset).ok()?;
    if len == 0 {
        return None;
    }
    let len = offset
        .checked_add(len)
        .and_then(|end| i64::try_from(end).ok())
        .map_or(0, |_| len.cast_signed());
    Some((start, len))
}

impl IfsHandle {
    fn path(&self) -> String {
        self.path
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The object was found at `p`: later calls start there.
    fn found(&self, p: String) {
        let mut cur = self.path.lock().unwrap_or_else(PoisonError::into_inner);
        if *cur != p {
            *cur = p;
        }
    }

    /// Another call came first: the open's attributes are no longer news.
    fn unprime(&self) {
        self.primed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
    }

    /// The descriptor, opened for reading now if there is none.
    async fn fd(&self) -> SmbResult<Fd> {
        if self.is_dir {
            return Err(SmbError::IsDirectory);
        }
        if let Some(fd) = *self.fd.lock().unwrap_or_else(PoisonError::into_inner) {
            return Ok(fd);
        }
        let (id, path) = (self.id, self.path());
        let fd = err(self
            .io
            .call(move |c| {
                c.open(
                    &locate(c, id, &path),
                    OpenArgs::new(O_RDONLY | O_SHARE_RDWR),
                )
            })
            .await)?;
        let mut slot = self.fd.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(have) = *slot {
            self.io.post(move |c| {
                let _ = c.close(fd);
            });
            return Ok(have);
        }
        let fd = Fd { fd, write: false };
        *slot = Some(fd);
        Ok(fd)
    }
}

#[async_trait]
impl Handle for IfsHandle {
    async fn read(&self, offset: u64, len: u32) -> SmbResult<Bytes> {
        self.unprime();
        let Fd { fd, .. } = self.fd().await?;
        let mut data = err(self
            .io
            .call(move |c| {
                let want = usize::try_from(len).unwrap_or(usize::MAX);
                // One request for a read of a chunk or less; its bytes kept.
                let mut out = c.pread(fd, want.min(CHUNK), offset)?;
                while !out.is_empty() && out.len() < want {
                    let chunk =
                        c.pread(fd, (want - out.len()).min(CHUNK), offset + out.len() as u64)?;
                    if chunk.is_empty() {
                        break;
                    }
                    out.extend_from_slice(&chunk);
                }
                Ok(out)
            })
            .await)?;
        if let Some(x) = &self.xlate {
            x.to_client(&mut data);
        }
        Ok(Bytes::from(data))
    }

    async fn write(&self, offset: u64, data: &[u8]) -> SmbResult<u32> {
        self.write_owned(offset, data.to_vec()).await
    }

    async fn write_owned(&self, offset: u64, mut data: Vec<u8>) -> SmbResult<u32> {
        self.unprime();
        let Fd { fd, write } = self.fd().await?;
        if !write {
            return Err(SmbError::AccessDenied);
        }
        if let Some(x) = &self.xlate {
            x.to_file(&mut data);
        }
        err(self
            .io
            .call(move |c| {
                let mut done = 0;
                while done < data.len() {
                    let end = (done + CHUNK).min(data.len());
                    let n = c.pwrite(fd, &data[done..end], offset + done as u64)?;
                    if n == 0 {
                        return Err(Errno::ENOSPC);
                    }
                    done += n;
                }
                Ok(u32::try_from(done).unwrap_or(u32::MAX))
            })
            .await)
    }

    async fn flush(&self) -> SmbResult<()> {
        self.unprime();
        let fd = *self.fd.lock().unwrap_or_else(PoisonError::into_inner);
        match fd {
            Some(Fd { fd, write: true }) => err(self.io.call(move |c| c.fsync(fd, false)).await),
            _ => Ok(()),
        }
    }

    async fn stat(&self) -> SmbResult<FileInfo> {
        let primed = self
            .primed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(info) = primed {
            return Ok(info);
        }
        let (id, path, root) = (self.id, self.path(), self.root.clone());
        let (p, info) = err(self
            .io
            .call(move |c| {
                let (p, st, attrs) = locate_stat(c, id, &path, WANTED)?;
                let info = file_info(leaf(&p, &root), &st, &attrs);
                Ok((p, info))
            })
            .await)?;
        self.found(p);
        Ok(info)
    }

    async fn set_times(&self, times: FileTimes) -> SmbResult<()> {
        self.unprime();
        if times.last_access_time.is_none() && times.last_write_time.is_none() {
            // The IFS keeps creation and change times itself.
            return Ok(());
        }
        let (id, path) = (self.id, self.path());
        let p = err(self
            .io
            .call(move |c| {
                let (p, st, _) = locate_stat(c, id, &path, &[])?;
                let a = times.last_access_time.map_or(st.atime, nanos);
                let m = times.last_write_time.map_or(st.mtime, nanos);
                c.utime(&p, Some((a, m)))?;
                Ok(p)
            })
            .await)?;
        self.found(p);
        Ok(())
    }

    async fn set_attributes(&self, attributes: u32) -> SmbResult<()> {
        self.unprime();
        let (id, path, dir) = (self.id, self.path(), self.is_dir);
        let p = err(self
            .io
            .call(move |c| {
                let (p, _, have) = locate_stat(c, id, &path, ATTRS)?;
                // Only what differs is set.
                for a in attr_changes(attributes, dir) {
                    if !have.contains(&a) {
                        c.set_attr(&p, &a, true)?;
                    }
                }
                Ok(p)
            })
            .await)?;
        self.found(p);
        Ok(())
    }

    async fn truncate(&self, len: u64) -> SmbResult<()> {
        self.unprime();
        let Fd { fd, write } = self.fd().await?;
        if !write {
            return Err(SmbError::AccessDenied);
        }
        err(self.io.call(move |c| c.ftruncate(fd, len)).await)
    }

    async fn list_dir(&self, pattern: Option<&str>) -> SmbResult<Vec<DirEntry>> {
        self.unprime();
        if !self.is_dir {
            return Err(SmbError::NotADirectory);
        }
        let (id, path) = (self.id, self.path());
        let pattern = pattern.map(str::to_owned);
        let (p, out) = err(self
            .io
            .call(move |c| {
                // The directory, its entries and their attributes: one
                // request while the directory is where it was opened.
                let mut p = path;
                let mut l = c.read_dir_stat(&p, WANTED)?;
                if let Some(id) = id
                    && !is(id, &l.dir)
                    && let Ok(q) = c.path_from_file_id(id)
                    && q != p
                {
                    l = c.read_dir_stat(&q, WANTED)?;
                    p = q;
                }
                let ents: Vec<_> = l.entries.iter().map(|e| e.ent.clone()).collect();
                let shorts = short::of(&ents, l.case_sensitive);
                let mut out = Vec::new();
                for (e, short) in l.entries.into_iter().zip(shorts) {
                    // A pattern matches the name or the short name.
                    if pattern.as_deref().is_some_and(|pat| {
                        !matches(pat, &e.ent.name)
                            && !short.as_deref().is_some_and(|s| matches(pat, s))
                    }) {
                        continue;
                    }
                    // An entry gone or unreadable since the listing is left out.
                    if let Some((st, attrs)) = e.stat {
                        let mut info = file_info(&e.ent.name, &st, &attrs);
                        info.short_name = short;
                        out.push(DirEntry { info });
                    }
                }
                Ok((p, out))
            })
            .await)?;
        self.found(p);
        Ok(out)
    }

    async fn short_name(&self) -> SmbResult<Option<String>> {
        self.unprime();
        let (id, path, root) = (self.id, self.path(), self.root.clone());
        err(self
            .io
            .call(move |c| Ok(short::of_path(c, &locate(c, id, &path), &root)))
            .await)
    }

    async fn close(self: Box<Self>) -> SmbResult<()> {
        let fd = self
            .fd
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        match fd {
            Some(Fd { fd, .. }) => err(self.io.call(move |c| c.close(fd)).await),
            None => Ok(()),
        }
    }

    async fn link_count(&self) -> SmbResult<u32> {
        self.unprime();
        let (id, path) = (self.id, self.path());
        let (p, n) = err(self
            .io
            .call(move |c| {
                let (p, st, _) = locate_stat(c, id, &path, &[])?;
                Ok((p, st.nlink))
            })
            .await)?;
        self.found(p);
        Ok(n)
    }

    async fn dir_is_empty(&self) -> SmbResult<bool> {
        self.unprime();
        let (id, path) = (self.id, self.path());
        err(self
            .io
            .call(move |c| {
                Ok(c.read_dir(&locate(c, id, &path))?
                    .iter()
                    .all(|e| e.name == "." || e.name == ".."))
            })
            .await)
    }

    async fn lock_range(&self, offset: u64, len: u64, exclusive: bool) -> SmbResult<()> {
        self.unprime();
        let Some((start, len)) = ifs_range(offset, len) else {
            // Nothing the IFS can lock: the server's own table holds it.
            return Ok(());
        };
        let Fd { fd, write } = self.fd().await?;
        // An exclusive lock through a read-only open is taken shared.
        let kind = if exclusive && write {
            RangeKind::Write
        } else {
            RangeKind::Read
        };
        self.io
            .call(move |c| c.lock_range(fd, F_SETLK, kind, SEEK_SET, start, len))
            .await
            .map(|_| ())
            .map_err(|e| {
                if e == Errno::EAGAIN {
                    SmbError::Sharing
                } else {
                    smb_error(e)
                }
            })
    }

    async fn lease_limit(&self, lease: LeaseKey, want: u32) -> u32 {
        match &self.leases {
            Some(link) => {
                link.limit(self.id, self.path(), lease, want, &self.reserved)
                    .await
            }
            None => want,
        }
    }

    async fn watch(&self) -> SmbResult<Option<Box<dyn DirWatch>>> {
        self.unprime();
        let Some(link) = self.leases.as_ref().filter(|l| self.is_dir && l.is_on()) else {
            return Ok(None);
        };
        // A tree watch: the library gives tree watches what lies below.
        let (id, path) = (self.id, self.path());
        let Ok(watch) = self
            .io
            .call(move |c| c.watch_add(&locate(c, id, &path), true))
            .await
        else {
            return Ok(None);
        };
        Ok(Some(Box::new(
            link.watches().watcher(watch, self.io.clone()),
        )))
    }

    async fn unlock_range(&self, offset: u64, len: u64) -> SmbResult<()> {
        self.unprime();
        let Some((start, len)) = ifs_range(offset, len) else {
            return Ok(());
        };
        let fd = *self.fd.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(Fd { fd, .. }) = fd else {
            return Ok(());
        };
        err(self
            .io
            .call(move |c| {
                c.lock_range(fd, F_SETLK, RangeKind::Unlock, SEEK_SET, start, len)
                    .map(|_| ())
            })
            .await)
    }
}

impl Drop for IfsHandle {
    fn drop(&mut self) {
        if let Some(link) = &self.leases {
            let keys =
                std::mem::take(&mut *self.reserved.lock().unwrap_or_else(PoisonError::into_inner));
            if !keys.is_empty() {
                link.closed(self.id, &keys);
            }
        }
        if let Some(Fd { fd, .. }) = self
            .fd
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            self.io.post(move |c| {
                let _ = c.close(fd);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ifs_range;

    #[test]
    fn a_range_past_the_ifs_runs_to_the_end() {
        // Linux's flock over cifs: 0 for 2^63.
        assert_eq!(ifs_range(0, 1 << 63), Some((0, 0)));
        assert_eq!(ifs_range(10, u64::MAX), Some((10, 0)));
        assert_eq!(ifs_range(5, 100), Some((5, 100)));
        assert_eq!(ifs_range(5, 0), None);
        assert_eq!(ifs_range(1 << 63, 1), None);
    }
}
