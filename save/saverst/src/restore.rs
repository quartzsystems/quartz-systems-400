// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The restorer: a stream back into libraries under IBM's rules.
//!
//! An object that does not exist is created fresh: new ID, the saved
//! owner when that profile exists (else the default owner), the saved
//! public authority and authorization list, private authorities only
//! with `PVTAUT(*YES)` (otherwise `RSTAUT` brings them back). An object
//! that exists keeps its ID, owner and authorities and gets the saved
//! contents, if `OPTION` and `ALWOBJDIF` allow it; a physical file's
//! members follow `MBROPT`. Each top-level object (a file with its
//! members) is one transaction. A last pass patches references to
//! objects restored later in the stream and re-links journals and
//! logical files.

use std::collections::{BTreeMap, HashMap};
use std::io::Read;

use db::file::{self, DependentEntry, FileAttr, FileKind, members};
use savfmt::object::NO_OWNER;
use savfmt::{ObjectDescriptor, Part, Reader, RefEntry, SavedObject, Scan};
use store::obj::header::{Domain, text_of};
use store::obj::library::Library;
use store::obj::{Authority, JobHandle, LockState, ObjectId, ops, types};
use store::Store;
use store::txn::Txn;
use tracing::debug;

use crate::hooks::Hooks;
use crate::save::ObjResult;
use crate::select::{ObjSel, wanted};
use crate::{Error, Result, ids, image, msg, order};

/// `OPTION`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RstOption {
    /// `*ALL`.
    #[default]
    All,
    /// `*NEW`: only objects not on the system.
    New,
    /// `*OLD`: only objects on the system.
    Old,
    /// `*FREE`: only objects whose storage a save freed.
    Free,
}

/// `MBROPT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MbrOpt {
    /// `*MATCH`: the saved and existing member lists must match.
    #[default]
    Match,
    /// `*ALL`: every saved member.
    All,
    /// `*NEW`: only members not in the file.
    New,
    /// `*OLD`: only members already in the file.
    Old,
}

/// `ALWOBJDIF`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct AllowDiff {
    /// `*ALL`.
    pub all: bool,
    /// `*AUTL`.
    pub autl: bool,
    /// `*FILELVL`.
    pub filelvl: bool,
    /// `*OWNER`.
    pub owner: bool,
    /// `*PGP`.
    pub pgp: bool,
    /// `*COMPATIBLE`.
    pub compatible: bool,
}

impl AllowDiff {
    /// From the `ALWOBJDIF` values.
    #[must_use]
    pub fn from_values(values: &[String]) -> Self {
        let mut a = Self::default();
        for v in values {
            match v.to_ascii_uppercase().as_str() {
                "*ALL" => a.all = true,
                "*AUTL" => a.autl = true,
                "*FILELVL" => a.filelvl = true,
                "*OWNER" => a.owner = true,
                "*PGP" => a.pgp = true,
                "*COMPATIBLE" => {
                    a.compatible = true;
                    a.autl = true;
                    a.owner = true;
                    a.pgp = true;
                    a.filelvl = true;
                }
                _ => {}
            }
        }
        a
    }
}

/// `QALWOBJRST`: which security-sensitive objects may be restored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct AllowRestore {
    /// `*ALL`.
    pub all: bool,
    /// `*ALWSYSSTT`: system-state programs.
    pub system_state: bool,
    /// `*ALWPGMADP`: programs that adopt their owner's authority.
    pub adopting: bool,
    /// `*ALWVLDERR`: objects whose signature does not verify.
    pub validation_errors: bool,
}

impl Default for AllowRestore {
    fn default() -> Self {
        Self::from_value("*ALL")
    }
}

impl AllowRestore {
    /// From the system value's text (`*ALL`, `*NONE`, or a list).
    #[must_use]
    pub fn from_value(v: &str) -> Self {
        let has = |w: &str| v.split_whitespace().any(|x| x.eq_ignore_ascii_case(w));
        Self {
            all: has("*ALL"),
            system_state: has("*ALWSYSSTT"),
            adopting: has("*ALWPGMADP"),
            validation_errors: has("*ALWVLDERR"),
        }
    }
}

