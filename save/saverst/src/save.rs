// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The saver: a library's objects, in save order, under save locks,
//! into a stream; then the save history in each object's header.

use std::io::Write;

use savfmt::object::{NO_OWNER, flags};
use savfmt::{AuthorityEntry, ObjectDescriptor, Writer};
use store::obj::header::{Domain, ObjectHeader};
use store::obj::library::Library;
use store::obj::{JobHandle, LockState, ObjectId, SaveInfo, ops, types};
use store::Store;
use store::txn::Txn;
use tracing::debug;

use crate::hooks::Hooks;
use crate::image::{self, Exported};
use crate::select::{ObjSel, wanted};
use crate::{Result, msg, order};

/// What a save does, from its command's parameters.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct SaveOptions {
    /// `OBJ`/`OBJTYPE` (`SAVOBJ`); empty for every object.
    pub select: Vec<ObjSel>,
    /// `OMITOBJ`.
    pub omit: Vec<ObjSel>,
    /// Save the library object itself (`SAVLIB`; not `SAVOBJ`).
    pub library_object: bool,
    /// `UPDHST(*YES)`.
    pub update_history: bool,
    /// `SAVFDTA(*YES)`: a save file's data.
    pub save_file_data: bool,
    /// `PVTAUT(*YES)`.
    pub private_auth: bool,
    /// `SPLFDTA(*ALL)`: output queues carry their spooled files.
    pub spool: bool,
    /// The save file this save is written to: when it is in the saved
    /// library it is saved without its data (rule 21).
    pub target_savf: Option<ObjectId>,
    /// `SAVCHGOBJ`: only objects changed since (Unix seconds).
    pub changed_since: Option<u64>,
    /// `SAVCHGOBJ OBJJRN(*NO)`: journaled files are left out.
    pub skip_journaled: bool,
    /// Unix seconds of the save.
    pub now: u64,
    /// Where the save goes, for the history.
    pub target: SaveTarget,
}

impl Default for SaveOptions {
    fn default() -> Self {
        Self {
            select: Vec::new(),
            omit: Vec::new(),
            library_object: true,
            update_history: true,
            save_file_data: true,
            private_auth: false,
            spool: false,
            target_savf: None,
            changed_since: None,
            skip_journaled: false,
            now: 0,
            target: SaveTarget::default(),
        }
    }
}

/// Where a save went, as the object's history records it.
#[derive(Debug, Clone, Default)]
pub struct SaveTarget {
    /// The command.
    pub command: String,
    /// The device, or `*SAVF`.
    pub device: String,
    /// The save file and its library.
    pub file: Option<(String, String)>,
    /// The first volume.
    pub volume: String,
    /// The file label.
    pub label: String,
    /// The sequence number.
    pub seq: u32,
}

impl SaveTarget {
    fn info(&self, size: u64) -> SaveInfo {
        let (file, lib) = self.file.clone().unwrap_or_default();
        SaveInfo {
            size,
            command: savfmt::fixed(&self.command),
            device: savfmt::fixed(&self.device),
            file: savfmt::fixed(&file),
            file_lib: savfmt::fixed(&lib),
            volume: savfmt::fixed(&self.volume),
            label: savfmt::fixed(&self.label),
            seq: self.seq,
        }
    }
}

/// One object's outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjResult {
    /// Library.
    pub library: String,
    /// Name.
    pub name: String,
    /// Type code.
    pub type_code: u16,
    /// Attribute.
    pub attribute: String,
    /// Bytes saved or restored.
    pub size: u64,
    /// `None` when it was saved (restored); otherwise why not.
    pub msgid: Option<&'static str>,
    /// Members saved (restored) with a file.
    pub members: u32,
}

/// What a library's save did.
#[derive(Debug, Clone, Default)]
pub struct SaveReport {
    /// The library.
    pub library: String,
    /// Every object considered, saved or not.
    pub objects: Vec<ObjResult>,
    /// Informational messages, e.g. a save file saved without its data.
    pub notes: Vec<(&'static str, String)>,
}

impl SaveReport {
    /// Objects saved.
    #[must_use]
    pub fn saved(&self) -> usize {
        self.objects.iter().filter(|o| o.msgid.is_none()).count()
    }

    /// Objects not saved.
    #[must_use]
    pub fn not_saved(&self) -> usize {
        self.objects.len() - self.saved()
    }

    /// The completion message: all saved, some not, nothing.
    #[must_use]
    pub fn completion(&self) -> &'static str {
        match (self.saved(), self.not_saved()) {
            (0, _) => msg::NONE_FOR_LIB,
            (_, 0) => msg::SAVED_LIB,
            _ => msg::SAVED_LIB_PARTIAL,
        }
    }
}

