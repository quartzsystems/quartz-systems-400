// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `accessweb::env::files::Files` over the IFS of the storage service,
//! with the job swapped to the signed-on user for every call, so the
//! service's authority checks are the user's (as the file host server
//! serves them). Errors are the `CPFA0xx` messages of the `errno`.
//! A file is written in place when it exists (it keeps its owner,
//! authority and CCSID), else made with the CCSID given.

use std::collections::HashMap;
use std::sync::Mutex;

use accessweb::env::files::{AuthChange, Entry, Files, Kind, Props, Share};
use accessweb::transfer::Queue;
use ifs::client::{IfsClient, write_all};
use ifs::errno::Errno;
use ifs::ops::flags::{
    O_CCSID, O_CREAT, O_EXCL, O_RDONLY, O_TRUNC, O_WRONLY, S_IFDIR, S_IFLNK, S_IFMT, S_IFREG,
};
use ifs::ops::{ChgAut, CopyOpts, OpenArgs, RenameMode, Stat, WalkOpts};
use qsweb::msg::{Kind as MsgKind, Message};
use sec::api::{Answer, Query};
use sec::profile::UsrPrf;
use stg::remote::RemoteIfs;

use super::Link;
use crate::work::web::bridge::Main;

/// Entries a size or count walk looks at before it gives up.
const WALK_MAX: usize = 20_000;

/// The IFS as the signed-on users see it.
pub struct LiveFiles {
    /// The job's thread.
    pub link: Link,
    /// The transfer queues.
    pub queue: Queue,
    /// Home directories by user.
    homes: Mutex<HashMap<String, String>>,
}

impl LiveFiles {
    /// Over the job's thread.
    pub fn new(link: Link) -> Self {
        Self {
            link,
            queue: Queue::default(),
            homes: Mutex::new(HashMap::new()),
        }
    }

    /// `f` with the IFS as `user`; `CPF22E9` when the user is not
    /// signed on here.
    fn with<T: Send + 'static>(
        &self,
        user: &str,
        f: impl FnOnce(&RemoteIfs) -> Result<T, Message> + Send + 'static,
    ) -> Result<T, Message> {
        self.link
            .read(user, move |m| Some(f(&RemoteIfs::new(m.link.clone()))))
            .unwrap_or_else(|| Err(Message::new(MsgKind::Error, "CPF22E9", "Sign on again.")))
    }
}

/// The message of `e` for `path`.
fn msg(e: Errno, path: &str) -> Message {
    let m = Message::new(MsgKind::Error, e.command_message(), e.text());
    if e.command_message_names_object() {
        m.with_data(&[path])
    } else {
        m
    }
}

fn secs(n: i64) -> u64 {
    u64::try_from(n / 1_000_000_000).unwrap_or(0)
}

/// An entry of `path` from its `lstat`; a link's target looked at.
fn entry_of(c: &RemoteIfs, path: &str, s: &Stat) -> Entry {
    let kind = match s.mode & S_IFMT {
        S_IFDIR => Kind::Dir,
        S_IFREG => Kind::File,
        S_IFLNK => {
            let target = c.readlink(path).unwrap_or_default();
            let to = c.stat(path, true);
            Kind::Link {
                target,
                dir: to.as_ref().is_ok_and(|t| t.mode & S_IFMT == S_IFDIR),
                broken: to.is_err(),
            }
        }
        _ => Kind::Other(s.objtype.clone()),
    };
    let file = kind == Kind::File;
    Entry {
        name: path.rsplit('/').next().unwrap_or(path).to_string(),
        kind,
        size: if file { s.size } else { 0 },
        modified: secs(s.mtime),
        owner: s.owner.clone(),
        mode: s.mode & 0o777,
        ccsid: if file { s.ccsid } else { 0 },
    }
}

fn join(dir: &str, name: &str) -> String {
    if dir == "/" {
        format!("/{name}")
    } else {
        format!("{}/{name}", dir.trim_end_matches('/'))
    }
}