/// What a restore does, from its command's parameters.
#[derive(Debug, Clone)]
pub struct RestoreOptions {
    /// `OPTION`.
    pub option: RstOption,
    /// `MBROPT`.
    pub mbropt: MbrOpt,
    /// `ALWOBJDIF`.
    pub allow: AllowDiff,
    /// `OBJ`/`OBJTYPE`; empty for every object.
    pub select: Vec<ObjSel>,
    /// `OMITOBJ`.
    pub omit: Vec<ObjSel>,
    /// `SAVLIB`: the saved libraries to restore; empty for all.
    pub libraries: Vec<String>,
    /// `OMITLIB`.
    pub omit_libraries: Vec<String>,
    /// `RSTLIB`: restore into this library instead.
    pub target_library: Option<String>,
    /// `PVTAUT(*YES)`.
    pub private_auth: bool,
    /// Owner of an object whose saved owner does not exist.
    pub default_owner: String,
    /// The system's name, for libraries restore creates.
    pub system: String,
    /// Unix seconds of the restore.
    pub now: u64,
    /// `QVFYOBJRST`, 1 to 5.
    pub verify: u8,
    /// `QALWOBJRST`.
    pub allow_restore: AllowRestore,
}

impl Default for RestoreOptions {
    fn default() -> Self {
        Self {
            option: RstOption::All,
            mbropt: MbrOpt::Match,
            allow: AllowDiff::default(),
            select: Vec::new(),
            omit: Vec::new(),
            libraries: Vec::new(),
            omit_libraries: Vec::new(),
            target_library: None,
            private_auth: false,
            default_owner: "QDFTOWN".into(),
            system: String::new(),
            now: 0,
            verify: 3,
            allow_restore: AllowRestore::default(),
        }
    }
}

/// What a restore did.
#[derive(Debug, Clone, Default)]
pub struct RestoreReport {
    /// Every object considered, restored or not, in stream order.
    pub objects: Vec<ObjResult>,
    /// Diagnostics: (message, text), e.g. a default owner used.
    pub notes: Vec<(&'static str, String)>,
    /// Libraries the restore created.
    pub created_libraries: Vec<String>,
    /// Output queues restored with spooled files: library, queue, the
    /// spool part, for the integrator to bring the files back.
    pub spool: Vec<(String, String, Vec<u8>)>,
}

impl RestoreReport {
    /// Objects restored.
    #[must_use]
    pub fn restored(&self) -> usize {
        self.objects.iter().filter(|o| o.msgid.is_none()).count()
    }

    /// Objects not restored.
    #[must_use]
    pub fn not_restored(&self) -> usize {
        self.objects.len() - self.restored()
    }

    /// The completion message.
    #[must_use]
    pub fn completion(&self) -> &'static str {
        match (self.restored(), self.not_restored()) {
            (0, 0) => msg::NONE_FOR_LIB,
            (_, 0) => msg::RESTORED_LIB,
            _ => msg::RESTORED_LIB_PARTIAL,
        }
    }
}

/// A restored file the last pass looks at again.
struct FileDone {
    id: ObjectId,
    logical: bool,
    fresh: bool,
    members: Vec<ObjectId>,
}

#[derive(Default)]
struct State {
    map: HashMap<[u8; 16], ObjectId>,
    deferred: Vec<(ObjectId, RefEntry)>,
    files: Vec<FileDone>,
    journals: Vec<ObjectId>,
    libs: HashMap<String, Option<ObjectId>>,
}

/// One object's staged effects, kept only when its transaction commits.
#[derive(Default)]
struct Staged {
    map: Vec<([u8; 16], ObjectId)>,
    deferred: Vec<(ObjectId, RefEntry)>,
    file: Option<FileDone>,
    journal: Option<ObjectId>,
    notes: Vec<(&'static str, String)>,
    members: u32,
    signature: Option<store::obj::Signature>,
}

struct Ctx<'a> {
    job: JobHandle,
    hooks: &'a dyn Hooks,
    qsys: ObjectId,
    opts: &'a RestoreOptions,
}