/// The descriptor of a header.
pub fn descriptor(h: &ObjectHeader, library: &str, e: &Exported, now: u64) -> ObjectDescriptor {
    let mut flags = 0;
    if h.flags & ObjectHeader::ADOPTS != 0 {
        flags |= flags::ADOPTS;
    }
    if h.signature.is_some() {
        flags |= flags::SIGNED;
    }
    ObjectDescriptor {
        index: 0,
        owner_index: NO_OWNER,
        type_code: h.id.type_code,
        flags,
        domain: u8::from(h.domain == Domain::User),
        state: u8::from(h.user_state),
        original_id: h.id.to_bytes(),
        library_id: h.library.to_bytes(),
        library: savfmt::name10(library),
        name: *h.name.as_bytes(),
        attribute: h.attribute,
        text: h.text,
        owner: h.owner,
        primary_group: h.primary_group,
        creator: h.creator,
        system: h.system,
        release: (h.release.v, h.release.r, h.release.m),
        created: h.created,
        changed: h.changed,
        saved: now,
        restored: h.last_restored,
        last_used: h.last_used,
        days_used: h.days_used,
        public_authority: h.public_authority.0,
        autl: h.autl,
        audit: h.audit,
        owner_authority: h.owner_authority.0,
        group_authority: h.group_authority.0,
        auth_flags: u32::from(h.owner_authority != store::obj::Authority::ALL),
        size_bytes: e.size(),
        part_count: u16::try_from(e.parts.len()).expect("parts"),
        tail_len: 0,
        owned_count: 0,
    }
}

/// Write one exported object.
pub(crate) fn write_object<W: Write>(
    w: &mut Writer<W>,
    mut d: ObjectDescriptor,
    e: &Exported,
    auths: &[AuthorityEntry],
) -> Result<u32> {
    let sig = e.header.signature;
    if !auths.is_empty() {
        d.flags |= flags::PRIVATE_AUTH;
    }
    let index = w.begin_object(d, &[])?;
    if !auths.is_empty() {
        w.authorities(auths)?;
    }
    for (p, b) in &e.parts {
        w.part(*p, b)?;
    }
    w.end_object()?;
    if let Some(s) = sig {
        w.signature(s.key_id, s.bytes)?;
    }
    Ok(index)
}

/// Every private authority of `h`, the overflow included.
pub fn private_auths(txn: &mut Txn<'_>, h: &ObjectHeader) -> Result<Vec<AuthorityEntry>> {
    Ok(store::obj::auth::private_list(txn, h)?
        .into_iter()
        .map(|p| AuthorityEntry {
            profile: p.profile,
            bits: p.bits.0,
        })
        .collect())
}

struct Candidate {
    id: ObjectId,
    name: String,
    rank: (u8, u16),
    created: u64,
}

