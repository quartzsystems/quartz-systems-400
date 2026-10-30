// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One SFTP session's requests and handles.

use std::collections::HashMap;

use ifs::Errno;
use ifs::client::IfsClient;
use ifs::ops::flags::{O_APPEND, O_CREAT, O_EXCL, O_RDONLY, O_RDWR, O_TRUNC, O_WRONLY};
use ifs::ops::{OpenArgs, RenameMode};

use super::names::{absolute, attrs, longname};
use super::proto::{self, Attrs, pflags, status};
use crate::wire::{Reader, WireResult, Writer};

/// The most bytes one `READ` answers.
const MAX_READ: u32 = 256 * 1024;
/// Entries per `READDIR` answer.
const DIR_BATCH: usize = 100;

enum Handle {
    File { fd: i32, append: bool },
    Dir { path: String, stream: u64 },
}

/// An SFTP session.
pub struct Server<'a> {
    ifs: &'a dyn IfsClient,
    cwd: String,
    handles: HashMap<u32, Handle>,
    next: u32,
    started: bool,
}

type Reply = Result<Vec<u8>, (u32, String)>;

fn errno(e: Errno) -> (u32, String) {
    let code = match e {
        Errno::ENOENT | Errno::ENOTDIR => status::NO_SUCH_FILE,
        Errno::EACCES | Errno::EPERM => status::PERMISSION_DENIED,
        Errno::ENOTSUP | Errno::ENOSYS => status::OP_UNSUPPORTED,
        _ => status::FAILURE,
    };
    (code, e.text().to_string())
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
}

fn status_text(code: u32) -> &'static str {
    match code {
        status::OK => "Success",
        status::EOF => "End of file",
        status::NO_SUCH_FILE => "No such file",
        status::PERMISSION_DENIED => "Permission denied",
        status::BAD_MESSAGE => "Bad message",
        status::OP_UNSUPPORTED => "Operation unsupported",
        _ => "Failure",
    }
}

impl<'a> Server<'a> {
    /// A session over `ifs`, relative paths from `home`.
    #[must_use]
    pub fn new(ifs: &'a dyn IfsClient, home: &str) -> Self {
        let cwd = if home.is_empty() || ifs.stat(home, true).is_err() {
            "/".to_string()
        } else {
            home.to_string()
        };
        Self {
            ifs,
            cwd,
            handles: HashMap::new(),
            next: 1,
            started: false,
        }
    }

    fn path(&self, p: &str) -> String {
        absolute(&self.cwd, p)
    }

    fn new_handle(&mut self, h: Handle) -> Vec<u8> {
        let id = self.next;
        self.next = self.next.wrapping_add(1);
        self.handles.insert(id, h);
        id.to_be_bytes().to_vec()
    }

    fn handle_id(raw: &[u8]) -> Option<u32> {
        Some(u32::from_be_bytes(raw.try_into().ok()?))
    }

    /// Close every handle still open.
    pub fn close_all(&mut self) {
        for (_, h) in self.handles.drain() {
            match h {
                Handle::File { fd, .. } => {
                    let _ = self.ifs.close(fd);
                }
                Handle::Dir { stream, .. } => {
                    let _ = self.ifs.closedir(stream);
                }
            }
        }
    }

    /// Answer one request; `None` ends the session (no `INIT` first).
    pub fn handle(&mut self, body: &[u8]) -> Option<Vec<u8>> {
        let mut r = Reader::new(body);
        let kind = r.u8().ok()?;
        if kind == proto::INIT {
            self.started = true;
            let mut w = Writer::msg(proto::VERSION);
            w.u32(3);
            for (name, ver) in [
                ("posix-rename@openssh.com", "1"),
                ("hardlink@openssh.com", "1"),
                ("fsync@openssh.com", "1"),
            ] {
                w.str(name).str(ver);
            }
            return Some(w.finish());
        }
        if !self.started {
            return None;
        }
        let Ok(id) = r.u32() else {
            return Some(Self::status(0, status::BAD_MESSAGE, ""));
        };
        let reply = match self.dispatch(kind, id, &mut r) {
            Ok(Ok(v)) => v,
            Ok(Err((code, text))) => Self::status(id, code, &text),
            Err(_) => Self::status(id, status::BAD_MESSAGE, ""),
        };
        Some(reply)
    }

