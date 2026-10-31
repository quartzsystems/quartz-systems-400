// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The operations the APIs and commands need: [`Ifs`] over a backend,
//! called with a [`Ctx`] (the caller's owner id, principal, profile
//! environment, audit sink and job CCSID). Descriptors, directory
//! streams, the current directory and the umask are per [`Owner`];
//! [`Ifs::release_owner`] ends an owner's use when its job's connection
//! closes. One mutex holds the state: every call is atomic with respect
//! to the others.

pub mod audit;
pub mod flags;

mod attr;
mod authcmd;
mod copy;
mod dir;
mod io;
mod names;
mod open;
mod stat;
mod udfs;
mod walk;

pub use attr::{Attr, AttrId};
pub use authcmd::{AuthEntry, AuthList, ChgAut, Role, Who};
pub use copy::CopyOpts;
pub use dir::DirEnt;
pub use names::RenameMode;
pub use open::OpenArgs;
pub use stat::{Stat, StatVfs};
pub use udfs::UdfsInfo;
pub use walk::{Visit, WalkOpts};

use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, MutexGuard};

use sec::auth::principal::Principal;

use crate::auth::need::{ObjNeed, Op, rule_of};
use crate::auth::{self, AuthEnv};
use crate::backend::{Backend, Kind, Nanos};
use crate::errno::{Errno, Result};
pub use crate::lock::Owner;
use crate::lock::share::Access;
use crate::lock::{LockTable, ObjKey};
use crate::meta::{self, Meta, ObjAudit};
use crate::resolve::index::FoldIndex;
use crate::resolve::{Node, Res, Sys};
use crate::text::TextState;
use crate::vfs::dev::DevKind;
use crate::vfs::optical::{NoOptical, OpticalView};
use crate::vfs::qsys::{MemberData, QsysView};
use crate::vfs::{FsId, MountTable, QOPENSYS_BASE, ROOT_BASE, UDFS_BASE};
use audit::{Audit, AuditEvent, AuditKind, FileId};

/// The caller of one operation.
pub struct Ctx<'a> {
    /// Whose descriptors and locks.
    pub owner: Owner,
    /// Who is asking.
    pub principal: &'a Principal,
    /// Profiles and authorization lists.
    pub env: &'a dyn AuthEnv,
    /// Where audit events go.
    pub audit: &'a dyn Audit,
    /// The job's CCSID (what `O_CCSID` with 0 means).
    pub job_ccsid: u16,
}

