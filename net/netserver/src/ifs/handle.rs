// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One open of a file or directory: an IFS descriptor for a file's data
//! (opened at CREATE when the open has data access, else on first use),
//! and the object's file ID, which finds it again after a rename.

use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use bytes::Bytes;
use ifs::Errno;
use ifs::client::{CHUNK, IfsClient};
use ifs::lock::range::RangeKind;
use ifs::ops::OpenArgs;
use ifs::ops::audit::FileId;
use ifs::ops::flags::{F_SETLK, O_RDONLY, O_SHARE_RDWR, SEEK_SET};
use smb_server::{DirEntry, DirWatch, FileInfo, FileTimes, Handle, LeaseKey, SmbError, SmbResult};

use super::info::{attr_changes, file_info, nanos, smb_error, stat_path};
use super::lease::LeaseLink;
use super::pattern::matches;
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
}

/// Where the object is now: the path opened while it still names the
/// object (one `stat`), else found by file ID (a walk of its file
/// system, only after a rename), else the path opened.
pub(super) fn locate(c: &dyn IfsClient, id: Option<FileId>, path: &str) -> String {
    let Some(id) = id else {
        return path.to_owned();
    };
    let same = |st: &ifs::ops::Stat| {
        st.ino == id.ino && st.dev == id.fs && (id.generation == 0 || st.ino_gen == id.generation)
    };
    if c.stat(path, false).is_ok_and(|st| same(&st)) {
        return path.to_owned();
    }
    c.path_from_file_id(id).unwrap_or_else(|_| path.to_owned())
}

/// The last component of `path`, or empty for the share root.
fn leaf<'a>(path: &'a str, root: &str) -> &'a str {
    if path == root {
        ""
    } else {
        path.rsplit('/').next().unwrap_or("")
    }
}

fn err<T>(r: ifs::Result<T>) -> SmbResult<T> {
    r.map_err(smb_error)
}

impl IfsHandle {
    fn path(&self) -> String {
        self.path
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
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
        let Fd { fd, .. } = self.fd().await?;
        let data = err(self
            .io
            .call(move |c| {
                let want = usize::try_from(len).unwrap_or(usize::MAX);
                let mut out = Vec::new();
                while out.len() < want {
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
        let mut data = data;
        if let Some(x) = &self.xlate {
            x.to_client(&mut data);
        }
        Ok(Bytes::from(data))
    }

    async fn write(&self, offset: u64, data: &[u8]) -> SmbResult<u32> {
        self.write_owned(offset, data.to_vec()).await
    }

    async fn write_owned(&self, offset: u64, mut data: Vec<u8>) -> SmbResult<u32> {
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
        let fd = *self.fd.lock().unwrap_or_else(PoisonError::into_inner);
        match fd {
            Some(Fd { fd, write: true }) => err(self.io.call(move |c| c.fsync(fd, false)).await),
            _ => Ok(()),
        }
    }

    async fn stat(&self) -> SmbResult<FileInfo> {
        let (id, path, root) = (self.id, self.path(), self.root.clone());
        err(self
            .io
            .call(move |c| {
                let p = locate(c, id, &path);
                let (st, attrs) = stat_path(c, &p)?;
                Ok(file_info(leaf(&p, &root), &st, &attrs))
            })
            .await)
    }

    async fn set_times(&self, times: FileTimes) -> SmbResult<()> {
        if times.last_access_time.is_none() && times.last_write_time.is_none() {
            // The IFS keeps creation and change times itself.
            return Ok(());
        }
        let (id, path) = (self.id, self.path());
        err(self
            .io
            .call(move |c| {
                let p = locate(c, id, &path);
                let st = c.stat(&p, true)?;
                let a = times.last_access_time.map_or(st.atime, nanos);
                let m = times.last_write_time.map_or(st.mtime, nanos);
                c.utime(&p, Some((a, m)))
            })
            .await)
    }

    async fn set_attributes(&self, attributes: u32) -> SmbResult<()> {
        let (id, path, dir) = (self.id, self.path(), self.is_dir);
        err(self
            .io
            .call(move |c| {
                let p = locate(c, id, &path);
                for a in attr_changes(attributes, dir) {
                    c.set_attr(&p, &a, true)?;
                }
                Ok(())
            })
            .await)
    }

    async fn truncate(&self, len: u64) -> SmbResult<()> {
        let Fd { fd, write } = self.fd().await?;
        if !write {
            return Err(SmbError::AccessDenied);
        }
        err(self.io.call(move |c| c.ftruncate(fd, len)).await)
    }

    async fn list_dir(&self, pattern: Option<&str>) -> SmbResult<Vec<DirEntry>> {
        if !self.is_dir {
            return Err(SmbError::NotADirectory);
        }
        let (id, path) = (self.id, self.path());
        let pattern = pattern.map(str::to_owned);
        err(self
            .io
            .call(move |c| {
                let p = locate(c, id, &path);
                let mut out = Vec::new();
                for e in c.read_dir(&p)? {
                    if e.name == "." || e.name == ".." {
                        continue;
                    }
                    if pattern.as_deref().is_some_and(|pat| !matches(pat, &e.name)) {
                        continue;
                    }
                    let full = if p.ends_with('/') {
                        format!("{p}{}", e.name)
                    } else {
                        format!("{p}/{}", e.name)
                    };
                    // An entry gone or unreadable since the listing is left out.
                    if let Ok((st, attrs)) = stat_path(c, &full) {
                        out.push(DirEntry {
                            info: file_info(&e.name, &st, &attrs),
                        });
                    }
                }
                Ok(out)
            })
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
        let (id, path) = (self.id, self.path());
        err(self
            .io
            .call(move |c| Ok(c.stat(&locate(c, id, &path), true)?.nlink))
            .await)
    }

    async fn dir_is_empty(&self) -> SmbResult<bool> {
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
        let (Ok(start), Ok(len)) = (i64::try_from(offset), i64::try_from(len)) else {
            // Past what the IFS can lock: the server's own table holds it.
            return Ok(());
        };
        if len == 0 {
            return Ok(());
        }
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
        let (Ok(start), Ok(len)) = (i64::try_from(offset), i64::try_from(len)) else {
            return Ok(());
        };
        let fd = *self.fd.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(Fd { fd, .. }) = fd else {
            return Ok(());
        };
        if len == 0 {
            return Ok(());
        }
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
