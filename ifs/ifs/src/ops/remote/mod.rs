// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The operations on remote mounts (`crate::remote`): `MOUNT
//! TYPE(*NFS)` through the registered factory, `UNMOUNT` by type and
//! directory, the mount table's view, the readiness of their waits,
//! and, in the submodules, every operation's remote half: names
//! ([`names`]), opens and data ([`io`]), attributes ([`attr`]).

mod attr;
mod io;
mod names;

use std::sync::Arc;
use std::time::{Duration, Instant};

use sec::profile::spcaut::SpecialAuth;

pub(crate) use io::RemoteOpen;

use super::{Ctx, Ifs};
use crate::backend::Backend;
use crate::errno::{Errno, Result};
use crate::lease::defer::Deferral;
use crate::lock::Owner;
use crate::path::{self, fold};
use crate::remote::mount::{Opened, Unclaimed};
use crate::remote::request::{self, Found};
use crate::remote::{
    CcsidSel, Events, HelperResult, HelperWork, MountCcsid, MountInfo, RemoteConfig, RemoteFactory,
    RemoteMount, RemoteRef, tickets,
};
use crate::resolve::Node;
use crate::vfs::FsKind;

/// Defer the request in progress on `ticket` of mount `fs`: `EAGAIN`.
fn wait_on(fs: u32, ticket: u64) -> Errno {
    if let Some(t) = request::current() {
        request::note_waiting();
        crate::lease::defer::defer_remote(Deferral::Remote {
            fs,
            ticket,
            token: t.id,
            until: Instant::now() + Duration::from_hours(24),
        });
    }
    Errno::EAGAIN
}

/// A CCSID `MOUNT` names, resolved against the job's.
fn resolve_ccsid(sel: CcsidSel, job: u16, path: bool) -> Result<u16> {
    let c = match sel {
        CcsidSel::Binary if path => ccsid::conv::UTF8,
        CcsidSel::Binary => ccsid::conv::BINARY,
        CcsidSel::Ascii => crate::remote::ascii_of(job).ok_or(Errno::ENOTSUP)?,
        CcsidSel::Job => job,
        CcsidSel::Ccsid(n) => n,
    };
    if c != ccsid::conv::BINARY {
        match ccsid::conv::Codec::of(c) {
            Ok(Some(codec)) if codec.is_mixed() => return Err(Errno::ENOTSUP),
            Ok(_) => {}
            Err(_) => return Err(Errno::EINVAL),
        }
    }
    Ok(c)
}

impl<B: Backend> Ifs<B> {
    /// Register what mounts remote file systems (the storage service's
    /// hook for the NFS client), with the settings of every mount;
    /// `None` takes it away (`MOUNT TYPE(*NFS)` is then `ENOTSUP`).
    pub fn set_remote_factory(&self, f: Option<Arc<dyn RemoteFactory>>, cfg: RemoteConfig) {
        self.remote.set_factory(f, cfg);
    }

    /// Where mounts report a server that stops and starts answering.
    pub fn set_remote_events(&self, e: Option<Events>) {
        self.remote.set_events(e);
    }

    /// Whether any remote file system is mounted (requests may then
    /// have to wait for one).
    pub fn has_remote(&self) -> bool {
        self.lock().mounts.has_remote()
    }

    /// The remote mount `fs`.
    pub fn remote_mount(&self, fs: u32) -> Option<Arc<RemoteMount>> {
        self.lock().mounts.remote(fs).cloned()
    }

    /// Whether a deferred request may run again: its leases broke, its
    /// remote work is done, or it was interrupted on an `intr` mount.
    pub fn deferral_ready(&self, d: &Deferral) -> bool {
        match *d {
            Deferral::Remote {
                fs, ticket, token, ..
            } => {
                !tickets::pending(ticket)
                    || (fs != super::syncer::LOCAL_SYNC
                        && request::interrupted_id(token)
                        && (fs == 0 || self.remote_mount(fs).is_none_or(|m| m.info().intr)))
            }
            _ => self.leases.ready(d),
        }
    }