impl Ctx<'_> {
    /// Whether the caller meets `need` on an object.
    #[must_use]
    pub fn meets(&self, m: &Meta, need: ObjNeed) -> bool {
        use sec::auth::required::Need;
        let p = self.principal;
        match need {
            ObjNeed::None => true,
            ObjNeed::Bits(b) => auth::check(p, m, self.env, Need::Bits(b)).allowed,
            ObjNeed::OwnerOrAllObj => auth::owner_or_allobj(p, m),
            ObjNeed::OwnerOr(b) => {
                auth::owner_or_allobj(p, m) || auth::check(p, m, self.env, Need::Bits(b)).allowed
            }
            ObjNeed::Special(s) => p.has(s),
            ObjNeed::BitsOrSpecial(b, s) => {
                p.has(s) || auth::check(p, m, self.env, Need::Bits(b)).allowed
            }
        }
    }

    fn event(
        &self,
        kind: AuditKind,
        m: &Meta,
        node: &Node,
        path: &str,
        oper: &'static str,
        detail: String,
    ) {
        self.audit.event(AuditEvent {
            kind,
            user: self.principal.user.clone(),
            path: path.to_string(),
            objtype: node.objtype(),
            file_id: FileId {
                fs: node.fs(),
                ino: node.ino(),
                generation: m.generation,
            },
            oper,
            violation: (kind == AuditKind::AuthFail).then_some('A'),
            detail,
        });
    }

    fn need(&self, m: &Meta, node: &Node, path: &str, need: ObjNeed, op: Op) -> Result<()> {
        if self.meets(m, need) {
            return Ok(());
        }
        self.event(
            AuditKind::AuthFail,
            m,
            node,
            path,
            crate::auth::need::name(op),
            String::new(),
        );
        Err(match need {
            ObjNeed::OwnerOrAllObj | ObjNeed::Special(_) => Errno::EPERM,
            _ => Errno::EACCES,
        })
    }

    /// Require what `op` needs on the object, auditing a refusal.
    pub fn require(&self, m: &Meta, node: &Node, path: &str, op: Op) -> Result<()> {
        self.need(m, node, path, rule_of(op).obj, op)
    }

    /// Require what `op` needs on the directory holding the object.
    pub fn require_dir(&self, m: &Meta, node: &Node, path: &str, op: Op) -> Result<()> {
        match rule_of(op).dir {
            Some(b) => self.need(m, node, path, ObjNeed::Bits(b), op),
            None => Ok(()),
        }
    }

    /// Report a read or change of an object whose auditing asks for it.
    pub fn audit_access(&self, m: &Meta, node: &Node, path: &str, op: Op) {
        let change = crate::auth::need::is_change(op);
        let value = match m.audit {
            ObjAudit::UsrPrf => ObjAudit::parse(&self.principal.objaud).unwrap_or(ObjAudit::None),
            v => v,
        };
        let wanted = match value {
            ObjAudit::All => true,
            ObjAudit::Change => change,
            _ => false,
        };
        if wanted {
            let kind = if change {
                AuditKind::Change
            } else {
                AuditKind::Read
            };
            self.event(
                kind,
                m,
                node,
                path,
                crate::auth::need::name(op),
                String::new(),
            );
        }
    }

    /// Report an event every time (create, delete, move, authority).
    pub fn audit_always(
        &self,
        kind: AuditKind,
        m: &Meta,
        node: &Node,
        path: &str,
        oper: &'static str,
        detail: String,
    ) {
        self.event(kind, m, node, path, oper, detail);
    }
}

/// Settings.
#[derive(Debug, Clone)]
pub struct Config {
    /// The CCSID of stream files created without `O_CCSID`/`O_CODEPAGE`.
    pub default_ccsid: u16,
    /// Most descriptors one owner may have.
    pub max_fds: usize,
    /// A new owner's umask.
    pub umask: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_ccsid: ccsid::conv::UTF8,
            max_fds: 65_535,
            umask: 0o022,
        }
    }
}

/// What an open file description reads and writes.
pub(crate) enum OpenKind<F> {
    /// A backend stream file.
    File(F),
    /// A character special file.
    Dev(DevKind),
    /// A member: its stream, written back on `fsync` and close.
    Member {
        lib: String,
        file: String,
        mbr: String,
        shape: MemberData,
        data: Vec<u8>,
        dirty: bool,
        /// Opened in text mode: lines are records.
        text: bool,
    },
    /// A `*USRSPC` or `*SAVF`.
    Stream {
        lib: String,
        name: String,
        objtype: String,
        data: Vec<u8>,
        dirty: bool,
    },
    /// An optical file; opened for writing, its bytes are held here and
    /// put back at close.
    Opt {
        vol: String,
        path: Vec<String>,
        size: u64,
        data: Option<Vec<u8>>,
        dirty: bool,
    },
    /// A directory opened for reading (for `openat`, `fchdir`, `fsync`).
    Dir,
}

/// An open file description.
pub(crate) struct Open<F> {
    /// Descriptors pointing at it.
    refs: u32,
    key: ObjKey,
    path: String,
    kind: OpenKind<F>,
    oflag: u32,
    access: Access,
    pos: u64,
    text: Option<TextState>,
}

/// A descriptor.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FdEntry {
    open: u64,
    cloexec: bool,
}

