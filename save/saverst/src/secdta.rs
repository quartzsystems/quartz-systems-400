// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Security data: `SAVSECDTA`, `RSTUSRPRF`, `RSTAUT`.
//!
//! As on IBM i, a profile's private authorities travel with the profile,
//! by object name, in its authority table. `RSTUSRPRF` restores the
//! profiles and authorization lists and keeps each profile's table
//! pending; `RSTAUT` applies the tables to the objects that exist by
//! then and keeps what it could not apply.

use std::collections::BTreeMap;
use std::io::{Read, Write};

use savfmt::object::flags;
use savfmt::{Part, Reader, RefEntry, Scan, Writer};
use store::Store;
use store::btree::BTree;
use store::obj::header::text_of;
use store::obj::library::Library;
use store::obj::ops::{self, CreateParams};
use store::obj::{JobHandle, ObjectId, types, usrspc};
use store::txn::Txn;

use crate::hooks::Hooks;
use crate::save::{ObjResult, SaveReport, descriptor, private_auths, write_object};
use crate::{Result, ids, image, msg};

/// One row of a profile's authority table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthEntry {
    /// The object's library (a member's file's), CP037.
    pub library: [u8; 10],
    /// Its name.
    pub name: [u8; 10],
    /// Member name, blank for an object.
    pub member: [u8; 10],
    /// Type code.
    pub type_code: u16,
    /// The authority bits.
    pub bits: u32,
}

/// `AuthTableEntry` size.
pub const ENTRY_LEN: usize = 36;

impl AuthEntry {
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.library);
        out.extend_from_slice(&self.name);
        out.extend_from_slice(&self.member);
        out.extend_from_slice(&self.type_code.to_be_bytes());
        out.extend_from_slice(&self.bits.to_be_bytes());
    }

    fn decode(b: &[u8]) -> Self {
        let f = |at: usize| -> [u8; 10] { b[at..at + 10].try_into().expect("10") };
        Self {
            library: f(0),
            name: f(10),
            member: f(20),
            type_code: u16::from_be_bytes([b[30], b[31]]),
            bits: u32::from_be_bytes(b[32..36].try_into().expect("4")),
        }
    }

    fn as_ref(&self) -> RefEntry {
        RefEntry {
            original_id: [0; 16],
            type_code: self.type_code,
            region: 0,
            library: self.library,
            name: self.name,
            member: self.member,
            offset: 0,
            flags: 0,
        }
    }
}

fn decode_table(b: &[u8]) -> Vec<AuthEntry> {
    b.as_chunks::<ENTRY_LEN>().0.iter().map(|c| AuthEntry::decode(c)).collect()
}

/// A profile's private authorities, by name.
fn table(txn: &mut Txn<'_>, profile: ObjectId) -> Result<Vec<AuthEntry>> {
    let (_, h) = ops::read_header(txn, profile)?;
    let t: BTree<ObjectId, u64> = BTree::open(h.tree_roots[1], profile);
    if t.is_empty() {
        return Ok(Vec::new());
    }
    let rows: Vec<(ObjectId, u64)> = t.entries(txn)?.collect::<store::Result<_>>()?;
    let mut out = Vec::new();
    for (obj, bits) in rows {
        if let Some(r) = ids::describe(txn, obj)? {
            out.push(AuthEntry {
                library: r.library,
                name: r.name,
                member: r.member,
                type_code: r.type_code,
                bits: u32::try_from(bits).unwrap_or(0),
            });
        }
    }
    Ok(out)
}

/// `SAVSECDTA`: every profile with its authority table, then every
/// authorization list with its entries.
pub fn save_security<W: Write>(
    store: &mut Store,
    hooks: &dyn Hooks,
    qsys: ObjectId,
    now: u64,
    w: &mut Writer<W>,
) -> Result<SaveReport> {
    let mut txn = store.begin()?;
    let mut report = SaveReport {
        library: "QSYS".into(),
        objects: Vec::new(),
    };
    let lib = Library::load(&mut txn, qsys)?;
    let mut ids_ = lib.list(&mut txn, Some(types::USRPRF))?;
    ids_.extend(lib.list(&mut txn, Some(types::AUTL))?);
    for (key, id) in ids_ {
        let (_, h) = ops::read_header(&mut txn, id)?;
        let mut result = ObjResult {
            library: "QSYS".into(),
            name: key.name.to_string(),
            type_code: id.type_code,
            attribute: String::new(),
            size: 0,
            msgid: None,
            members: 0,
        };
        if let Err(m) = hooks.may_save(&mut txn, &h) {
            result.msgid = Some(m);
            report.objects.push(result);
            continue;
        }
        let mut e = image::export(&mut txn, id, true)?;
        let auths = if id.type_code == types::USRPRF {
            let mut b = Vec::new();
            for row in table(&mut txn, id)? {
                row.encode(&mut b);
            }
            if !b.is_empty() {
                e.parts.insert(Part::typed(0), b);
            }
            Vec::new()
        } else {
            private_auths(&mut txn, &h)?
        };
        let d = descriptor(&h, "QSYS", &e, now);
        write_object(w, d, &e, &auths)?;
        result.size = e.size();
        report.objects.push(result);
    }
    txn.abort();
    Ok(report)
}