/// Restore what `reader` holds.
pub fn restore<R: Read>(
    store: &mut Store,
    job: JobHandle,
    hooks: &dyn Hooks,
    qsys: ObjectId,
    reader: &mut Reader<R>,
    opts: &RestoreOptions,
) -> Result<RestoreReport> {
    let cx = Ctx {
        job,
        hooks,
        qsys,
        opts,
    };
    let mut st = State::default();
    let mut report = RestoreReport::default();
    while let Some(scan) = reader.next_object()? {
        let o = match scan {
            Scan::Object(o) => *o,
            Scan::Damaged(d) => {
                if let Some(d) = d.filter(|d| d.owner_index == NO_OWNER) {
                    report.objects.push(result_of(&d, Some(msg::NOT_RESTORED_DAMAGED)));
                }
                continue;
            }
        };
        let d = o.descriptor.clone();
        if d.owner_index != NO_OWNER {
            // Its owner was skipped.
            continue;
        }
        let mut members = Vec::new();
        let mut damaged = false;
        for _ in 0..d.owned_count {
            match reader.next_object()? {
                Some(Scan::Object(m)) if m.descriptor.owner_index == d.index => members.push(*m),
                _ => damaged = true,
            }
        }
        if d.type_code == types::LIB {
            library(store, &cx, &mut st, &mut report, &o)?;
            continue;
        }
        let saved_lib = d.library_name();
        if !lib_selected(opts, &saved_lib) {
            continue;
        }
        if !wanted(&opts.select, &opts.omit, &d.object_name(), d.type_code) {
            continue;
        }
        let mut result = result_of(&d, None);
        if damaged {
            result.msgid = Some(msg::NOT_RESTORED_DAMAGED);
            report.objects.push(result);
            continue;
        }
        let Some(lib) = target_lib(store, &cx, &mut st, &saved_lib)? else {
            result.msgid = Some(msg::NO_LIBRARY);
            report.objects.push(result);
            continue;
        };
        result.library = target_name(opts, &saved_lib);
        let mut txn = store.begin()?;
        let r = one(&mut txn, &cx, lib, &o, &members);
        match r {
            Ok(Some(staged)) => {
                txn.commit()?;
                result.members = staged.members;
                result.size = d.size_bytes;
                report.notes.extend(staged.notes.iter().cloned());
                st.map.extend(staged.map);
                st.deferred.extend(staged.deferred);
                st.files.extend(staged.file);
                st.journals.extend(staged.journal);
                if d.type_code == types::OUTQ && !o.part(Part::SPOOL).is_empty() {
                    report.spool.push((
                        result.library.clone(),
                        d.object_name(),
                        o.part(Part::SPOOL).to_vec(),
                    ));
                }
                report.objects.push(result);
            }
            Ok(None) => txn.abort(),
            Err(Error::Refused { msgid, text }) => {
                txn.abort();
                debug!("restore: {} not restored: {text}", d.object_name());
                result.msgid = Some(msgid);
                report.objects.push(result);
            }
            Err(e) => {
                txn.abort();
                debug!("restore: {} failed: {e}", d.object_name());
                result.msgid = Some(msg::NOT_RESTORED_DAMAGED);
                report.objects.push(result);
            }
        }
    }
    let mut txn = store.begin()?;
    finish(&mut txn, &cx, &st, &mut report)?;
    txn.commit()?;
    Ok(report)
}

fn result_of(d: &ObjectDescriptor, msgid: Option<&'static str>) -> ObjResult {
    ObjResult {
        library: d.library_name(),
        name: d.object_name(),
        type_code: d.type_code,
        attribute: text_of(&d.attribute),
        size: 0,
        msgid,
        members: 0,
    }
}

fn lib_selected(opts: &RestoreOptions, lib: &str) -> bool {
    (opts.libraries.is_empty() || opts.libraries.iter().any(|l| crate::select::generic(l, lib)))
        && !opts.omit_libraries.iter().any(|l| crate::select::generic(l, lib))
}

fn target_name(opts: &RestoreOptions, saved: &str) -> String {
    opts.target_library.clone().unwrap_or_else(|| saved.to_string())
}

fn target_lib(
    store: &mut Store,
    cx: &Ctx<'_>,
    st: &mut State,
    saved: &str,
) -> Result<Option<ObjectId>> {
    if let Some(l) = st.libs.get(saved) {
        return Ok(*l);
    }
    let name = target_name(cx.opts, saved);
    let mut txn = store.begin()?;
    let found = qsname::Name::parse(&name)
        .ok()
        .and_then(|n| ops::find_library(&mut txn, cx.qsys, n).ok())
        .map(|f| f.id);
    txn.abort();
    st.libs.insert(saved.to_string(), found);
    Ok(found)
}