/// Delete `path` from the inside out: the entries deleted, or where it
/// stopped.
fn delete_tree(c: &RemoteIfs, path: &str, done: &mut u64) -> Result<(), (String, Message)> {
    let s = c
        .stat(path, false)
        .map_err(|e| (path.to_string(), msg(e, path)))?;
    if s.mode & S_IFMT == S_IFDIR {
        let kids = c
            .read_dir(path)
            .map_err(|e| (path.to_string(), msg(e, path)))?;
        for d in kids {
            if d.name != "." && d.name != ".." {
                delete_tree(c, &join(path, &d.name), done)?;
            }
        }
        c.rmdir(path)
            .map_err(|e| (path.to_string(), msg(e, path)))?;
    } else {
        c.unlink(path)
            .map_err(|e| (path.to_string(), msg(e, path)))?;
    }
    *done += 1;
    Ok(())
}

impl Files for LiveFiles {
    fn home(&self, user: &str) -> String {
        if let Some(h) = self.homes.lock().ok().and_then(|m| m.get(user).cloned()) {
            return h;
        }
        let u = user.to_string();
        let found: Option<String> = self.link.read(user, move |m: &mut Main| {
            match m.sec(Query::Profile { user: u }) {
                Ok(Answer::Profile { data }) => UsrPrf::decode(&data).ok().map(|p| p.homedir),
                _ => None,
            }
        });
        let h = found
            .filter(|h| h.starts_with('/'))
            .unwrap_or_else(|| format!("/home/{user}"));
        if let Ok(mut m) = self.homes.lock() {
            m.insert(user.to_string(), h.clone());
        }
        h
    }

    fn list(&self, user: &str, dir: &str) -> Result<Vec<Entry>, Message> {
        let d = dir.to_string();
        self.with(user, move |c| {
            let names = c.read_dir(&d).map_err(|e| msg(e, &d))?;
            Ok(names
                .into_iter()
                .filter(|n| n.name != "." && n.name != "..")
                .filter_map(|n| {
                    let p = join(&d, &n.name);
                    c.stat(&p, false).ok().map(|s| entry_of(c, &p, &s))
                })
                .collect())
        })
    }

    fn entry(&self, user: &str, path: &str) -> Result<Entry, Message> {
        let p = path.to_string();
        self.with(user, move |c| {
            c.stat(&p, false)
                .map(|s| entry_of(c, &p, &s))
                .map_err(|e| msg(e, &p))
        })
    }

    fn props(&self, user: &str, path: &str) -> Result<Props, Message> {
        let p = path.to_string();
        self.with(user, move |c| {
            let s = c.stat(&p, false).map_err(|e| msg(e, &p))?;
            let autl = c
                .dspaut(&p)
                .ok()
                .and_then(|a| a.autl)
                .unwrap_or_else(|| "*NONE".into());
            let typ = match s.mode & S_IFMT {
                S_IFDIR => format!(
                    "Directory ({})",
                    if s.objtype.is_empty() {
                        "*DIR"
                    } else {
                        &s.objtype
                    }
                ),
                S_IFLNK => "Symbolic link (*SYMLNK)".into(),
                S_IFREG => format!(
                    "Stream file ({})",
                    if s.objtype.is_empty() {
                        "*STMF"
                    } else {
                        &s.objtype
                    }
                ),
                _ => s.objtype.clone(),
            };
            Ok(Props {
                entry: entry_of(c, &p, &s),
                typ,
                created: secs(s.crtime),
                accessed: secs(s.atime),
                changes: s.change,
                group: s.group.clone().unwrap_or_else(|| "*NONE".into()),
                autl,
            })
        })
    }

    fn read(&self, user: &str, path: &str, offset: u64, len: u64) -> Result<Vec<u8>, Message> {
        let p = path.to_string();
        self.with(user, move |c| {
            let fd = c
                .open(&p, OpenArgs::new(O_RDONLY))
                .map_err(|e| msg(e, &p))?;
            let mut out = Vec::new();
            let mut at = offset;
            let r = loop {
                let want = usize::try_from(len.saturating_sub(out.len() as u64))
                    .unwrap_or(usize::MAX)
                    .min(256 * 1024);
                if want == 0 {
                    break Ok(());
                }
                match c.pread(fd, want, at) {
                    Ok(b) if b.is_empty() => break Ok(()),
                    Ok(b) => {
                        at += b.len() as u64;
                        out.extend_from_slice(&b);
                    }
                    Err(e) => break Err(msg(e, &p)),
                }
            };
            let _ = c.close(fd);
            r.map(|()| out)
        })
    }

