// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Opens of remote files and their data: close-to-open revalidation,
//! the server's access check, truncation and a stateful protocol's
//! open before anything changes here; reads and writes through the
//! mount's cache; `fsync` and the last `close` write back (and answer
//! the first write-back error); a file removed while open goes when its
//! last open does; byte-range locks are the server's.

use std::sync::Arc;

use super::super::flags as f;
use super::super::{Ctx, Ifs, Open, OpenKind, Rest, Stat};
use crate::backend::Backend;
use crate::errno::{Errno, Result};
use crate::lock::Owner;
use crate::lock::range::{RangeKind, RangeLock};
use crate::remote::access as a;
use crate::remote::request;
use crate::remote::work::{Answer, Mutation};
use crate::remote::{Cred, LockKind, LockOwner, OpenToken, RHandle, RemoteMount, SetAttr, Share};
use crate::resolve::{Node, Sys};

/// What an open of a remote file holds.
pub(crate) struct RemoteOpen {
    pub mount: Arc<RemoteMount>,
    pub h: RHandle,
    pub cred: Cred,
    pub token: OpenToken,
    /// It took byte-range locks (released at close).
    pub locked: bool,
    pub owner: Owner,
}

impl RemoteOpen {
    /// The size the caller sees.
    pub(crate) fn size(&self) -> Result<u64> {
        let m = &self.mount;
        Ok(m.settle(m.attr(&self.h, &self.cred))?.size)
    }

    /// The size as the server has it now (with writes not yet sent):
    /// where `O_APPEND` writes go.
    pub(crate) fn size_now(&self) -> Result<u64> {
        let m = &self.mount;
        Ok(m.settle(m.attr_now(&self.h, &self.cred))?.size)
    }

    pub(crate) fn read(&self, buf: &mut [u8], pos: u64) -> Result<usize> {
        let m = &self.mount;
        let size = self.size()?;
        let data = m.settle(m.read(&self.h, &self.cred, pos, buf.len(), size))?;
        buf[..data.len()].copy_from_slice(&data);
        Ok(data.len())
    }

    /// Write into the cache; `sync` (`O_SYNC`, `O_DSYNC`) waits for the
    /// write-back, the data taken once whatever the runs.
    pub(crate) fn write(&self, data: &[u8], pos: u64, sync: bool) -> Result<usize> {
        let m = &self.mount;
        if m.info().read_only {
            return Err(Errno::EROFS);
        }
        let key = request::occurrence("remote-write");
        if request::step::<bool>(&key).is_none() {
            let size = self.size()?;
            m.settle(m.write(&self.h, &self.cred, pos, data, size))?;
            if sync {
                request::set_step(&key, true);
            }
        }
        if sync {
            m.settle(m.mutate(Mutation::Flush { h: self.h.clone() }, &self.cred))?;
        }
        Ok(data.len())
    }

    /// `fsync`: everything written back and committed.
    pub(crate) fn sync(&self) -> Result<()> {
        let m = &self.mount;
        m.settle(m.mutate(Mutation::Flush { h: self.h.clone() }, &self.cred))
            .map(|_| ())
    }

    /// `ftruncate`.
    pub(crate) fn truncate(&self, len: u64) -> Result<()> {
        let m = &self.mount;
        if m.info().read_only {
            return Err(Errno::EROFS);
        }
        let set = SetAttr {
            size: Some(len),
            ..SetAttr::default()
        };
        m.settle(m.mutate(
            Mutation::Setattr {
                h: self.h.clone(),
                set,
            },
            &self.cred,
        ))
        .map(|_| ())
    }

    /// After the open is gone: what is left is sent without anyone
    /// waiting (write-back at a job's end, locks, the protocol's close).
    pub(crate) fn finish(&self) {
        let m = &self.mount;
        if m.has_dirty(&self.h) {
            m.background(Mutation::Flush { h: self.h.clone() }, &self.cred);
        }
        if self.locked {
            m.background(
                Mutation::Unlock {
                    h: self.h.clone(),
                    owner: LockOwner(self.owner.0),
                    off: 0,
                    len: 0,
                },
                &self.cred,
            );
        }
        if m.info().stateful_open {
            m.background(
                Mutation::Close {
                    h: self.h.clone(),
                    token: self.token.clone(),
                },
                &self.cred,
            );
        }
    }
}