    /// Block until `d` is ready (true) or its time is up (false): for
    /// callers that may block (tests, tools), never the service's loop.
    pub fn wait_deferral(&self, d: &Deferral) -> bool {
        match *d {
            Deferral::Remote { ticket, until, .. } => {
                tickets::wait(ticket, until, &|| self.deferral_ready(d))
            }
            _ => self.leases.wait(d),
        }
    }

    /// Interrupt `owner`'s waiting requests (its job is ending, or
    /// System Request 2): on an `intr` mount they answer `EINTR`.
    pub fn interrupt_owner(&self, owner: Owner) -> bool {
        request::interrupt_owner(owner)
    }

    /// `MOUNT TYPE(kind) MFS(mfs) MNTOVRDIR(over) OPTIONS(options)
    /// CCSID(...)`: `*IOSYSCFG`; `over` an existing directory of a
    /// backend file system that is not a mount point. The factory's
    /// mount runs on a thread of its own and the request waits.
    pub fn mount_remote(
        &self,
        cx: &Ctx<'_>,
        kind: &str,
        mfs: &str,
        over: &str,
        options: &str,
        ccsid: MountCcsid,
    ) -> Result<u32> {
        if !cx.principal.has(SpecialAuth::IOSYSCFG) {
            return Err(Errno::EPERM);
        }
        let factory = self
            .remote
            .factory()
            .filter(|f| f.supports(kind))
            .ok_or(Errno::ENOTSUP)?;
        let comps = self.mount_point(cx, over)?;
        let data = resolve_ccsid(ccsid.data, cx.job_ccsid, false)?;
        let pathc = resolve_ccsid(ccsid.path, cx.job_ccsid, true)?;
        let tok = request::current().ok_or(Errno::EAGAIN)?;
        let key = format!("mount:{kind}:{mfs}:{over}");
        match request::slot(tok, &key) {
            Found::InFlight(t) => Err(wait_on(0, t)),
            Found::None => {
                if !request::allowed() {
                    return Err(Errno::EAGAIN);
                }
                let ticket = tickets::issue();
                request::queued(tok, &key, ticket);
                let (kind_s, mfs_s, opts) =
                    (kind.to_string(), mfs.to_string(), options.to_string());
                let cred = crate::remote::cred_of(cx);
                let resolved = MountCcsid {
                    data: CcsidSel::Ccsid(data),
                    path: CcsidSel::Ccsid(pathc),
                };
                let key2 = key.clone();
                let spawned = std::thread::Builder::new()
                    .name("qs-remote-mount".into())
                    .spawn(move || {
                        let r = factory
                            .mount(&kind_s, &mfs_s, &opts, resolved, &cred)
                            .and_then(|m| {
                                let root = m.fs.root()?;
                                let pathconf = m.fs.pathconf(&cred, &root.0).unwrap_or_default();
                                Ok(Opened {
                                    mounted: m,
                                    root,
                                    pathconf,
                                })
                            });
                        // Dropped unclaimed, it ends the mount on the server.
                        request::post_any(tok.id, &key2, Box::new(Unclaimed(Some(r))));
                        tickets::finish(ticket);
                    });
                if spawned.is_err() {
                    tickets::finish(ticket);
                    return Err(Errno::ENOSYSRSC);
                }
                Err(wait_on(0, ticket))
            }
            Found::Done(_) => {
                let got = request::take_any(tok, &key).ok_or(Errno::EIO)?;
                let mut got = got.downcast::<Unclaimed>().map_err(|_| Errno::EIO)?;
                let opened = got.0.take().ok_or(Errno::EIO)?;
                let opened = opened.map_err(crate::remote::RemoteError::errno)?;
                // The directory may have changed while the mount was made:
                // then the mount ends again on the server.
                let comps2 = self.mount_point(cx, over);
                if comps2.as_ref().ok() != Some(&comps) {
                    drop(Unclaimed(Some(Ok(opened))));
                    return Err(comps2.err().unwrap_or(Errno::EBUSY));
                }
                let m = RemoteMount::start(
                    opened,
                    kind,
                    mfs,
                    data,
                    pathc,
                    self.remote.config(),
                    self.remote.events(),
                );
                let mut st = self.lock();
                let id = st.mounts.add_remote(comps, RemoteRef(Arc::clone(&m)));
                Ok(id)
            }
        }
    }