    fn status(id: u32, code: u32, text: &str) -> Vec<u8> {
        let mut w = Writer::msg(proto::STATUS);
        let text = if text.is_empty() { status_text(code) } else { text };
        w.u32(id).u32(code).str(text).str("en");
        w.finish()
    }

    #[allow(clippy::unnecessary_wraps, clippy::unused_self)]
    fn ok(&self, id: u32) -> Reply {
        Ok(Self::status(id, status::OK, ""))
    }

    fn attrs_reply(id: u32, a: &Attrs) -> Vec<u8> {
        let mut w = Writer::msg(proto::ATTRS);
        w.u32(id);
        a.encode(&mut w);
        w.finish()
    }

    fn name_reply(id: u32, entries: &[(String, String, Attrs)]) -> Vec<u8> {
        let mut w = Writer::msg(proto::NAME);
        w.u32(id).u32(u32::try_from(entries.len()).unwrap_or(0));
        for (n, l, a) in entries {
            w.str(n).str(l);
            a.encode(&mut w);
        }
        w.finish()
    }

    #[allow(clippy::too_many_lines)]
    fn dispatch(&mut self, kind: u8, id: u32, r: &mut Reader<'_>) -> WireResult<Reply> {
        let ifs = self.ifs;
        Ok(match kind {
            proto::OPEN => {
                let path = self.path(r.str()?);
                let pf = r.u32()?;
                let a = Attrs::decode(r)?;
                let mut oflag = match (pf & pflags::READ != 0, pf & pflags::WRITE != 0) {
                    (true, true) => O_RDWR,
                    (false, true) => O_WRONLY,
                    _ => O_RDONLY,
                };
                if pf & pflags::CREAT != 0 {
                    oflag |= O_CREAT;
                }
                if pf & pflags::TRUNC != 0 {
                    oflag |= O_TRUNC;
                }
                if pf & pflags::EXCL != 0 {
                    oflag |= O_EXCL;
                }
                let append = pf & pflags::APPEND != 0;
                if append {
                    oflag |= O_APPEND;
                }
                let mode = a.permissions.map_or(0o644, |p| p & 0o7777);
                match ifs.open(&path, OpenArgs::new(oflag).mode(mode)) {
                    Ok(fd) => {
                        let h = self.new_handle(Handle::File { fd, append });
                        let mut w = Writer::msg(proto::HANDLE);
                        w.u32(id).bytes(&h);
                        Ok(w.finish())
                    }
                    Err(e) => Err(errno(e)),
                }
            }
            proto::CLOSE => {
                let h = Self::handle_id(r.bytes()?);
                match h.and_then(|h| self.handles.remove(&h)) {
                    Some(Handle::File { fd, .. }) => ifs.close(fd).map_err(errno).and_then(|()| self.ok(id)),
                    Some(Handle::Dir { stream, .. }) => {
                        ifs.closedir(stream).map_err(errno).and_then(|()| self.ok(id))
                    }
                    None => Err((status::FAILURE, "The handle is not valid.".into())),
                }
            }
            proto::READ => {
                let h = Self::handle_id(r.bytes()?);
                let off = r.u64()?;
                let len = r.u32()?.min(MAX_READ) as usize;
                match h.and_then(|h| self.handles.get(&h)) {
                    Some(Handle::File { fd, .. }) => match ifs.pread(*fd, len, off) {
                        Ok(d) if d.is_empty() => Err((status::EOF, String::new())),
                        Ok(d) => {
                            let mut w = Writer::msg(proto::DATA);
                            w.u32(id).bytes(&d);
                            Ok(w.finish())
                        }
                        Err(e) => Err(errno(e)),
                    },
                    _ => Err((status::FAILURE, "The handle is not valid.".into())),
                }
            }
            proto::WRITE => {
                let h = Self::handle_id(r.bytes()?);
                let off = r.u64()?;
                let data = r.bytes()?;
                match h.and_then(|h| self.handles.get(&h)) {
                    Some(Handle::File { fd, append }) => {
                        let (fd, append) = (*fd, *append);
                        let mut done = 0usize;
                        let mut res = Ok(());
                        while done < data.len() {
                            let w = if append {
                                ifs.write(fd, &data[done..])
                            } else {
                                ifs.pwrite(fd, &data[done..], off + done as u64)
                            };
                            match w {
                                Ok(0) => {
                                    res = Err(Errno::EIO);
                                    break;
                                }
                                Ok(n) => done += n,
                                Err(e) => {
                                    res = Err(e);
                                    break;
                                }
                            }
                        }
                        res.map_err(errno).and_then(|()| self.ok(id))
                    }
                    _ => Err((status::FAILURE, "The handle is not valid.".into())),
                }
            }
            proto::LSTAT | proto::STAT => {
                let path = self.path(r.str()?);
                match ifs.stat(&path, kind == proto::STAT) {
                    Ok(st) => Ok(Self::attrs_reply(id, &attrs(&st))),
                    Err(e) => Err(errno(e)),
                }
            }
            proto::FSTAT => {
                let h = Self::handle_id(r.bytes()?);
                match h.and_then(|h| self.handles.get(&h)) {
                    Some(Handle::File { fd, .. }) => match ifs.fstat(*fd) {
                        Ok(st) => Ok(Self::attrs_reply(id, &attrs(&st))),
                        Err(e) => Err(errno(e)),
                    },
                    Some(Handle::Dir { path, .. }) => match ifs.stat(path, true) {
                        Ok(st) => Ok(Self::attrs_reply(id, &attrs(&st))),
                        Err(e) => Err(errno(e)),
                    },
                    None => Err((status::FAILURE, "The handle is not valid.".into())),
                }
            }
            proto::SETSTAT => {
                let path = self.path(r.str()?);
                let a = Attrs::decode(r)?;
                self.setstat(&path, None, &a)
                    .map_err(errno)
                    .and_then(|()| self.ok(id))
            }
            proto::FSETSTAT => {
                let h = Self::handle_id(r.bytes()?);
                let a = Attrs::decode(r)?;
                let target = match h.and_then(|h| self.handles.get(&h)) {
                    Some(Handle::File { fd, .. }) => ifs.fd_path(*fd).map(|p| (p, Some(*fd))),
                    Some(Handle::Dir { path, .. }) => Ok((path.clone(), None)),
                    None => Err(Errno::EBADF),
                };
                target
                    .and_then(|(p, fd)| self.setstat(&p, fd, &a))
                    .map_err(errno)
                    .and_then(|()| self.ok(id))
            }
            proto::OPENDIR => {
                let path = self.path(r.str()?);
                match ifs.opendir(&path) {
                    Ok(stream) => {
                        let h = self.new_handle(Handle::Dir { path, stream });
                        let mut w = Writer::msg(proto::HANDLE);
                        w.u32(id).bytes(&h);
                        Ok(w.finish())
                    }
                    Err(e) => Err(errno(e)),
                }
            }
            proto::READDIR => {
                let h = Self::handle_id(r.bytes()?);
                match h.and_then(|h| self.handles.get(&h)) {
                    Some(Handle::Dir { path, stream }) => match ifs.readdir(*stream, DIR_BATCH) {
                        Ok(ents) if ents.is_empty() => Err((status::EOF, String::new())),
                        Ok(ents) => {
                            let now = now();
                            let list: Vec<_> = ents
                                .into_iter()
                                .map(|e| {
                                    let full = absolute(path, &e.name);
                                    match ifs.stat(&full, false) {
                                        Ok(st) => {
                                            let l = longname(&e.name, &st, now);
                                            (e.name, l, attrs(&st))
                                        }
                                        Err(_) => (e.name.clone(), e.name, Attrs::default()),
                                    }
                                })
                                .collect();
                            Ok(Self::name_reply(id, &list))
                        }
                        Err(e) => Err(errno(e)),
                    },
                    _ => Err((status::FAILURE, "The handle is not valid.".into())),
                }
            }
            proto::REMOVE => {
                let path = self.path(r.str()?);
                ifs.unlink(&path).map_err(errno).and_then(|()| self.ok(id))
            }
            proto::MKDIR => {
                let path = self.path(r.str()?);
                let a = Attrs::decode(r)?;
                let mode = a.permissions.map_or(0o755, |p| p & 0o7777);
                ifs.mkdir(&path, mode).map_err(errno).and_then(|()| self.ok(id))
            }
            proto::RMDIR => {
                let path = self.path(r.str()?);
                ifs.rmdir(&path).map_err(errno).and_then(|()| self.ok(id))
            }
            proto::REALPATH => {
                let path = self.path(r.str()?);
                Ok(Self::name_reply(id, &[(path.clone(), path, Attrs::default())]))
            }
            proto::RENAME => {
                let from = self.path(r.str()?);
                let to = self.path(r.str()?);
                ifs.rename(&from, &to, RenameMode::Keep)
                    .map_err(errno)
                    .and_then(|()| self.ok(id))
            }
            proto::READLINK => {
                let path = self.path(r.str()?);
                match ifs.readlink(&path) {
                    Ok(t) => Ok(Self::name_reply(id, &[(t.clone(), t, Attrs::default())])),
                    Err(e) => Err(errno(e)),
                }
            }
            proto::SYMLINK => {
                // OpenSSH sends the target first, then the link's path.
                let target = r.str()?.to_string();
                let link = self.path(r.str()?);
                ifs.symlink(&target, &link)
                    .map_err(errno)
                    .and_then(|()| self.ok(id))
            }
            proto::EXTENDED => self.extended(id, r)?,
            _ => Err((status::OP_UNSUPPORTED, String::new())),
        })
    }