/// A saved library object: create the target library when it is not
/// there.
fn library(
    store: &mut Store,
    cx: &Ctx<'_>,
    st: &mut State,
    report: &mut RestoreReport,
    o: &SavedObject,
) -> Result<()> {
    let saved = o.descriptor.object_name();
    if !lib_selected(cx.opts, &saved) {
        return Ok(());
    }
    if target_lib(store, cx, st, &saved)?.is_some() {
        return Ok(());
    }
    let name = target_name(cx.opts, &saved);
    let n = qsname::Name::parse(&name).map_err(|_| Error::refused(msg::NO_LIBRARY, name.clone()))?;
    let mut txn = store.begin()?;
    let params = ops::CreateParams {
        library: cx.qsys,
        name: n,
        type_code: types::LIB,
        attribute: String::new(),
        text: text_of(&o.descriptor.text),
        owner: owner_for(&mut txn, cx, &o.descriptor, &mut Vec::new()),
        system: cx.opts.system.clone(),
        now: cx.opts.now,
        by_system: true,
    };
    let id = ops::create(&mut txn, &params)?;
    image::import(&mut txn, id, &o.parts)?;
    set_header(&mut txn, cx, id, &o.descriptor, &o.authorities, true, &mut Vec::new())?;
    txn.commit()?;
    st.libs.insert(saved, Some(id));
    report.created_libraries.push(name);
    Ok(())
}