impl<B: Backend> Ifs<B> {
    /// What opening remote `node` needs before anything changes here:
    /// fresh attributes (close-to-open), the server's access answer,
    /// `O_TRUNC`, the protocol's open. `None` for a directory.
    pub(crate) fn remote_open_prep(
        sys: &Sys<'_, B>,
        node: &Node,
        oflag: u32,
        created: bool,
        owner: Owner,
    ) -> Result<Option<RemoteOpen>> {
        let Node::Remote { fs, handle, attr } = node else {
            return Ok(None);
        };
        let m = sys.remote(*fs)?;
        let cred = request::cred();
        request::note_owner(owner);
        let acc = oflag & f::O_ACCMODE;
        let writes = acc != f::O_RDONLY || oflag & f::O_TRUNC != 0;
        if writes && m.info().read_only {
            return Err(Errno::EROFS);
        }
        if !created {
            m.settle(m.attr_now(handle, &cred))?;
            let need = match acc {
                f::O_WRONLY => a::MODIFY,
                f::O_RDWR => a::READ | a::MODIFY,
                _ => a::READ,
            } | if oflag & f::O_TRUNC != 0 {
                a::MODIFY
            } else {
                0
            };
            let have = m.settle(m.access(handle, &cred))?;
            if attr.kind != crate::remote::RKind::Dir && have & need != need {
                return Err(Errno::EACCES);
            }
        }
        if node.is_dir() {
            return Ok(None);
        }
        if oflag & f::O_TRUNC != 0 && attr.size > 0 {
            m.settle(m.mutate(
                Mutation::Setattr {
                    h: handle.clone(),
                    set: SetAttr {
                        size: Some(0),
                        ..SetAttr::default()
                    },
                },
                &cred,
            ))?;
        }
        let token = if m.info().stateful_open {
            let share = Share {
                read: acc != f::O_WRONLY,
                write: acc != f::O_RDONLY,
                deny_read: oflag & (f::O_SHARE_NONE | f::O_SHARE_WRONLY) != 0,
                deny_write: oflag & (f::O_SHARE_NONE | f::O_SHARE_RDONLY) != 0,
            };
            match m.settle(m.mutate(
                Mutation::Open {
                    h: handle.clone(),
                    share,
                    owner: LockOwner(owner.0),
                },
                &cred,
            ))? {
                Answer::Token(t) => t,
                _ => OpenToken::default(),
            }
        } else {
            OpenToken::default()
        };
        Ok(Some(RemoteOpen {
            mount: m,
            h: handle.clone(),
            cred,
            token,
            locked: false,
            owner,
        }))
    }

    /// Before the last close of a remote open: its data written back
    /// (outer error: the request waits). The write-back's result; the
    /// close itself goes on either way.
    pub(crate) fn remote_close_prep(
        rest: &Rest<B::File>,
        cx: &Ctx<'_>,
        fd: i32,
    ) -> Result<Result<()>> {
        let Some(id) = rest
            .owners
            .get(&cx.owner)
            .and_then(|o| o.fds.get(&fd))
            .map(|e| e.open)
        else {
            return Ok(Ok(()));
        };
        let Some(o) = rest.opens.get(&id) else {
            return Ok(Ok(()));
        };
        let OpenKind::Remote(r) = &o.kind else {
            return Ok(Ok(()));
        };
        if o.refs != 1 {
            return Ok(Ok(()));
        }
        let key = format!("remote-close:{id}");
        if request::step::<bool>(&key).is_none() {
            if !r.mount.has_dirty(&r.h) {
                return Ok(Ok(()));
            }
            request::set_step(&key, true);
        }
        match r.sync() {
            Err(Errno::EAGAIN) => Err(Errno::EAGAIN),
            other => Ok(other),
        }
    }