    /// The directory a mount goes over: an existing directory of a
    /// backend file system, not a mount point.
    fn mount_point(&self, cx: &Ctx<'_>, over: &str) -> Result<Vec<String>> {
        let mut st = self.lock();
        let cwd = Self::cwd_of(&mut st, cx.owner);
        let (mut sys, _) = self.split(&mut st);
        let r = sys.resolve(&cwd, over, true, Some(cx))?;
        if sys.mounts.all().iter().any(|mt| mt.at == r.comps) {
            return Err(Errno::EBUSY);
        }
        if matches!(r.node, Node::Remote { .. }) {
            // Nothing is mounted inside a remote mount.
            return Err(Errno::ENOTSUP);
        }
        if !matches!(r.node, Node::Back { .. }) || !r.node.is_dir() {
            return Err(Errno::ENOTDIR);
        }
        Ok(r.comps)
    }

    /// `UNMOUNT TYPE(kind) MNTOVRDIR(over)`: `kind` `*NFS`, `*UDFS` or
    /// `*ALL`; `over` a mount point or `*ALL`. A mount anything is open
    /// in, or a current directory, or mounted over, is `EBUSY`; a
    /// remote one writes what it holds first and tells its server. How
    /// many were unmounted.
    pub fn unmount_kind(&self, cx: &Ctx<'_>, kind: &str, over: &str) -> Result<u32> {
        if !cx.principal.has(SpecialAuth::IOSYSCFG) {
            return Err(Errno::EPERM);
        }
        let kind = kind.trim().to_ascii_uppercase();
        let all = over.trim().eq_ignore_ascii_case("*ALL");
        let target = path::components(&path::join_lexical("/", over));
        let picked: Vec<(u32, Vec<String>)> = {
            let st = self.lock();
            let mut v: Vec<(u32, Vec<String>)> = st
                .mounts
                .all()
                .iter()
                .filter(|m| match &m.kind {
                    FsKind::Remote(r) => kind == "*ALL" || r.0.kind.eq_ignore_ascii_case(&kind),
                    FsKind::Udfs(_) => kind == "*ALL" || kind == "*UDFS",
                    _ => false,
                })
                .filter(|m| {
                    all || (m.at.len() == target.len()
                        && m.at.iter().zip(&target).all(|(a, b)| fold::same(a, b)))
                })
                .map(|m| (m.id, m.at.clone()))
                .collect();
            // The deepest first: one mounted over another goes before it.
            v.sort_by_key(|(_, at)| std::cmp::Reverse(at.len()));
            v
        };
        let done_key = "unmount:count";
        let mut done: u32 = request::step(done_key).unwrap_or(0);
        if picked.is_empty() {
            return if done > 0 {
                Ok(done)
            } else {
                Err(Errno::EINVAL)
            };
        }
        let mut busy = false;
        for (id, at) in picked {
            {
                let st = self.lock();
                let in_use = st.rest.locks.opens_on(id) > 0
                    || st.rest.owners.values().any(|o| {
                        o.cwd.starts_with(&at) || o.dirs.values().any(|d| d.at().starts_with(&at))
                    })
                    || st.mounts.under(&at).len() > 1;
                if in_use {
                    busy = true;
                    continue;
                }
            }
            if let Some(m) = self.remote_mount(id) {
                let cred = crate::remote::cred_of(cx);
                m.settle(m.mutate(crate::remote::work::Mutation::FlushAll, &cred))?;
                // A server that refuses to hear of it does not keep the mount.
                if let Err(e) = m.settle(m.mutate(crate::remote::work::Mutation::Unmount, &cred)) {
                    if e == Errno::EAGAIN || e == Errno::EINTR {
                        return Err(e);
                    }
                    tracing::warn!(error = ?e, "remote unmount call failed");
                }
                m.shutdown();
            }
            let mut st = self.lock();
            if let Some(mt) = st.mounts.remove(id)
                && !mt.base.is_empty()
            {
                st.index.drop_tree(&mt.base);
            }
            done += 1;
            request::set_step(done_key, done);
        }
        if busy {
            return Err(Errno::EBUSY);
        }
        Ok(done)
    }