fn owner_for(
    txn: &mut Txn<'_>,
    cx: &Ctx<'_>,
    d: &ObjectDescriptor,
    notes: &mut Vec<(&'static str, String)>,
) -> String {
    let owner = text_of(&d.owner);
    if !owner.is_empty() && cx.hooks.profile_exists(txn, &owner) {
        return owner;
    }
    notes.push((
        msg::DEFAULT_OWNER,
        format!("{} {}: owner {owner} not found", d.library_name(), d.object_name()),
    ));
    cx.opts.default_owner.clone()
}

/// Restore one top-level object with its members, in `txn`. `None`:
/// skipped by `OPTION`.
fn one(
    txn: &mut Txn<'_>,
    cx: &Ctx<'_>,
    lib: ObjectId,
    o: &SavedObject,
    members: &[SavedObject],
) -> Result<Option<Staged>> {
    let d = &o.descriptor;
    let name = qsname::Name::from_bytes(d.name);
    let existing = Library::load(txn, lib)?.lookup(txn, d.type_code, name)?;
    match (cx.opts.option, existing.is_some()) {
        // QS/400 never frees an object's storage on save, so `*FREE`
        // finds nothing to restore.
        (RstOption::New, true) | (RstOption::Old, false) | (RstOption::Free, _) => {
            return Ok(None);
        }
        _ => {}
    }
    let existing_h = existing.map(|id| ops::read_header(txn, id)).transpose()?.map(|x| x.1);
    cx.hooks
        .may_restore(txn, d, lib, existing_h.as_ref())
        .map_err(|m| Error::refused(m, d.object_name()))?;
    if let Some(h) = &existing_h {
        differs(cx.opts.allow, d, h)?;
    }
    let signature = policy(cx, o)?;
    let mut staged = Staged {
        signature,
        ..Staged::default()
    };
    if d.type_code == types::FILE {
        restore_file(txn, cx, lib, o, members, existing, &mut staged)?;
    } else {
        restore_plain(txn, cx, lib, o, existing, &mut staged)?;
    }
    Ok(Some(staged))
}

/// Types whose signatures `QVFYOBJRST` checks.
fn signable(type_code: u16) -> bool {
    matches!(
        type_code,
        types::PGM | types::SRVPGM | types::MODULE | 0x001E
    )
}

/// `QALWOBJRST` and `QVFYOBJRST`: may the object come back, and with
/// which signature?
fn policy(cx: &Ctx<'_>, o: &SavedObject) -> Result<Option<store::obj::Signature>> {
    let d = &o.descriptor;
    let a = cx.opts.allow_restore;
    let refuse = |what: &str| Err(Error::refused(msg::NOT_RESTORED_SYSVAL, format!("{}: {what}", d.object_name())));
    if !a.all && signable(d.type_code) {
        if d.state == 0 && !a.system_state {
            return refuse("system state");
        }
        if d.has(savfmt::object::flags::ADOPTS) && !a.adopting {
            return refuse("adopts authority");
        }
    }
    let level = cx.opts.verify;
    let sig = o.signature.map(|s| {
        let good = cx.hooks.verify(s.key_id, &s.digest, &s.signature) == Some(true);
        (s, good)
    });
    if level >= 2 && signable(d.type_code) {
        let bad = |what: &str| {
            Err(Error::refused(
                msg::NOT_RESTORED_SIGNATURE,
                format!("{}: {what}", d.object_name()),
            ))
        };
        match sig {
            None if level >= 4 => return bad("not signed"),
            Some((_, false))
                if matches!(level, 3 | 5) && !(a.all || a.validation_errors) =>
            {
                return bad("signature does not verify");
            }
            _ => {}
        }
    }
    // Only a signature that verifies is kept.
    Ok(sig.filter(|(_, good)| *good).map(|(s, _)| store::obj::Signature {
        bytes: s.signature,
        key_id: s.key_id,
        system: cx.hooks.system_key(s.key_id),
    }))
}

/// `ALWOBJDIF`: may the saved object replace `h`?
fn differs(allow: AllowDiff, d: &ObjectDescriptor, h: &store::obj::ObjectHeader) -> Result<()> {
    if allow.all {
        return Ok(());
    }
    let refuse = |what: &str| Err(Error::refused(msg::NOT_RESTORED_DIFFERS, what.to_string()));
    if d.owner != h.owner && !allow.owner {
        return refuse("owner differs");
    }
    if d.primary_group != h.primary_group && !allow.pgp {
        return refuse("primary group differs");
    }
    if d.autl != h.autl && !allow.autl {
        return refuse("authorization list differs");
    }
    Ok(())
}

/// Resolve the references of an object about to be written: in-stream
/// ones through the map, the rest by name; what is not found yet is
/// zeroed and deferred to the last pass.
fn patch_refs(
    txn: &mut Txn<'_>,
    cx: &Ctx<'_>,
    map: &HashMap<[u8; 16], ObjectId>,
    target: ObjectId,
    parts: &mut BTreeMap<Part, Vec<u8>>,
    staged: &mut Staged,
) -> Result<Vec<RefEntry>> {
    let mut unresolved = Vec::new();
    for r in image::refs(parts)? {
        let mapped = map
            .get(&r.original_id)
            .copied()
            .or_else(|| staged.map.iter().find(|(o, _)| *o == r.original_id).map(|x| x.1));
        if let Some(id) = mapped {
            image::patch(parts, &r, id);
            continue;
        }
        let in_stream = r.flags & RefEntry::IN_STREAM != 0;
        if in_stream && r.flags & RefEntry::EARLIER == 0 {
            // Later in the stream: the last pass, or nothing.
            image::patch(parts, &r, ObjectId::NONE);
            staged.deferred.push((target, r.clone()));
            continue;
        }
        let first = if in_stream {
            cx.opts.target_library.as_deref()
        } else {
            None
        };
        if let Some(id) = ids::resolve(txn, cx.qsys, &r, first) {
            image::patch(parts, &r, id);
        } else {
            image::patch(parts, &r, ObjectId::NONE);
            unresolved.push(r);
        }
    }
    Ok(unresolved)
}

fn restore_plain(
    txn: &mut Txn<'_>,
    cx: &Ctx<'_>,
    lib: ObjectId,
    o: &SavedObject,
    existing: Option<ObjectId>,
    staged: &mut Staged,
) -> Result<()> {
    let d = &o.descriptor;
    let id = match existing {
        Some(id) => {
            lock(txn, cx, id, d)?;
            id
        }
        None => create(txn, cx, lib, d, None, &mut staged.notes)?,
    };
    staged.map.push((d.original_id, id));
    let mut parts = o.parts.clone();
    patch_refs(txn, cx, &HashMap::new(), id, &mut parts, staged)?;
    image::import(txn, id, &parts)?;
    set_header(txn, cx, id, d, &o.authorities, existing.is_none(), &mut staged.notes)?;
    set_signature(txn, id, staged.signature)?;
    if existing.is_some() {
        txn.store().locks_mut().unlock(cx.job, id, LockState::Exclusive);
    }
    if d.type_code == order::JRN {
        staged.journal = Some(id);
    }
    let (_, h) = ops::read_header(txn, id)?;
    cx.hooks.restored(txn, &h, existing.is_some());
    Ok(())
}

fn lock(txn: &mut Txn<'_>, cx: &Ctx<'_>, id: ObjectId, d: &ObjectDescriptor) -> Result<()> {
    txn.store()
        .locks_mut()
        .lock(cx.job, id, LockState::Exclusive)
        .map_err(|_| Error::refused(msg::NOT_RESTORED_IN_USE, d.object_name()))
}

/// A fresh object in `lib`, or owned by `owner`.
fn create(
    txn: &mut Txn<'_>,
    cx: &Ctx<'_>,
    lib: ObjectId,
    d: &ObjectDescriptor,
    owner: Option<ObjectId>,
    notes: &mut Vec<(&'static str, String)>,
) -> Result<ObjectId> {
    let params = ops::CreateParams {
        library: lib,
        name: qsname::Name::from_bytes(d.name),
        type_code: d.type_code,
        attribute: text_of(&d.attribute),
        text: text_of(&d.text),
        owner: owner_for(txn, cx, d, notes),
        system: text_of(&d.system),
        now: cx.opts.now,
        by_system: true,
    };
    Ok(match owner {
        Some(file) => ops::create_owned(txn, &params, file)?,
        None => ops::create(txn, &params)?,
    })
}

/// The header fields a restore sets. A fresh object takes the saved
/// ownership and authority; an existing one keeps its own.
fn set_header(
    txn: &mut Txn<'_>,
    cx: &Ctx<'_>,
    id: ObjectId,
    d: &ObjectDescriptor,
    auths: &[savfmt::AuthorityEntry],
    fresh: bool,
    notes: &mut Vec<(&'static str, String)>,
) -> Result<()> {
    let (page, mut h) = ops::read_header(txn, id)?;
    h.attribute = d.attribute;
    h.text = d.text;
    h.created = d.created;
    h.changed = d.changed;
    h.last_restored = cx.opts.now;
    h.domain = if d.domain == 0 {
        Domain::System
    } else {
        Domain::User
    };
    h.user_state = d.state != 0;
    if d.has(savfmt::object::flags::ADOPTS) {
        h.flags |= store::obj::ObjectHeader::ADOPTS;
    } else {
        h.flags &= !store::obj::ObjectHeader::ADOPTS;
    }
    if fresh {
        h.creator = d.creator;
        h.system = d.system;
        h.release = release::Release {
            v: d.release.0,
            r: d.release.1,
            m: d.release.2,
        };
        h.public_authority = Authority(d.public_authority);
        h.owner_authority = Authority(if d.auth_flags & 1 != 0 {
            d.owner_authority
        } else {
            Authority::ALL.0
        });
        h.group_authority = Authority(d.group_authority);
        h.audit = d.audit;
        let group = text_of(&d.primary_group);
        h.primary_group = if group.is_empty() || cx.hooks.profile_exists(txn, &group) {
            d.primary_group
        } else {
            [0x40; 10]
        };
        let autl = text_of(&d.autl);
        if autl.is_empty() || cx.hooks.autl_exists(txn, &autl) {
            h.autl = d.autl;
        } else {
            h.autl = [0x40; 10];
            notes.push((
                msg::NOT_RESTORED_DIFFERS,
                format!("{}: authorization list {autl} not found", d.object_name()),
            ));
        }
        h.auth_gen += 1;
    }
    ops::write_header(txn, page, &h)?;
    if fresh {
        let autl = text_of(&h.autl);
        if !autl.is_empty() {
            cx.hooks.secure(txn, &autl, id)?;
        }
        if cx.opts.private_auth {
            for a in auths {
                if cx.hooks.profile_exists(txn, &text_of(&a.profile)) {
                    cx.hooks.set_private(txn, id, a.profile, a.bits)?;
                }
            }
        }
    }
    Ok(())
}

fn set_signature(txn: &mut Txn<'_>, id: ObjectId, sig: Option<store::obj::Signature>) -> Result<()> {
    let (page, mut h) = ops::read_header(txn, id)?;
    if h.signature != sig {
        h.signature = sig;
        ops::write_header(txn, page, &h)?;
    }
    Ok(())
}

fn file_attr(o: &SavedObject) -> Result<FileAttr> {
    Ok(FileAttr::decode(o.part(Part::ATTR))?)
}

/// The formats region of a file's definition bytes.
fn formats<'a>(attr: &FileAttr, space: &'a [u8]) -> &'a [u8] {
    let at = usize::try_from(attr.formats_off).unwrap_or(usize::MAX);
    let len = usize::try_from(attr.formats_len).unwrap_or(0);
    space.get(at..at.saturating_add(len)).unwrap_or(&[])
}

#[allow(clippy::too_many_lines, clippy::many_single_char_names)]
fn restore_file(
    txn: &mut Txn<'_>,
    cx: &Ctx<'_>,
    lib: ObjectId,
    o: &SavedObject,
    saved_members: &[SavedObject],
    existing: Option<ObjectId>,
    staged: &mut Staged,
) -> Result<()> {
    let d = &o.descriptor;
    let attr = file_attr(o)?;
    let logical = attr.kind == FileKind::Logical;
    let Some(fid) = existing else {
        // Fresh: the file, then its members, then the contents, so the
        // member table and the members' file IDs patch at once.
        let fid = create(txn, cx, lib, d, None, &mut staged.notes)?;
        staged.map.push((d.original_id, fid));
        let mut mids = Vec::new();
        for m in saved_members {
            let mid = create(txn, cx, fid, &m.descriptor, Some(fid), &mut Vec::new())?;
            staged.map.push((m.descriptor.original_id, mid));
            mids.push(mid);
        }
        let mut parts = o.parts.clone();
        let unresolved = patch_refs(txn, cx, &HashMap::new(), fid, &mut parts, staged)?;
        if unresolved.iter().any(|r| r.type_code == order::JRN) {
            staged.notes.push((
                msg::JOURNAL_MISSING,
                format!("{}: its journal was not found", d.object_name()),
            ));
        }
        image::import(txn, fid, &parts)?;
        set_header(txn, cx, fid, d, &o.authorities, true, &mut staged.notes)?;
        for (m, mid) in saved_members.iter().zip(&mids) {
            let mut mp = m.parts.clone();
            let unresolved = patch_refs(txn, cx, &HashMap::new(), *mid, &mut mp, staged)?;
            if logical && !unresolved.is_empty() {
                return Err(Error::refused(msg::NOT_RESTORED_BASED_ON, d.object_name()));
            }
            image::import(txn, *mid, &mp)?;
            set_header(txn, cx, *mid, &m.descriptor, &[], true, &mut Vec::new())?;
        }
        staged.members = u32::try_from(mids.len()).expect("members");
        staged.file = Some(FileDone {
            id: fid,
            logical,
            fresh: true,
            members: mids,
        });
        let (_, h) = ops::read_header(txn, fid)?;
        cx.hooks.restored(txn, &h, false);
        return Ok(());
    };
    let desc = file::describe(txn, fid)?;
    if (desc.attr.kind == FileKind::Logical) != logical {
        return Err(Error::refused(msg::NOT_RESTORED_DIFFERS, d.object_name()));
    }
    lock(txn, cx, fid, d)?;
    staged.map.push((d.original_id, fid));
    if logical {
        // An existing logical file keeps its definition and paths.
        txn.store().locks_mut().unlock(cx.job, fid, LockState::Exclusive);
        return Ok(());
    }
    let (space, _) = store::obj::space::Space::load(txn, fid)?;
    let len = usize::try_from(space.bytes()).expect("space");
    let cur = space.read(txn, 0, len)?;
    let level_differs = formats(&desc.attr, &cur) != formats(&attr, o.part(Part::SPACE));
    if level_differs && !(cx.opts.allow.filelvl || cx.opts.allow.all) {
        return Err(Error::refused(msg::NOT_RESTORED_DIFFERS, d.object_name()));
    }
    if level_differs && desc.attr.record_len != attr.record_len {
        return Err(Error::refused(msg::NOT_RESTORED_DIFFERS, d.object_name()));
    }
    let saved_table = members::decode_table(
        o.part(Part::SPACE)
            .get(usize::try_from(attr.members_off).unwrap_or(usize::MAX)..)
            .unwrap_or(&[]),
        attr.member_count,
    );
    let have: Vec<String> = desc.members.iter().map(|m| m.name.to_string()).collect();
    let saved: Vec<String> = saved_members.iter().map(|m| m.descriptor.object_name()).collect();
    if cx.opts.mbropt == MbrOpt::Match {
        let (mut a, mut b) = (have.clone(), saved.clone());
        a.sort();
        b.sort();
        if a != b {
            return Err(Error::refused(msg::NOT_RESTORED_MEMBERS, d.object_name()));
        }
    }
    let mut done = Vec::new();
    for m in saved_members {
        let mname = m.descriptor.object_name();
        let current = desc.members.iter().find(|e| e.name.to_string() == mname);
        let mid = match (current, cx.opts.mbropt) {
            (Some(_), MbrOpt::New) | (None, MbrOpt::Old) => continue,
            (Some(e), _) => e.object,
            (None, _) => {
                let srctype = saved_table
                    .iter()
                    .find(|e| e.name.to_string() == mname)
                    .and_then(|e| e.srctype);
                file::add_member(
                    txn,
                    fid,
                    qsname::Name::from_bytes(m.descriptor.name),
                    &text_of(&m.descriptor.text),
                    srctype,
                    cx.opts.now,
                )?
            }
        };
        staged.map.push((m.descriptor.original_id, mid));
        let mut mp = m.parts.clone();
        let mut map = HashMap::new();
        map.insert(d.original_id, fid);
        patch_refs(txn, cx, &map, mid, &mut mp, staged)?;
        image::import(txn, mid, &mp)?;
        set_header(txn, cx, mid, &m.descriptor, &[], false, &mut Vec::new())?;
        // Logical members over it rebuild their paths before next use.
        for dep in desc.dependents.iter().filter(|x| x.base_member == mid) {
            if let Ok(mut lm) = db::member::Core::load(txn, dep.lf_member) {
                lm.invalidate(txn)?;
            }
        }
        done.push(mid);
    }
    let (page, mut h) = ops::read_header(txn, fid)?;
    h.last_restored = cx.opts.now;
    ops::write_header(txn, page, &h)?;
    txn.store().locks_mut().unlock(cx.job, fid, LockState::Exclusive);
    staged.members = u32::try_from(done.len()).expect("members");
    staged.file = Some(FileDone {
        id: fid,
        logical: false,
        fresh: false,
        members: done,
    });
    cx.hooks.restored(txn, &h, true);
    Ok(())
}

/// The last pass: references to objects restored later in the stream,
/// then the links between files, members and journals.
fn finish(txn: &mut Txn<'_>, cx: &Ctx<'_>, st: &State, report: &mut RestoreReport) -> Result<()> {
    for (obj, r) in &st.deferred {
        if let Some(new) = st.map.get(&r.original_id) {
            image::patch_stored(txn, *obj, r, *new)?;
        }
    }
    for f in &st.files {
        if f.logical && f.fresh {
            for mid in &f.members {
                let m = db::member::Core::load(txn, *mid)?;
                for base in m.base_members(txn)? {
                    if base.is_none() {
                        continue;
                    }
                    let (_, bh) = ops::read_header(txn, base)?;
                    file::add_dependent(
                        txn,
                        bh.library,
                        DependentEntry {
                            lf_file: f.id,
                            lf_member: *mid,
                            base_member: base,
                        },
                    )?;
                }
            }
        }
        if !f.logical && f.fresh {
            file::prune_dependents(txn, f.id)?;
        }
        if f.fresh {
            let desc = file::describe(txn, f.id)?;
            let j = desc.attr.journal;
            if !j.is_none() {
                let mut jr = jrn::Journal::load(txn, j)?;
                if jr.object(txn, f.id)?.is_none() {
                    jr.add_object(
                        txn,
                        jrn::JrnObjEntry {
                            object: f.id,
                            object_type: types::FILE,
                            both_images: desc.attr.jrn_both_images,
                            omit_open_close: desc.attr.jrn_omit_opnclo,
                            started: cx.opts.now,
                        },
                        &desc.name().to_string(),
                    )?;
                }
            }
        }
    }
    for j in &st.journals {
        let mut jr = jrn::Journal::load(txn, *j)?;
        for o in jr.objects(txn)? {
            if ops::read_header(txn, o.object).is_err() {
                jr.remove_object(txn, o.object, "")?;
                report.notes.push((
                    msg::JOURNAL_MISSING,
                    format!("{}: a journaled object is gone", jr.name()),
                ));
            }
        }
    }
    Ok(())
}