    /// The open `o` is gone from `rest`: a file removed while open is
    /// removed when no open of it is left.
    pub(crate) fn remote_closed(rest: &Rest<B::File>, o: &Open<B::File>) {
        let OpenKind::Remote(r) = &o.kind else {
            return;
        };
        if rest.opens.values().any(|x| x.key == o.key) {
            return;
        }
        if let Some((dir, name)) = r.mount.take_hidden(&r.h) {
            r.mount.background(
                Mutation::Remove {
                    dir,
                    name,
                    h: Some(r.h.clone()),
                },
                &r.cred,
            );
        }
    }

    /// `RLSIFSLCK` of a remote object (or its whole mount): every local
    /// open of it that took locks unlocks all of its owner's on the
    /// server, without waiting. How many opens did.
    pub(crate) fn remote_release(
        rest: &mut Rest<B::File>,
        key: crate::lock::ObjKey,
        whole_fs: bool,
    ) -> usize {
        let mut n = 0;
        for o in rest.opens.values_mut() {
            let on = if whole_fs {
                o.key.fs == key.fs
            } else {
                o.key == key
            };
            if let (true, OpenKind::Remote(r)) = (on, &mut o.kind)
                && r.locked
            {
                r.mount.background(
                    Mutation::Unlock {
                        h: r.h.clone(),
                        owner: LockOwner(r.owner.0),
                        off: 0,
                        len: 0,
                    },
                    &r.cred,
                );
                r.locked = false;
                n += 1;
            }
        }
        n
    }

    /// `fstat` of a remote open.
    pub(crate) fn remote_fstat(cx: &Ctx<'_>, o: &Open<B::File>) -> Result<Option<Stat>> {
        let OpenKind::Remote(r) = &o.kind else {
            return Ok(None);
        };
        let m = &r.mount;
        let attr = m.settle(m.attr(&r.h, &r.cred))?;
        let node = Node::Remote {
            fs: o.key.fs,
            handle: r.h.clone(),
            attr,
        };
        let mut meta = crate::meta::fallback(&crate::remote::bstat_of(&attr), cx.env);
        meta.ccsid = m.data_ccsid();
        meta.codepage = ccsid::conv::codepage_of(meta.ccsid);
        meta.change = attr.change;
        Ok(Some(super::super::stat::stat_of(&node, &meta, cx.env)))
    }

    /// `fcntl` locks on a remote open: the server's.
    pub(crate) fn remote_lock(
        o: &mut Open<B::File>,
        cmd: i32,
        kind: RangeKind,
        start: u64,
        len: u64,
    ) -> Result<Option<RangeLock>> {
        let OpenKind::Remote(r) = &mut o.kind else {
            return Err(Errno::EINVAL);
        };
        let m = Arc::clone(&r.mount);
        if !m.info().locks {
            return Err(Errno::ENOLCK);
        }
        let owner = LockOwner(r.owner.0);
        let lk = |k: RangeKind| {
            if k == RangeKind::Read {
                LockKind::Read
            } else {
                LockKind::Write
            }
        };
        match cmd {
            f::F_GETLK | f::F_GETLK64 => {
                let ans = m.settle(m.mutate(
                    Mutation::Test {
                        h: r.h.clone(),
                        owner,
                        kind: lk(kind),
                        off: start,
                        len,
                    },
                    &r.cred,
                ))?;
                Ok(match ans {
                    Answer::Conflict(Some(c)) => Some(RangeLock::new(
                        Owner(c.holder),
                        if c.kind == LockKind::Read {
                            RangeKind::Read
                        } else {
                            RangeKind::Write
                        },
                        c.off,
                        c.len,
                    )),
                    _ => None,
                })
            }
            f::F_SETLK | f::F_SETLK64 | f::F_SETLKW | f::F_SETLKW64 => {
                let mu = if kind == RangeKind::Unlock {
                    Mutation::Unlock {
                        h: r.h.clone(),
                        owner,
                        off: start,
                        len,
                    }
                } else {
                    Mutation::Lock {
                        h: r.h.clone(),
                        owner,
                        kind: lk(kind),
                        off: start,
                        len,
                        token: r.token.clone(),
                    }
                };
                m.settle(m.mutate(mu, &r.cred))?;
                if kind != RangeKind::Unlock {
                    r.locked = true;
                }
                Ok(None)
            }
            _ => Err(Errno::EINVAL),
        }
    }
}