/// One owner's state.
pub(crate) struct OwnerState {
    cwd: Vec<String>,
    umask: u32,
    fds: BTreeMap<i32, FdEntry>,
    dirs: BTreeMap<u64, dir::DirStream>,
    next_dir: u64,
}

/// Everything but the mounts and the index.
pub(crate) struct Rest<F> {
    owners: HashMap<Owner, OwnerState>,
    opens: HashMap<u64, Open<F>>,
    next_open: u64,
    locks: LockTable,
    next_gen: u32,
}

pub(crate) struct State<F> {
    mounts: MountTable,
    index: FoldIndex,
    rest: Rest<F>,
}

/// The integrated file system over a backend.
pub struct Ifs<B: Backend> {
    b: B,
    qsys: Box<dyn QsysView>,
    opt: Box<dyn OpticalView>,
    cfg: Config,
    st: Mutex<State<B::File>>,
}

/// The directories every system has, with their `*PUBLIC` data
/// authority and whether they restrict rename and unlink.
pub const STANDARD_DIRS: &[(&str, &str, bool)] = &[
    ("/home", "*RX", false),
    ("/tmp", "*RWX", true),
    ("/QIBM", "*RX", false),
    ("/QIBM/ProdData", "*RX", false),
    ("/QIBM/UserData", "*RX", false),
    ("/etc", "*RX", false),
    ("/QOpenSys/usr", "*RX", false),
    ("/QOpenSys/etc", "*RX", false),
    ("/QOpenSys/tmp", "*RWX", true),
];

impl<B: Backend> Ifs<B> {
    /// The IFS over `b` with a `/QSYS.LIB` view and no optical volumes.
    pub fn new(b: B, qsys: Box<dyn QsysView>, cfg: Config) -> Self {
        Self::with_optical(b, qsys, Box::new(NoOptical), cfg)
    }

    /// The IFS with an optical view.
    pub fn with_optical(
        b: B,
        qsys: Box<dyn QsysView>,
        opt: Box<dyn OpticalView>,
        cfg: Config,
    ) -> Self {
        Self {
            b,
            qsys,
            opt,
            cfg,
            st: Mutex::new(State {
                mounts: MountTable::standard(),
                index: FoldIndex::default(),
                rest: Rest {
                    owners: HashMap::new(),
                    opens: HashMap::new(),
                    next_open: 1,
                    locks: LockTable::default(),
                    next_gen: 1,
                },
            }),
        }
    }

    /// The backend.
    pub fn backend(&self) -> &B {
        &self.b
    }