/// `RSTUSRPRF`'s choices.
#[derive(Debug, Clone, Default)]
pub struct ProfileOptions {
    /// `USRPRF`: names; empty for `*ALL`.
    pub users: Vec<String>,
    /// `USRPRF(*NEW)`: only profiles not on the system.
    pub new_only: bool,
    /// `OMITUSRPRF`.
    pub omit: Vec<String>,
    /// Unix seconds.
    pub now: u64,
}

/// What `RSTUSRPRF` did.
#[derive(Debug, Clone, Default)]
pub struct ProfileReport {
    /// Profiles and authorization lists considered.
    pub objects: Vec<ObjResult>,
}

impl ProfileReport {
    /// Profiles restored.
    #[must_use]
    pub fn profiles(&self) -> usize {
        self.count(types::USRPRF, true)
    }

    /// Profiles not restored.
    #[must_use]
    pub fn profiles_not(&self) -> usize {
        self.count(types::USRPRF, false)
    }

    /// Authorization lists restored.
    #[must_use]
    pub fn autls(&self) -> usize {
        self.count(types::AUTL, true)
    }

    fn count(&self, t: u16, ok: bool) -> usize {
        self.objects
            .iter()
            .filter(|o| o.type_code == t && o.msgid.is_none() == ok)
            .count()
    }
}

/// The pending authority tables: a user space in QSYS.
const PENDING: &str = "QSRSTAUT";

type Pending = BTreeMap<String, Vec<AuthEntry>>;

fn pending_id(txn: &mut Txn<'_>, qsys: ObjectId, create: bool) -> Result<Option<ObjectId>> {
    let name = qsname::Name::parse(PENDING).expect("name");
    if let Some(id) = Library::load(txn, qsys)?.lookup(txn, types::USRSPC, name)? {
        return Ok(Some(id));
    }
    if !create {
        return Ok(None);
    }
    let params = CreateParams {
        library: qsys,
        name,
        type_code: types::USRSPC,
        attribute: String::new(),
        text: "Authorities waiting for RSTAUT".into(),
        owner: "QSYS".into(),
        system: String::new(),
        now: 0,
        by_system: true,
    };
    Ok(Some(usrspc::create(txn, &params, 16, true, 0)?))
}

fn load_pending(txn: &mut Txn<'_>, qsys: ObjectId) -> Result<Pending> {
    let Some(id) = pending_id(txn, qsys, false)? else {
        return Ok(Pending::new());
    };
    let len = u64::from_be_bytes(usrspc::read(txn, id, 0, 8)?.try_into().expect("8"));
    let b = usrspc::read(txn, id, 8, usize::try_from(len).expect("fits"))?;
    let mut out = Pending::new();
    let mut at = 0;
    while at + 14 <= b.len() {
        let name = text_of(&b[at..at + 10]);
        let n = u32::from_be_bytes(b[at + 10..at + 14].try_into().expect("4")) as usize;
        at += 14;
        let end = (at + n * ENTRY_LEN).min(b.len());
        out.insert(name, decode_table(&b[at..end]));
        at = end;
    }
    Ok(out)
}

fn store_pending(txn: &mut Txn<'_>, qsys: ObjectId, p: &Pending) -> Result<()> {
    let id = pending_id(txn, qsys, true)?.expect("created");
    let mut b = Vec::new();
    for (name, rows) in p {
        b.extend_from_slice(&savfmt::name10(name));
        b.extend_from_slice(&u32::try_from(rows.len()).expect("rows").to_be_bytes());
        for r in rows {
            r.encode(&mut b);
        }
    }
    usrspc::write(txn, id, 0, &(b.len() as u64).to_be_bytes())?;
    usrspc::write(txn, id, 8, &b)?;
    Ok(())
}

fn wanted(opts: &ProfileOptions, name: &str) -> bool {
    (opts.users.is_empty() || opts.users.iter().any(|u| crate::select::generic(u, name)))
        && !opts.omit.iter().any(|u| crate::select::generic(u, name))
}