/// Save library `lib` into `w`. Objects in use, or that the hooks
/// refuse, are reported as not saved; the library is never left
/// locked.
pub fn save_library<W: Write>(
    store: &mut Store,
    job: JobHandle,
    hooks: &dyn Hooks,
    qsys: ObjectId,
    lib: &str,
    opts: &SaveOptions,
    w: &mut Writer<W>,
) -> Result<SaveReport> {
    let lib_name = qsname::Name::parse(lib)
        .map_err(|_| crate::Error::refused(msg::NONE_FOR_LIB, lib.to_string()))?;
    let mut report = SaveReport {
        library: lib.to_string(),
        objects: Vec::new(),
        notes: Vec::new(),
    };
    let mut locked = Vec::new();
    let mut saved = Vec::new();
    let result = {
        let mut txn = store.begin()?;
        let r = save_in(
            &mut txn,
            job,
            hooks,
            qsys,
            lib_name,
            opts,
            w,
            &mut report,
            &mut locked,
            &mut saved,
        );
        for id in &locked {
            txn.store().locks_mut().unlock(job, *id, LockState::SharedNoUpdate);
        }
        txn.abort();
        r
    };
    result?;
    if opts.update_history && !saved.is_empty() {
        let mut txn = store.begin()?;
        for (id, size) in &saved {
            let (page, mut h) = ops::read_header(&mut txn, *id)?;
            h.last_saved = opts.now;
            h.save = opts.target.info(*size);
            ops::write_header(&mut txn, page, &h)?;
        }
        txn.commit()?;
    }
    Ok(report)
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn save_in<W: Write>(
    txn: &mut Txn<'_>,
    job: JobHandle,
    hooks: &dyn Hooks,
    qsys: ObjectId,
    lib_name: qsname::Name,
    opts: &SaveOptions,
    w: &mut Writer<W>,
    report: &mut SaveReport,
    locked: &mut Vec<ObjectId>,
    saved: &mut Vec<(ObjectId, u64)>,
) -> Result<()> {
    let lib = ops::find_library(txn, qsys, lib_name)
        .map_err(|_| crate::Error::refused(msg::NO_LIBRARY, lib_name.to_string()))?
        .id;
    let lib_text = lib_name.to_string();
    if txn.store().locks_mut().lock(job, lib, LockState::SharedNoUpdate).is_err() {
        return Err(crate::Error::refused(msg::NOT_SAVED_IN_USE, lib_text));
    }
    locked.push(lib);
    let mut cands = Vec::new();
    for (key, id) in Library::load(txn, lib)?.list(txn, None)? {
        let name = key.name.to_string();
        if !wanted(&opts.select, &opts.omit, &name, id.type_code) {
            continue;
        }
        let (_, h) = ops::read_header(txn, id)?;
        if opts.changed_since.is_some_and(|t| h.changed < t) {
            continue;
        }
        let file = (id.type_code == types::FILE)
            .then(|| db::file::describe(txn, id).ok())
            .flatten();
        if opts.skip_journaled && file.as_ref().is_some_and(|d| !d.attr.journal.is_none()) {
            continue;
        }
        let logical = file.is_some_and(|d| d.attr.kind == db::FileKind::Logical);
        cands.push(Candidate {
            id,
            name,
            rank: order::rank(id.type_code, logical),
            created: h.created,
        });
    }
    cands.sort_by(|a, b| {
        let by_age = a.rank.0 == 40;
        a.rank.cmp(&b.rank).then(if by_age {
            a.created.cmp(&b.created)
        } else {
            a.name.cmp(&b.name)
        })
    });
    // Stream positions, members just after their file, for the
    // references' flags.
    let mut pos = std::collections::HashMap::new();
    let mut members_of = std::collections::HashMap::new();
    pos.insert(lib.to_bytes(), 0);
    for (i, c) in cands.iter().enumerate() {
        let p = 2 * u32::try_from(i + 1).expect("objects");
        pos.insert(c.id.to_bytes(), p);
        if c.id.type_code == types::FILE {
            let members = db::file::describe(txn, c.id)?.members;
            for m in &members {
                pos.insert(m.object.to_bytes(), p + 1);
            }
            members_of.insert(c.id, members);
        }
    }
    if opts.library_object {
        let e = image::export(txn, lib, true)?;
        let (_, h) = ops::read_header(txn, lib)?;
        let d = descriptor(&h, "QSYS", &e, opts.now);
        write_object(w, d, &e, &[])?;
        saved.push((lib, e.size()));
    }
    for c in &cands {
        let (_, h) = ops::read_header(txn, c.id)?;
        let mut result = ObjResult {
            library: lib_text.clone(),
            name: c.name.clone(),
            type_code: c.id.type_code,
            attribute: store::obj::header::text_of(&h.attribute),
            size: 0,
            msgid: None,
            members: 0,
        };
        if let Err(m) = hooks.may_save(txn, &h) {
            result.msgid = Some(m);
            report.objects.push(result);
            continue;
        }
        if txn.store().locks_mut().lock(job, c.id, LockState::SharedNoUpdate).is_err() {
            debug!("save: {} in use", c.name);
            result.msgid = Some(msg::NOT_SAVED_IN_USE);
            report.objects.push(result);
            continue;
        }
        locked.push(c.id);
        let target = opts.target_savf == Some(c.id);
        if target {
            report.notes.push((msg::SAVF_NO_DATA, c.name.clone()));
        }
        let data = c.id.type_code != order::SAVF || (opts.save_file_data && !target);
        let me = pos[&c.id.to_bytes()];
        let mut e = image::export(txn, c.id, data)?;
        image::mark_refs(&mut e, &pos, me);
        if opts.spool && c.id.type_code == types::OUTQ
            && let Some(b) = hooks.spool_part(&lib_text, &c.name)
            && !b.is_empty()
        {
            e.parts.insert(savfmt::Part::SPOOL, b);
        }
        let mut d = descriptor(&h, &lib_text, &e, opts.now);
        if !data {
            d.flags |= flags::NO_DATA;
        }
        let members = members_of.remove(&c.id).unwrap_or_default();
        d.owned_count = u32::try_from(members.len()).expect("members");
        let auths = if opts.private_auth {
            private_auths(txn, &h)?
        } else {
            Vec::new()
        };
        let index = write_object(w, d, &e, &auths)?;
        let mut size = e.size();
        for m in &members {
            let mut mx = image::export(txn, m.object, true)?;
            image::mark_refs(&mut mx, &pos, me + 1);
            let me = mx;
            let (_, mh) = ops::read_header(txn, m.object)?;
            let mut md = descriptor(&mh, &lib_text, &me, opts.now);
            md.owner_index = index;
            write_object(w, md, &me, &[])?;
            size += me.size();
            saved.push((m.object, me.size()));
        }
        hooks.saved(txn, &h);
        result.size = size;
        result.members = u32::try_from(members.len()).expect("members");
        saved.push((c.id, size));
        report.objects.push(result);
    }
    Ok(())
}