    /// Every mounted file system, for `DSPMFSINF`, SQL and Navigator.
    pub fn mount_infos(&self) -> Vec<MountInfo> {
        let st = self.lock();
        st.mounts
            .all()
            .iter()
            .map(|m| {
                let base = MountInfo {
                    id: m.id,
                    kind: m.name.clone(),
                    at: path::to_text(&m.at),
                    source: String::new(),
                    options: if m.read_only {
                        "ro".into()
                    } else {
                        "rw".into()
                    },
                    read_only: m.read_only,
                    case_sensitive: m.case_sensitive,
                    responding: None,
                    data_ccsid: ccsid::conv::UTF8,
                    path_ccsid: ccsid::conv::UTF8,
                };
                match &m.kind {
                    FsKind::Udfs(p) => MountInfo {
                        kind: "*UDFS".into(),
                        source: p.clone(),
                        ..base
                    },
                    FsKind::Remote(r) => {
                        let i = r.0.info();
                        MountInfo {
                            kind: r.0.kind.clone(),
                            source: i.source.clone(),
                            options: i.options.clone(),
                            responding: Some(r.0.responding()),
                            data_ccsid: r.0.data_ccsid(),
                            path_ccsid: r.0.path_ccsid(),
                            ..base
                        }
                    }
                    _ => base,
                }
            })
            .collect()
    }

    /// Lend read-ahead or write-behind work to a block I/O helper job
    /// (`QNFSBIOD`): the first any remote mount has, or none.
    pub fn remote_take(&self, cx: &Ctx<'_>) -> Result<Option<HelperWork>> {
        if !cx.principal.has(SpecialAuth::IOSYSCFG) {
            return Err(Errno::EPERM);
        }
        let mounts: Vec<Arc<RemoteMount>> = {
            let st = self.lock();
            st.mounts
                .all()
                .iter()
                .filter_map(|m| match &m.kind {
                    FsKind::Remote(r) => Some(Arc::clone(&r.0)),
                    _ => None,
                })
                .collect()
        };
        Ok(mounts.iter().find_map(|m| m.take_helper()))
    }

    /// A helper's answer to work [`Self::remote_take`] lent it.
    pub fn remote_post(&self, cx: &Ctx<'_>, fs: u32, id: u64, r: &HelperResult) -> Result<()> {
        if !cx.principal.has(SpecialAuth::IOSYSCFG) {
            return Err(Errno::EPERM);
        }
        self.remote_mount(fs)
            .ok_or(Errno::ENOENT)?
            .post_helper(id, r)
    }

    /// Whether `owner`'s descriptor `fd` is open on a remote file.
    pub fn fd_is_remote(&self, owner: Owner, fd: i32) -> bool {
        let st = self.lock();
        st.rest
            .owners
            .get(&owner)
            .and_then(|o| o.fds.get(&fd))
            .and_then(|e| st.rest.opens.get(&e.open))
            .is_some_and(|o| matches!(o.kind, super::OpenKind::Remote(_)))
    }

    /// Whether `p` is where a remote file system is mounted.
    pub(crate) fn remote_point(&self, p: &str) -> bool {
        let c = path::components(&path::join_lexical("/", p));
        self.lock().mounts.all().iter().any(|m| {
            matches!(m.kind, FsKind::Remote(_))
                && m.at.len() == c.len()
                && m.at.iter().zip(&c).all(|(a, b)| fold::same(a, b))
        })
    }
}