    fn write(&self, user: &str, path: &str, data: &[u8], ccsid: u16) -> Result<(), Message> {
        let (p, d) = (path.to_string(), data.to_vec());
        self.with(user, move |c| {
            let exists = c.stat(&p, true).is_ok();
            let args = if exists {
                OpenArgs::new(O_WRONLY | O_TRUNC)
            } else {
                let mut a = OpenArgs::new(O_WRONLY | O_CREAT | O_EXCL | O_CCSID).mode(0o644);
                a.conversion_id = u32::from(ccsid);
                a
            };
            let fd = c.open(&p, args).map_err(|e| msg(e, &p))?;
            let w = write_all(c, fd, &d).map_err(|e| msg(e, &p));
            let closed = c.close(fd).map_err(|e| msg(e, &p));
            w.and(closed)
        })
    }

    fn mkdir(&self, user: &str, path: &str) -> Result<(), Message> {
        let p = path.to_string();
        self.with(user, move |c| c.mkdir(&p, 0o755).map_err(|e| msg(e, &p)))
    }

    fn rename(&self, user: &str, from: &str, to: &str) -> Result<(), Message> {
        let (f, t) = (from.to_string(), to.to_string());
        self.with(user, move |c| {
            c.rename(&f, &t, RenameMode::Keep).map_err(|e| msg(e, &t))
        })
    }

    fn copy(&self, user: &str, from: &str, to: &str) -> Result<(), Message> {
        let (f, t) = (from.to_string(), to.to_string());
        self.with(user, move |c| {
            let opts = CopyOpts {
                subtree: true,
                replace: false,
                to_ccsid: None,
                keep_owner: false,
            };
            c.copy(&f, &t, opts).map_err(|e| msg(e, &t))
        })
    }

    fn delete(&self, user: &str, path: &str) -> Result<u64, (String, Message, u64)> {
        let p = path.to_string();
        let r = self.with(user, move |c| {
            let mut done = 0;
            Ok(match delete_tree(c, &p, &mut done) {
                Ok(()) => Ok(done),
                Err((at, m)) => Err((at, m, done)),
            })
        });
        match r {
            Ok(x) => x,
            Err(m) => Err((path.to_string(), m, 0)),
        }
    }

    fn count(&self, user: &str, path: &str) -> Result<u64, Message> {
        let p = path.to_string();
        self.with(user, move |c| {
            let s = c.stat(&p, false).map_err(|e| msg(e, &p))?;
            if s.mode & S_IFMT != S_IFDIR {
                return Ok(1);
            }
            let opts = WalkOpts {
                follow: false,
                one_fs: true,
            };
            c.walk(&p, opts, WALK_MAX)
                .map(|v| v.len() as u64)
                .map_err(|e| msg(e, &p))
        })
    }

    fn set_auth(&self, user: &str, path: &str, change: &AuthChange) -> Result<(), Message> {
        let (p, mode, autl) = (path.to_string(), change.mode, change.autl.clone());
        self.with(user, move |c| {
            c.chmod(&p, mode & 0o777).map_err(|e| msg(e, &p))?;
            let now = c
                .dspaut(&p)
                .ok()
                .and_then(|a| a.autl)
                .unwrap_or_else(|| "*NONE".into());
            if now != autl {
                let list = if autl == "*NONE" { None } else { Some(autl) };
                let ca = ChgAut {
                    autl: Some(list),
                    ..ChgAut::default()
                };
                c.chgaut(&p, &ca).map_err(|e| msg(e, &p))?;
            }
            Ok(())
        })
    }

    fn shares(&self, user: &str) -> Vec<Share> {
        // QS/400 serves no file shares: My files is the home directory.
        let home = self.home(user);
        let h = home.clone();
        let bytes = self
            .with(user, move |c| {
                let opts = WalkOpts {
                    follow: false,
                    one_fs: true,
                };
                let all = c.walk(&h, opts, WALK_MAX).map_err(|e| msg(e, &h))?;
                Ok((all.len() < WALK_MAX).then(|| {
                    all.iter()
                        .filter_map(|w| w.stat.as_ref().ok())
                        .filter(|s| s.mode & S_IFMT == S_IFREG)
                        .map(|s| s.size)
                        .sum::<u64>()
                }))
            })
            .ok()
            .flatten();
        vec![Share {
            name: "Home".into(),
            path: home,
            typ: "Directory".into(),
            used: bytes,
        }]
    }

    fn queue(&self) -> &Queue {
        &self.queue
    }
}