    /// The settings.
    pub fn config(&self) -> &Config {
        &self.cfg
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, State<B::File>> {
        self.st
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(crate) fn split<'a>(
        &'a self,
        st: &'a mut State<B::File>,
    ) -> (Sys<'a, B>, &'a mut Rest<B::File>) {
        let State {
            mounts,
            index,
            rest,
        } = st;
        (
            Sys {
                b: &self.b,
                qsys: &*self.qsys,
                opt: &*self.opt,
                mounts,
                index,
            },
            rest,
        )
    }

    /// Make the backend's top-level layout and the standard directories
    /// (owned by `QSYS`) where they are missing. Run at install and at
    /// every IPL.
    pub fn ensure_layout(&self, env: &dyn AuthEnv) -> Result<()> {
        let now = now();
        let mut top = Meta::new("QSYS", ccsid::conv::UTF8, now);
        top.public = crate::auth::data::RX;
        top.flags.set(meta::Flags::SYSDIR, true);
        for base in [ROOT_BASE, QOPENSYS_BASE, UDFS_BASE] {
            if self.b.stat(base).is_err() {
                self.b.mkdir(base, 0o755)?;
                meta::write(&self.b, base, Kind::Dir, &top, env)?;
            }
        }
        let asp = crate::backend::join(UDFS_BASE, crate::vfs::dev::SYSTEM_ASP);
        if self.b.stat(&asp).is_err() {
            self.b.mkdir(&asp, 0o755)?;
            meta::write(&self.b, &asp, Kind::Dir, &top, env)?;
        }
        for (path, public, restricted) in STANDARD_DIRS {
            let (base, rest) = match path.strip_prefix("/QOpenSys/") {
                Some(r) => (QOPENSYS_BASE, r),
                None => (ROOT_BASE, &path[1..]),
            };
            let rel = crate::backend::join(base, rest);
            if self.b.stat(&rel).is_ok() {
                continue;
            }
            self.b.mkdir(&rel, 0o755)?;
            let mut m = top.clone();
            m.public = crate::auth::data::DataAuth::parse(public)
                .expect("name")
                .auth();
            m.flags.set(meta::Flags::RSTDRNMUNL, *restricted);
            meta::write(&self.b, &rel, Kind::Dir, &m, env)?;
        }
        self.lock().index.clear();
        Ok(())
    }

    fn owner_state<'r>(&self, rest: &'r mut Rest<B::File>, o: Owner) -> &'r mut OwnerState {
        rest.owners.entry(o).or_insert_with(|| OwnerState {
            cwd: Vec::new(),
            umask: self.cfg.umask,
            fds: BTreeMap::new(),
            dirs: BTreeMap::new(),
            next_dir: 1,
        })
    }

    pub(crate) fn cwd_of(st: &mut State<B::File>, o: Owner) -> Vec<String> {
        st.rest
            .owners
            .get(&o)
            .map(|s| s.cwd.clone())
            .unwrap_or_default()
    }

    /// `getcwd()`.
    pub fn getcwd(&self, cx: &Ctx<'_>) -> String {
        let mut st = self.lock();
        crate::path::to_text(&Self::cwd_of(&mut st, cx.owner))
    }

    /// `chdir()`: needs `*X` on the directory.
    pub fn chdir(&self, cx: &Ctx<'_>, path: &str) -> Result<()> {
        let mut st = self.lock();
        let cwd = Self::cwd_of(&mut st, cx.owner);
        let (mut sys, rest) = self.split(&mut st);
        let r = sys.resolve(&cwd, path, true, Some(cx))?;
        if !r.node.is_dir() {
            return Err(Errno::ENOTDIR);
        }
        sys.check(&r.node, &r.path(), Some(cx), Op::Search)?;
        self.owner_state(rest, cx.owner).cwd = r.comps;
        Ok(())
    }

    /// `umask()`: set the mask, return the old one.
    pub fn umask(&self, cx: &Ctx<'_>, mask: u32) -> u32 {
        let mut st = self.lock();
        let s = self.owner_state(&mut st.rest, cx.owner);
        std::mem::replace(&mut s.umask, mask & 0o777)
    }

    /// End an owner: close its descriptors and directory streams
    /// (writing back members), drop its locks and its state.
    pub fn release_owner(&self, owner: Owner) {
        let mut st = self.lock();
        let Some(os) = st.rest.owners.remove(&owner) else {
            st.rest.locks.release_owner(owner);
            return;
        };
        for (_, fd) in os.fds {
            if let Some(o) = st.rest.opens.get_mut(&fd.open) {
                o.refs = o.refs.saturating_sub(1);
                if o.refs == 0
                    && let Some(o) = st.rest.opens.remove(&fd.open)
                    && let Err(e) = self.finish(o)
                {
                    tracing::warn!(error = %e, "IFS write-back failed releasing an owner");
                }
            }
        }
        st.rest.locks.release_owner(owner);
    }

    /// Owners with state (for displays and leak checks).
    pub fn owners(&self) -> Vec<Owner> {
        let mut v: Vec<Owner> = self.lock().rest.owners.keys().copied().collect();
        v.sort();
        v
    }

    /// The casefold index: directories cached and indexes built so far.
    pub fn index_stats(&self) -> (usize, u64) {
        let st = self.lock();
        (st.index.len(), st.index.builds)
    }

    /// Open file descriptions (for leak checks).
    pub fn open_count(&self) -> usize {
        self.lock().rest.opens.len()
    }

    /// The locks on the object at `path`, for `WRKOBJLCK`.
    pub fn object_locks(&self, cx: &Ctx<'_>, path: &str) -> Result<Vec<crate::lock::LockInfo>> {
        let mut st = self.lock();
        let cwd = Self::cwd_of(&mut st, cx.owner);
        let (mut sys, rest) = self.split(&mut st);
        let r = sys.resolve(&cwd, path, true, Some(cx))?;
        Ok(rest.locks.list(r.node.key()))
    }

    /// Everything an owner holds, for `WRKOBJLCK` by job.
    pub fn owner_locks(&self, owner: Owner) -> Vec<(ObjKey, crate::lock::LockInfo)> {
        self.lock().rest.locks.held_by(owner)
    }

    /// The mounts, for `DSPUDFS` and the displays.
    pub fn mounts(&self) -> Vec<crate::vfs::Mount> {
        self.lock().mounts.all().to_vec()
    }

    /// A node's metadata for the caller's resolution of `path`.
    pub(crate) fn resolve_meta(
        &self,
        cx: &Ctx<'_>,
        path: &str,
        follow: bool,
    ) -> Result<(Res, Meta)> {
        let mut st = self.lock();
        let cwd = Self::cwd_of(&mut st, cx.owner);
        let (mut sys, _) = self.split(&mut st);
        let r = sys.resolve(&cwd, path, follow, Some(cx))?;
        let (m, _) = sys.meta(&r.node, Some(cx))?;
        Ok((r, m))
    }

    /// Write a backend node's record (and its mirror).
    pub(crate) fn put_meta(&self, cx: &Ctx<'_>, node: &Node, m: &Meta) -> Result<()> {
        match node {
            Node::Back { rel, st, .. } => meta::write(&self.b, rel, st.kind, m, cx.env),
            Node::UdfsFile { asp, name, .. } => {
                let rel = crate::backend::join(&crate::backend::join(UDFS_BASE, asp), name);
                meta::write(&self.b, &rel, Kind::Dir, m, cx.env)
            }
            _ => Err(Errno::ENOTSUP),
        }
    }

    pub(crate) fn next_generation(rest: &mut Rest<B::File>) -> u32 {
        let g = rest.next_gen;
        rest.next_gen = rest.next_gen.wrapping_add(1).max(1);
        g
    }

    /// Whether the mount holding a node is read-only.
    pub(crate) fn read_only(sys: &Sys<'_, B>, fs: FsId) -> bool {
        sys.mounts.get(fs).is_some_and(|m| m.read_only)
    }

    /// Whether a node may not change: its mount is read-only, except
    /// in a writable optical volume.
    pub(crate) fn read_only_node(sys: &Sys<'_, B>, node: &Node) -> bool {
        match node {
            Node::Opt { vol, .. } => !sys.opt.writable(vol),
            _ => Self::read_only(sys, node.fs()),
        }
    }
}

/// The time now.
#[must_use]
pub fn now() -> Nanos {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_nanos()).unwrap_or(i64::MAX))
}

impl<B: Backend> Ifs<B> {
    /// `RCLLNK`: make every record whole (see `meta::reclaim`); needs
    /// `*ALLOBJ`. `check_only` reports without changing anything.
    pub fn rcllnk(&self, cx: &Ctx<'_>, check_only: bool) -> Result<meta::reclaim::Report> {
        if !cx.principal.has(sec::profile::spcaut::SpecialAuth::ALLOBJ) {
            return Err(Errno::EPERM);
        }
        let mut st = self.lock();
        let rep = meta::reclaim::run(&self.b, cx.env, check_only)?;
        st.index.clear();
        Ok(rep)
    }
}