/// `RSTUSRPRF`: profiles and authorization lists from a `SAVSECDTA`
/// (or `SAVSYS`) stream; each profile's authority table is kept for
/// `RSTAUT`.
pub fn restore_profiles<R: Read>(
    store: &mut Store,
    job: JobHandle,
    hooks: &dyn Hooks,
    qsys: ObjectId,
    reader: &mut Reader<R>,
    opts: &ProfileOptions,
) -> Result<ProfileReport> {
    let mut report = ProfileReport::default();
    while let Some(scan) = reader.next_object()? {
        let Scan::Object(o) = scan else { continue };
        let d = &o.descriptor;
        if !matches!(d.type_code, types::USRPRF | types::AUTL) {
            continue;
        }
        let name = d.object_name();
        if d.type_code == types::USRPRF && !wanted(opts, &name) {
            continue;
        }
        let mut result = ObjResult {
            library: "QSYS".into(),
            name: name.clone(),
            type_code: d.type_code,
            attribute: String::new(),
            size: d.size_bytes,
            msgid: None,
            members: 0,
        };
        let mut txn = store.begin()?;
        let r = one_profile(&mut txn, job, hooks, qsys, &o, opts);
        match r {
            Ok(true) => {
                txn.commit()?;
                report.objects.push(result);
            }
            Ok(false) => txn.abort(),
            Err(e) => {
                txn.abort();
                result.msgid = Some(match e {
                    crate::Error::Refused { msgid, .. } => msgid,
                    _ => msg::NOT_RESTORED_DAMAGED,
                });
                report.objects.push(result);
            }
        }
    }
    Ok(report)
}

fn one_profile(
    txn: &mut Txn<'_>,
    job: JobHandle,
    hooks: &dyn Hooks,
    qsys: ObjectId,
    o: &savfmt::SavedObject,
    opts: &ProfileOptions,
) -> Result<bool> {
    let d = &o.descriptor;
    let name = qsname::Name::from_bytes(d.name);
    let existing = Library::load(txn, qsys)?.lookup(txn, d.type_code, name)?;
    if existing.is_some() && opts.new_only {
        return Ok(false);
    }
    let id = if let Some(id) = existing {
        if txn
            .store()
            .locks_mut()
            .lock(job, id, store::obj::LockState::Exclusive)
            .is_err()
        {
            return Err(crate::Error::refused(msg::NOT_RESTORED_IN_USE, d.object_name()));
        }
        id
    } else {
        ops::create(
            txn,
            &CreateParams {
                library: qsys,
                name,
                type_code: d.type_code,
                attribute: String::new(),
                text: text_of(&d.text),
                owner: text_of(&d.owner),
                system: text_of(&d.system),
                now: opts.now,
                by_system: true,
            },
        )?
    };
    let mut parts = o.parts.clone();
    let rows = parts.remove(&Part::typed(0)).unwrap_or_default();
    parts.remove(&Part::REFS);
    image::import(txn, id, &parts)?;
    let (page, mut h) = ops::read_header(txn, id)?;
    h.text = d.text;
    h.last_restored = opts.now;
    if existing.is_none() {
        h.public_authority = store::obj::Authority(d.public_authority);
    }
    ops::write_header(txn, page, &h)?;
    if existing.is_some() {
        txn.store()
            .locks_mut()
            .unlock(job, id, store::obj::LockState::Exclusive);
    }
    if d.type_code == types::USRPRF {
        let mut p = load_pending(txn, qsys)?;
        p.insert(d.object_name(), decode_table(&rows));
        store_pending(txn, qsys, &p)?;
    } else if d.has(flags::PRIVATE_AUTH) {
        for a in &o.authorities {
            if hooks.profile_exists(txn, &text_of(&a.profile)) {
                hooks.set_private(txn, id, a.profile, a.bits)?;
            }
        }
    }
    Ok(true)
}

/// What `RSTAUT` did for one profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthResult {
    /// The profile.
    pub profile: String,
    /// Authorities granted.
    pub restored: usize,
    /// Authorities whose object was not found; they stay pending.
    pub not_restored: usize,
}

/// `RSTAUT`: grant the pending authorities of `users` (empty: every
/// profile with a pending table).
pub fn rstaut(
    store: &mut Store,
    hooks: &dyn Hooks,
    qsys: ObjectId,
    users: &[String],
) -> Result<Vec<AuthResult>> {
    let mut txn = store.begin()?;
    let mut pending = load_pending(&mut txn, qsys)?;
    let mut out = Vec::new();
    let names: Vec<String> = pending
        .keys()
        .filter(|n| users.is_empty() || users.iter().any(|u| crate::select::generic(u, n)))
        .cloned()
        .collect();
    for profile in names {
        let rows = pending.remove(&profile).unwrap_or_default();
        let mut left = Vec::new();
        let mut restored = 0;
        if hooks.profile_exists(&mut txn, &profile) {
            for row in rows {
                match ids::resolve(&mut txn, qsys, &row.as_ref(), None) {
                    Some(obj) => {
                        hooks.set_private(&mut txn, obj, savfmt::name10(&profile), row.bits)?;
                        restored += 1;
                    }
                    None => left.push(row),
                }
            }
        } else {
            left = rows;
        }
        out.push(AuthResult {
            profile: profile.clone(),
            restored,
            not_restored: left.len(),
        });
        if !left.is_empty() {
            pending.insert(profile, left);
        }
    }
    store_pending(&mut txn, qsys, &pending)?;
    txn.commit()?;
    Ok(out)
}