    fn extended(&mut self, id: u32, r: &mut Reader<'_>) -> WireResult<Reply> {
        let ifs = self.ifs;
        Ok(match r.str()? {
            "posix-rename@openssh.com" => {
                let from = self.path(r.str()?);
                let to = self.path(r.str()?);
                ifs.rename(&from, &to, RenameMode::Unlink)
                    .map_err(errno)
                    .and_then(|()| self.ok(id))
            }
            "hardlink@openssh.com" => {
                let from = self.path(r.str()?);
                let to = self.path(r.str()?);
                ifs.link(&from, &to)
                    .map_err(errno)
                    .and_then(|()| self.ok(id))
            }
            "fsync@openssh.com" => {
                let h = Self::handle_id(r.bytes()?);
                match h.and_then(|h| self.handles.get(&h)) {
                    Some(Handle::File { fd, .. }) => ifs
                        .fsync(*fd, false)
                        .map_err(errno)
                        .and_then(|()| self.ok(id)),
                    _ => Err((status::FAILURE, "The handle is not valid.".into())),
                }
            }
            _ => Err((status::OP_UNSUPPORTED, String::new())),
        })
    }

    fn setstat(&self, path: &str, fd: Option<i32>, a: &Attrs) -> ifs::Result<()> {
        let ifs = self.ifs;
        if let Some(size) = a.size {
            if let Some(fd) = fd {
                ifs.ftruncate(fd, size)?;
            } else {
                let fd = ifs.open(path, OpenArgs::new(O_WRONLY))?;
                let r = ifs.ftruncate(fd, size);
                let _ = ifs.close(fd);
                r?;
            }
        }
        if let Some(p) = a.permissions {
            ifs.chmod(path, p & 0o7777)?;
        }
        if let Some((u, g)) = a.uid_gid {
            ifs.chown(path, u, g)?;
        }
        if let Some((at, mt)) = a.times {
            let ns = |s: u32| i64::from(s) * 1_000_000_000;
            ifs.utime(path, Some((ns(at), ns(mt))))?;
        }
        Ok(())
    }
}
