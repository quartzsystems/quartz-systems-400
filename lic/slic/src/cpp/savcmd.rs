// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Save and restore commands: `CRTSAVF`, `CHGSAVF`, `CLRSAVF`,
//! `SAVLIB`, `SAVOBJ`, `RSTLIB`, `RSTOBJ` and the rest. The engine is
//! `save/saverst`; this is parameters, the save file or the device
//! (`stgsvc::media::stream`: tape, optical, image catalogs), authority
//! and the messages.

use std::fmt::Display;
use std::fmt::Write as _;
use std::io::Cursor;

use cmd::Value;
use cmd::block::Parms;
use saverst::msg::{Data, Dest};
use saverst::{
    AllowDiff, AllowRestore, Hooks, MbrOpt, ObjSel, RestoreOptions, RestoreReport, RstOption,
    SaveOptions, SaveReport, SaveTarget, msg,
};
use savfmt::{Compression, MediaHeader, Reader, SaveKind, Writer};
use sec::profile::SpecialAuth;
use stg::proto::{CmdMsg, CmdReply};
use store::obj::header::ObjectHeader;
use store::obj::savf::{self, SavfAttr};
use store::obj::{JobHandle, ObjectId, ops, types};
use store::txn::Txn;

use super::direct::{ctx, lib_or};
use super::msgout::{self, Field};
use crate::ipl::system::{System, now_unix};
use crate::stgsvc::media::stream::{self, Media};
use crate::stgsvc::security::actor;
use optical::volume::layout::SaveTarget as OptTarget;

/// Run `name`, or `None` when it is not one of these.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    let r = match name {
        "CRTSAVF" => crtsavf(system, parms),
        "CHGSAVF" => chgsavf(system, parms),
        "CLRSAVF" => clrsavf(system, parms),
        "SAVLIB" | "SAVOBJ" | "SAVCHGOBJ" | "SAVCFG" => save(system, name, parms),
        "RSTLIB" | "RSTOBJ" | "RSTCFG" => rst(system, name, parms),
        "SAVSECDTA" => savsecdta(system, parms),
        "RSTUSRPRF" => rstusrprf(system, parms),
        "RSTAUT" => rstaut(system, parms),
        "SAV" => sav(system, parms),
        "RST" => rst_ifs(system, parms),
        "DSPSAVF" => dspsavf(system, parms),
        "SAVSAVFDTA" => savsavfdta(system, parms),
        _ => return None,
    };
    Some(r.unwrap_or_else(Fail::reply))
}

/// Save file not empty and `CLEAR(*NONE)`: the inquiry `CPA4067` takes
/// its default reply C, then `CPF4110` and the escape `CPF3794`.
const SAVF_NOT_EMPTY: &str = "CPF3794";
/// The diagnostic before it.
const SAVF_HOLDS_DATA: &str = "CPF4110";
/// The inquiry it answers.
const SAVF_INQUIRY: &str = "CPA4067";
/// `CLEAR(*AFTER)` is not for save files.
const CLEAR_AFTER: &str = "CPD3754";
/// The save file is empty (a restore).
const SAVF_EMPTY: &str = "CPF3707";
/// The save needs more records than the save file allows: `CPF3808`
/// then the escape `CPF3815`.
const SAVF_FULL: &str = "CPF3815";
/// The diagnostic before it.
const SAVF_INCOMPLETE: &str = "CPF3808";
/// No such device.
const NO_DEVICE: &str = "CPF9814";
/// Save active and other functions this release does not have
/// (QS/400's own: `CPF3700` is unused in IBM's QCPFMSG).
const NOT_AVAILABLE: &str = "CPF3700";
/// The save cannot be read: a later release, or not a save.
const BAD_SAVE: &str = "CPF3743";
/// A target release this release cannot write.
const BAD_TGTRLS: &str = "CPD3765";
/// Save file not found.
const NO_SAVF: &str = "CPF9812";
/// Not authorized to an object (the save file).
const NOT_AUTHORIZED: &str = "CPF2189";
/// A special authority the command needs.
const SPECIAL_AUTHORITY: &str = "CPF222E";
/// The save or restore ended: a cause underneath it.
const ENDED: &str = "CPF3794";
/// A library not found.
const NO_LIB: &str = "CPF2110";

/// A message for the reply: `kind` is the job log's type.
fn message(id: &str, kind: u8, data: Vec<u8>) -> CmdMsg {
    CmdMsg {
        msgid: id.into(),
        kind,
        data,
    }
}

/// The job log's types.
const DIAG: u8 = 2;
const INFO: u8 = 1;
const COMP: u8 = 4;
const ESC: u8 = 7;

/// A command that failed: its escape as the catalogue declares it,
/// diagnostics before it, and the cause underneath (`CPF9897`).
#[derive(Debug)]
pub(super) struct Fail {
    id: String,
    data: Vec<u8>,
    detail: String,
    diags: Vec<CmdMsg>,
}

impl Fail {
    /// Escape `id` with `fields`.
    pub(super) fn new(id: &str, fields: &[Field<'_>]) -> Self {
        Self::raw(id, msgout::data(fields))
    }

    /// Escape `id` with data laid out already.
    fn raw(id: &str, data: Vec<u8>) -> Self {
        Self {
            id: id.into(),
            data,
            detail: String::new(),
            diags: Vec::new(),
        }
    }

    /// The save or restore ended for `why` (the store, the stream).
    pub(super) fn ended(why: impl Display) -> Self {
        Self::new(ENDED, &[Field::Char("", 7)]).because(why)
    }

    /// With the cause underneath.
    pub(super) fn because(mut self, why: impl Display) -> Self {
        self.detail = why.to_string();
        self
    }

    /// With a diagnostic before the escape.
    fn after(mut self, m: CmdMsg) -> Self {
        self.diags.push(m);
        self
    }

    /// The reply.
    pub(super) fn reply(self) -> CmdReply {
        let mut r = msgout::escape(&self.id, &[], &self.detail);
        r.messages.extend(self.diags);
        if let Some(e) = r.escape.as_mut() {
            e.data = self.data;
        }
        r
    }

    /// The old text form, for callers that pass errors as text.
    pub(super) fn text(&self) -> String {
        format!("{} {}", self.id, self.detail)
            .trim_end()
            .to_string()
    }
}

/// Text from code that still reports `CPFnnnn detail` (the media
/// layer): its identifier, the rest as the cause.
impl From<String> for Fail {
    fn from(e: String) -> Self {
        match super::leading_msgid(e.trim()) {
            Some(id) => {
                let id = id.to_string();
                let rest = e.trim()[id.len()..].trim().to_string();
                Self::raw(&id, Vec::new()).because(rest)
            }
            None => Self::ended(e),
        }
    }
}

/// The special authority `a` (`*SAVSYS`, `*SECADM`), or `CPF222E`.
pub(super) fn need(a: SpecialAuth, name: &str, what: &str) -> Result<(), Fail> {
    crate::stgsvc::security::require(a, what)
        .map_err(|e| Fail::new(SPECIAL_AUTHORITY, &[Field::Char(name, 10)]).because(e))
}

/// Run `f` on ASP 1 in a transaction with the job's names.
fn in_store<T>(
    system: &mut System,
    f: impl FnOnce(&mut Txn<'_>, ObjectId, &ops::NameContext) -> Result<T, Fail>,
) -> Result<T, Fail> {
    let (store, qsys) = storage(system)?;
    let mut txn = store.begin().map_err(Fail::ended)?;
    let names = crate::svc::ile::name_context(&mut txn, qsys);
    match f(&mut txn, qsys, &names) {
        Ok(v) => {
            txn.commit().map_err(Fail::ended)?;
            Ok(v)
        }
        Err(e) => {
            txn.abort();
            Err(e)
        }
    }
}

/// The store and `QSYS`, or the escape when the store is not open.
fn storage(system: &mut System) -> Result<(&mut store::Store, ObjectId), Fail> {
    let s = system
        .storage
        .as_mut()
        .ok_or_else(|| Fail::ended("the object store is not open"))?;
    let qsys = s
        .qsys
        .ok_or_else(|| Fail::ended("the object store is not initialized"))?;
    Ok((&mut s.store, qsys))
}

/// A library by name, `*CURLIB` through the names.
fn library(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    names: &ops::NameContext,
    lib: &str,
) -> Result<ObjectId, Fail> {
    match lib {
        "*CURLIB" | "" => names
            .curlib
            .ok_or_else(|| Fail::new(NO_LIB, &[Field::Char("*CURLIB", 10)])),
        l => wm::obj::library(txn, qsys, l)
            .map_err(|e| Fail::new(NO_LIB, &[Field::Char(l, 10)]).because(e)),
    }
}

/// A device that is not one: `CPF9814`.
fn no_device(dev: &[String]) -> Fail {
    Fail::new(
        NO_DEVICE,
        &[Field::Char(dev.first().map_or("", String::as_str), 10)],
    )
}

fn job(system: &System) -> JobHandle {
    JobHandle(
        system
            .jobs
            .get(system.current_job)
            .map_or(u64::from(system.current_job), |j| j.handle),
    )
}

fn user(system: &System) -> String {
    actor()
        .map(|p| p.user.clone())
        .or_else(|| system.svc_user.clone())
        .unwrap_or_else(|| "QSYS".into())
}

/// The actor may save or restore anything (`*SAVSYS`, `*ALLOBJ`).
fn savsys() -> bool {
    actor().is_none_or(|p| p.has(SpecialAuth::SAVSYS) || p.has(SpecialAuth::ALLOBJ))
}

/// The security layer's answers for the engine.
pub(super) struct SvcHooks {
    qsys: ObjectId,
    /// Spool parts gathered before a save, by `lib/queue`.
    spool: std::collections::HashMap<String, Vec<u8>>,
}

impl SvcHooks {
    pub(super) fn new(qsys: ObjectId) -> Self {
        Self {
            qsys,
            spool: std::collections::HashMap::new(),
        }
    }
}

impl Hooks for SvcHooks {
    fn convert(&self, txn: &mut Txn<'_>, id: ObjectId, all: bool) -> Result<u32, String> {
        ile::store::convert(txn, id, all).map_err(|e| e.to_string())
    }

    fn may_save(&self, txn: &mut Txn<'_>, h: &ObjectHeader) -> Result<(), &'static str> {
        if savsys() || sec::guard::allowed(txn, self.qsys, "obj.save", h.id) {
            Ok(())
        } else {
            Err(msg::NOT_SAVED_AUTH)
        }
    }

    fn may_restore(
        &self,
        txn: &mut Txn<'_>,
        _d: &savfmt::ObjectDescriptor,
        lib: ObjectId,
        existing: Option<&ObjectHeader>,
    ) -> Result<(), &'static str> {
        if savsys() {
            return Ok(());
        }
        let ok = match existing {
            Some(h) => sec::guard::allowed(txn, self.qsys, "obj.restore", h.id),
            None => sec::guard::allowed(txn, self.qsys, "lib.create-in", lib),
        };
        if ok {
            Ok(())
        } else {
            Err(msg::NOT_RESTORED_AUTH)
        }
    }

    fn set_private(
        &self,
        txn: &mut Txn<'_>,
        id: ObjectId,
        profile: [u8; 10],
        bits: u32,
    ) -> store::Result<()> {
        let name = store::obj::header::text_of(&profile);
        sec::auth::grant::set_private(
            txn,
            self.qsys,
            id,
            &name,
            Some(sec::auth::bits::Auth::of(store::obj::Authority(bits))),
        )
        .map_err(|e| store::StoreError::BadValue {
            what: e.to_string(),
        })
    }

    fn secure(&self, txn: &mut Txn<'_>, autl: &str, id: ObjectId) -> store::Result<()> {
        // A list that is gone leaves the object unsecured.
        let _ = sec::auth::autl::secure(txn, self.qsys, autl, id);
        Ok(())
    }

    fn verify(&self, key_id: [u8; 8], digest: &[u8; 32], sig: &[u8; 64]) -> Option<bool> {
        trusted_keys()
            .iter()
            .find(|k| saverst::sign::key_id(k) == key_id)
            .map(|k| saverst::sign::verify(k, digest, sig))
    }

    fn system_key(&self, key_id: [u8; 8]) -> bool {
        trusted_keys()
            .iter()
            .any(|k| saverst::sign::key_id(k) == key_id)
    }

    fn spool_part(&self, lib: &str, name: &str) -> Option<Vec<u8>> {
        self.spool.get(&format!("{lib}/{name}")).cloned()
    }

    fn profile_exists(&self, txn: &mut Txn<'_>, name: &str) -> bool {
        sec::profile::objects::exists(txn, self.qsys, name)
    }

    fn autl_exists(&self, txn: &mut Txn<'_>, name: &str) -> bool {
        sec::auth::autl::find(txn, self.qsys, name).is_ok()
    }
}

/// Where the image keeps the public keys of the signers the system
/// trusts for its own objects: 32 raw bytes each, back to back.
const TRUSTED_KEYS: &str = "/qs400/keys/system.pub";

/// The trusted signers' public keys.
pub(crate) fn trusted_keys() -> Vec<[u8; 32]> {
    std::fs::read(TRUSTED_KEYS)
        .map(|b| b.as_chunks::<32>().0.to_vec())
        .unwrap_or_default()
}

/// The save file a `FILE`/`SAVF` parameter names.
fn find_savf(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    names: &ops::NameContext,
    (name, lib): &(String, String),
) -> Result<ObjectId, Fail> {
    let l = lib_or(lib);
    wm::obj::find(txn, qsys, names, &l, name, types::SAVF)
        .map_err(|_| Fail::new(NO_SAVF, &[Field::Char(name, 10), Field::Char(&l, 10)]))
}

/// `CPF2189` for save file `file` in `lib`.
fn savf_not_authorized(file: &str, lib: &str) -> Fail {
    Fail::new(
        NOT_AUTHORIZED,
        &[
            Field::Char(file, 10),
            Field::Char(lib, 10),
            Field::Char("FILE", 7),
        ],
    )
}

fn crtsavf(system: &mut System, parms: &Parms) -> Result<CmdReply, Fail> {
    let (name, lib) = parms.qual("FILE");
    let text = parms.text("TEXT");
    let attr = SavfAttr {
        max_records: u32::try_from(parms.int("MAXRCDS").unwrap_or(0)).unwrap_or(0),
        wait_file: i32::try_from(parms.int("WAITFILE").unwrap_or(0)).unwrap_or(0),
        share: parms.text("SHARE") == "*YES",
    };
    let c = ctx(system);
    let owner = user(system);
    let lib = if lib.is_empty() {
        "*CURLIB".to_string()
    } else {
        lib
    };
    in_store(system, |txn, qsys, names| {
        let l = library(txn, qsys, names, &lib)?;
        let n = qsname::Name::parse(&name)
            .map_err(|e| Fail::new("CPD0074", &[Field::Char("FILE", 10)]).because(e))?;
        savf::create(
            txn,
            &ops::CreateParams {
                library: l,
                name: n,
                type_code: types::SAVF,
                attribute: String::new(),
                text,
                owner,
                system: c.system.clone(),
                now: c.now,
                by_system: false,
            },
            attr,
        )
        .map_err(|e| {
            Fail::new(
                e.msgid().unwrap_or("CPF7302"),
                &[Field::Char(&name, 10), Field::Char(&lib, 10)],
            )
            .because(e)
        })?;
        Ok(())
    })?;
    Ok(CmdReply::default())
}

/// What `CHGSAVF` changes: -1, -2 and `*SAME` keep a value.
struct SavfChange {
    max: i64,
    wait: i64,
    share: String,
    text: String,
}

fn chgsavf(system: &mut System, parms: &Parms) -> Result<CmdReply, Fail> {
    let file = parms.qual("FILE");
    let ch = SavfChange {
        max: parms.int("MAXRCDS").unwrap_or(-1),
        wait: parms.int("WAITFILE").unwrap_or(-2),
        share: parms.text("SHARE"),
        text: parms.text("TEXT"),
    };
    let now = now_unix();
    in_store(system, |txn, qsys, names| {
        change_savf(txn, qsys, names, &file, &ch, now)
    })?;
    Ok(CmdReply::default())
}

/// `CHGSAVF` in the store: `*OBJOPR` and `*OBJMGT` to the save file.
fn change_savf(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    names: &ops::NameContext,
    file: &(String, String),
    ch: &SavfChange,
    now: u64,
) -> Result<(), Fail> {
    let id = find_savf(txn, qsys, names, file)?;
    if !sec::guard::allowed(txn, qsys, "savf.change", id) {
        return Err(savf_not_authorized(&file.0, &lib_or(&file.1)));
    }
    let mut a = savf::attr(txn, id).map_err(Fail::ended)?;
    if ch.max >= 0 {
        a.max_records = u32::try_from(ch.max).unwrap_or(u32::MAX);
    }
    if ch.wait != -2 {
        a.wait_file = i32::try_from(ch.wait).unwrap_or(0);
    }
    if ch.share != "*SAME" {
        a.share = ch.share == "*YES";
    }
    savf::set_attr(txn, id, a).map_err(Fail::ended)?;
    if ch.text != "*SAME" {
        ops::change_text(txn, id, &ch.text, now).map_err(Fail::ended)?;
    }
    Ok(())
}

fn clrsavf(system: &mut System, parms: &Parms) -> Result<CmdReply, Fail> {
    let file = parms.qual("FILE");
    in_store(system, |txn, qsys, names| {
        let id = find_savf(txn, qsys, names, &file)?;
        if !savsys() && !sec::guard::allowed(txn, qsys, "savf.clear", id) {
            return Err(savf_not_authorized(&file.0, &lib_or(&file.1)));
        }
        savf::clear(txn, id).map_err(Fail::ended)
    })?;
    Ok(CmdReply::default())
}

/// A save into save file `id`: `*USE` and `*ADD`, and `*OBJMGT` too
/// when it holds records. `*SAVSYS` covers the objects saved, not the
/// save file (IBM's table puts no `*SAVSYS` footnote on its rows);
/// `*ALLOBJ` passes through the search. Returns the records it holds.
pub(super) fn may_write_savf(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    id: ObjectId,
    file: &str,
) -> Result<u64, String> {
    write_allowed(txn, qsys, id, (file, "*LIBL")).map_err(|f| f.text())
}

fn write_allowed(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    id: ObjectId,
    (file, lib): (&str, &str),
) -> Result<u64, Fail> {
    let used = savf::len(txn, id).map_err(Fail::ended)?;
    let ok = if used > 0 {
        sec::guard::allowed(txn, qsys, "savf.replace", id)
    } else {
        sec::guard::allowed(txn, qsys, "savf.write", id)
    };
    if !ok {
        return Err(savf_not_authorized(file, lib));
    }
    Ok(used)
}

/// The save file a save writes into: found, allowed, and empty unless
/// `CLEAR(*ALL)` or `CLEAR(*REPLACE)` clears it.
fn save_target(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    names: &ops::NameContext,
    file: &(String, String),
    clear: &str,
) -> Result<ObjectId, Fail> {
    let lib = lib_or(&file.1);
    let id = find_savf(txn, qsys, names, file)?;
    let used = write_allowed(txn, qsys, id, (&file.0, &lib))?;
    if clear == "*AFTER" {
        return Err(Fail::new(CLEAR_AFTER, &[]));
    }
    if used > 0 && !matches!(clear, "*ALL" | "*REPLACE") {
        return Err(
            Fail::new(SAVF_NOT_EMPTY, &[Field::Char(SAVF_HOLDS_DATA, 7)]).after(
                msgout::diagnostic(
                    SAVF_HOLDS_DATA,
                    &[
                        Field::Char("", 10),
                        Field::Char(&file.0, 10),
                        Field::Char(&lib, 10),
                        Field::Char("", 10),
                        Field::Char("", 24),
                        Field::Char(SAVF_INQUIRY, 7),
                    ],
                ),
            ),
        );
    }
    Ok(id)
}

/// A restore or display from save file `id`: `*USE`, `*SAVSYS` or not.
pub(super) fn may_read_savf(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    id: ObjectId,
    file: &str,
) -> Result<(), String> {
    read_allowed(txn, qsys, id, (file, "*LIBL")).map_err(|f| f.text())
}

fn read_allowed(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    id: ObjectId,
    (file, lib): (&str, &str),
) -> Result<(), Fail> {
    if sec::guard::allowed(txn, qsys, "savf.read", id) {
        Ok(())
    } else {
        Err(savf_not_authorized(file, lib))
    }
}

/// A save file's stream, found and readable; empty when it holds none.
fn savf_bytes(system: &mut System, file: &(String, String)) -> Result<Vec<u8>, Fail> {
    let lib = lib_or(&file.1);
    in_store(system, |txn, qsys, names| {
        let id = find_savf(txn, qsys, names, file)?;
        read_allowed(txn, qsys, id, (&file.0, &lib))?;
        savf::read(txn, id).map_err(Fail::ended)
    })
}

/// A save file's stream: `CPF3707` when it holds no save.
fn read_stream(system: &mut System, file: &(String, String)) -> Result<Vec<u8>, Fail> {
    let stream = savf_bytes(system, file)?;
    if stream.is_empty() {
        return Err(savf_empty(file));
    }
    Ok(stream)
}

/// `CPF3707`: the save file holds no save.
fn savf_empty(file: &(String, String)) -> Fail {
    Fail::new(
        SAVF_EMPTY,
        &[Field::Char(&file.0, 10), Field::Char(&lib_or(&file.1), 10)],
    )
}

/// Write `bytes` into save file `id`: `CPF3808` and `CPF3815` when it
/// does not fit.
fn write_savf(
    system: &mut System,
    id: ObjectId,
    file: &(String, String),
    bytes: &[u8],
) -> Result<(), Fail> {
    let lib = lib_or(&file.1);
    in_store(system, |txn, _, _| {
        savf::write(txn, id, bytes).map_err(|e| match e {
            store::StoreError::BadValue { .. } => {
                let f = [Field::Char(&file.0, 10), Field::Char(&lib, 10)];
                Fail::new(SAVF_FULL, &f)
                    .because(e)
                    .after(msgout::diagnostic(
                        SAVF_INCOMPLETE,
                        &[
                            Field::Char("", 17),
                            Field::Char(&file.0, 10),
                            Field::Char(&lib, 10),
                        ],
                    ))
            }
            e => Fail::ended(e),
        })
    })
}

/// Text values of a list parameter, uppercase.
/// `FRCOBJCVN`: `*SYSVAL` is `QFRCCVNRST`, `*NO` converts nothing,
/// `(*YES *RQD)` what must be converted (level 2), `(*YES *ALL)`
/// everything (level 7).
fn convert_level(system: &System, parms: &Parms) -> u8 {
    let t = parms.name("FRCOBJCVN");
    let words: Vec<&str> = t
        .split(|c: char| c.is_whitespace() || c == '(' || c == ')')
        .filter(|w| !w.is_empty())
        .collect();
    if words.contains(&"*ALL") {
        7
    } else if words.contains(&"*YES") || words.contains(&"*RQD") {
        2
    } else if words.contains(&"*NO") {
        0
    } else {
        crate::stgsvc::security::sysval_num(system, "QFRCCVNRST", 1)
    }
}

pub(super) fn values(parms: &Parms, kwd: &str) -> Vec<String> {
    parms
        .list(kwd)
        .iter()
        .map(|v| v.source().to_ascii_uppercase())
        .collect()
}

/// `OMITOBJ` (and friends): `((lib/obj type) ...)`.
fn objsels(parms: &Parms, kwd: &str) -> Vec<(ObjSel, String)> {
    let mut out = Vec::new();
    for item in parms.list(kwd) {
        let Value::Elems(e) = item else { continue };
        let (obj, lib) = match e.first().cloned().flatten() {
            Some(Value::Qual(q)) => (
                q.first().map(Value::source).unwrap_or_default(),
                q.get(1).map_or_else(|| "*ALL".into(), Value::source),
            ),
            Some(v) => (v.source(), "*ALL".into()),
            None => continue,
        };
        let ty = e
            .get(1)
            .cloned()
            .flatten()
            .map(|v| v.source())
            .unwrap_or_default();
        out.push((
            ObjSel {
                name: obj.to_ascii_uppercase(),
                type_code: type_code(&ty),
            },
            lib.to_ascii_uppercase(),
        ));
    }
    out
}

fn type_code(ty: &str) -> Option<u16> {
    if ty.is_empty() || ty.eq_ignore_ascii_case("*ALL") {
        None
    } else {
        types::type_by_name(ty).map(|t| t.code)
    }
}

/// The configuration object types `SAVCFG` saves and `RSTCFG` restores.
pub(super) const CFG_TYPES: &[u16] = &[types::DEVD, 0x001C, 0x001D];

/// `REFDATE`/`REFTIME` as Unix seconds, `None` for `*SAVLIB`. Dates
/// arrive as `CYYMMDD`, times as `HHMMSS`.
fn refdate(parms: &Parms) -> Option<u64> {
    let d = parms.text("REFDATE");
    let d = d.trim();
    if d.is_empty() || d == "*SAVLIB" || d.trim_matches('0').is_empty() {
        return None;
    }
    let n: u64 = d.parse().ok()?;
    let (c, yy, mm, dd) = (n / 1_000_000, n / 10_000 % 100, n / 100 % 100, n % 100);
    let year = i64::try_from(1900 + c * 100 + yy).ok()?;
    let days = days_from_civil(year, u32::try_from(mm).ok()?, u32::try_from(dd).ok()?);
    let t: u64 = parms.text("REFTIME").trim().parse().unwrap_or(0);
    let secs = t / 10_000 * 3600 + t / 100 % 100 * 60 + t % 100;
    u64::try_from(days).ok().map(|d| d * 86_400 + secs)
}

/// Days since 1970-01-01 of a civil date.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = i64::from((m + 9) % 12);
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// When `SAVLIB` last saved `lib`, for `REFDATE(*SAVLIB)`; 0 for never.
fn last_savlib(system: &mut System, lib: &str) -> u64 {
    in_store(system, |txn, qsys, _| {
        let n = qsname::Name::parse(lib).map_err(Fail::ended)?;
        let id = ops::find_library(txn, qsys, n).map_err(Fail::ended)?.id;
        let (_, h) = ops::read_header(txn, id).map_err(Fail::ended)?;
        Ok(
            if store::obj::header::text_of(&h.save.command) == "SAVLIB" {
                h.last_saved
            } else {
                0
            },
        )
    })
    .unwrap_or(0)
}

/// Never saved by `SAVLIB`: the system's own and the per-IPL ones.
const NEVER: &[&str] = &[
    "QSYS",
    "QTEMP",
    "QSPL",
    "QRECOVERY",
    "QRPLOBJ",
    "QSRV",
    "QDOC",
];
/// `Q` libraries that hold user data (`*ALLUSR`).
const USER_Q: &[&str] = &[
    "QGPL", "QUSRSYS", "QSYS2", "QRCL", "QS36F", "QUSER38", "QPFRDATA", "QUSRBRM",
];

/// The files on a medium that hold the libraries `wanted` names
/// (`*NONSYS`, `*ALLUSR`, `*IBM`): each file labeled with a library's
/// name, once, without the labels of the other saves (configuration,
/// security data, IFS, install media) and `omit`.
pub(super) fn library_files(labels: &[String], wanted: &[String], omit: &[String]) -> Vec<String> {
    let other = |l: &str| {
        matches!(
            l,
            "QFILEIOC" | "QFILEUPR" | IFS_LABEL | instmedia::layout::MANIFEST_LABEL
        ) || instmedia::Payload::from_label(l).is_some()
    };
    let mut out: Vec<String> = Vec::new();
    for l in labels.iter().map(|l| l.trim().to_ascii_uppercase()) {
        if other(&l)
            || NEVER.contains(&l.as_str())
            || qsname::Name::parse(&l).is_err()
            || out.contains(&l)
        {
            continue;
        }
        if omit.iter().any(|o| saverst::select::generic(o, &l)) {
            continue;
        }
        let user = !l.starts_with('Q') || USER_Q.contains(&l.as_str());
        let take = wanted.iter().any(|s| match s.as_str() {
            "*NONSYS" => true,
            "*ALLUSR" => user,
            "*IBM" => !user,
            lib => *lib == l,
        });
        if take {
            out.push(l);
        }
    }
    out
}

/// The libraries `*NONSYS`, `*ALLUSR` and `*IBM` mean.
pub(super) fn expand_libs(system: &mut System, libs: &[String]) -> Result<Vec<String>, String> {
    let special = |l: &str| matches!(l, "*NONSYS" | "*ALLUSR" | "*IBM");
    if !libs.iter().any(|l| special(l)) {
        return Ok(libs.to_vec());
    }
    let all: Vec<String> = in_store(system, |txn, qsys, _| {
        Ok(ops::libraries(txn, qsys)
            .map_err(Fail::ended)?
            .into_iter()
            .map(|(n, _)| n.to_string())
            .collect())
    })
    .map_err(|f| f.text())?;
    let mut out = Vec::new();
    for l in &all {
        if NEVER.contains(&l.as_str()) {
            continue;
        }
        let user = !l.starts_with('Q') || USER_Q.contains(&l.as_str());
        let take = libs.iter().any(|s| match s.as_str() {
            "*NONSYS" => true,
            "*ALLUSR" => user,
            "*IBM" => !user,
            other => other == l,
        });
        if take {
            out.push(l.clone());
        }
    }
    Ok(out)
}

fn compression(parms: &Parms) -> Compression {
    Compression::from_param(&parms.text("DTACPR")).unwrap_or(Compression::None)
}

fn header(system: &System, command: &str, parms: &Parms, kind: SaveKind) -> MediaHeader {
    header_with(system, command, compression(parms), kind)
}

/// A stream's media header for `command`.
pub(super) fn header_with(
    system: &System,
    command: &str,
    compression: Compression,
    kind: SaveKind,
) -> MediaHeader {
    let now = now_unix();
    let mut id = [0u8; 16];
    id[..8].copy_from_slice(&now.to_be_bytes());
    id[8..12].copy_from_slice(&system.current_job.to_be_bytes());
    id[12..].copy_from_slice(&std::process::id().to_be_bytes());
    let r = release::RELEASE;
    MediaHeader {
        release: (r.v, r.r, r.m),
        target: (r.v, r.r, r.m),
        compression,
        kind,
        created: now,
        stream_id: id,
        system: "QS400".into(),
        serial: crate::stgsvc::security::sysval_text(system, "QSRLNBR"),
        command: command.into(),
        user: user(system),
    }
}

fn tgtrls_ok(v: &str) -> bool {
    let r = release::RELEASE;
    matches!(v, "*CURRENT" | "*PRV" | "") || v == format!("V{}R{}M{}", r.v, r.r, r.m)
}

/// The media parameters when `DEV` names a device; `None` for `*SAVF`.
fn media_of(parms: &Parms) -> Option<Media> {
    let dev = values(parms, "DEV");
    dev.first()
        .filter(|d| stream::is_device(d))
        .map(|d| stream::of(parms, d))
}

/// Where a save to media went, for its messages.
fn media_dest(dev: &str, w: &medio::Written) -> Dest {
    Dest::Media {
        volumes: w.volumes.join(" "),
        device: dev.to_string(),
        last: w.volumes.last().cloned().unwrap_or_default(),
    }
}

/// A library the engine refused before saving anything: its own
/// diagnostic (`CPF3781`, `CPF3761`, `CPF3770`).
fn refused_lib(msgid: &'static str, lib: &str, now: u64) -> CmdMsg {
    let data = if msgid == msg::NONE_FOR_LIB {
        Data::new().char(lib, 10).dts(now).into_bytes()
    } else {
        msg::object(msgid, lib, types::LIB, "QSYS")
    };
    message(msgid, DIAG, data)
}

#[allow(clippy::too_many_lines)]
fn save(system: &mut System, name: &str, parms: &Parms) -> Result<CmdReply, Fail> {
    let dev = values(parms, "DEV");
    let media = media_of(parms);
    if media.is_none() && dev.first().map(String::as_str) != Some("*SAVF") {
        return Err(no_device(&dev));
    }
    let savact = parms.text("SAVACT");
    if !savact.is_empty() && savact != "*NO" {
        return Err(Fail::new(NOT_AVAILABLE, &[]).because(format!("SAVACT({savact})")));
    }
    let tgtrls = parms.text("TGTRLS");
    if !tgtrls_ok(&tgtrls) {
        return Err(Fail::new(BAD_TGTRLS, &[]).because(format!("TGTRLS({tgtrls})")));
    }
    let savf_name = parms.qual("SAVF");
    let libs = if name == "SAVCFG" {
        vec!["QSYS".to_string()]
    } else {
        values(parms, "LIB")
    };
    let libs = expand_libs(system, &libs).map_err(Fail::from)?;
    let omit_libs = values(parms, "OMITLIB");
    let omit = objsels(parms, "OMITOBJ");
    let select: Vec<ObjSel> = if name == "SAVCFG" {
        CFG_TYPES
            .iter()
            .map(|t| ObjSel {
                name: "*ALL".into(),
                type_code: Some(*t),
            })
            .collect()
    } else if matches!(name, "SAVOBJ" | "SAVCHGOBJ") {
        let types: Vec<Option<u16>> = values(parms, "OBJTYPE")
            .iter()
            .map(|t| type_code(t))
            .collect();
        let objs = values(parms, "OBJ");
        objs.iter()
            .flat_map(|o| {
                types.iter().map(|t| ObjSel {
                    name: o.clone(),
                    type_code: *t,
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    let clear = parms.text("CLEAR");
    let now = now_unix();
    let base = SaveOptions {
        select,
        omit: Vec::new(),
        library_object: name == "SAVLIB",
        skip_journaled: name == "SAVCHGOBJ" && parms.text("OBJJRN") != "*YES",
        update_history: parms.text("UPDHST") != "*NO",
        save_file_data: parms.text("SAVFDTA") != "*NO",
        spool: parms.text("SPLFDTA") == "*ALL",
        target_savf: None,
        private_auth: parms.text("PVTAUT") == "*YES",
        changed_since: None,
        now,
        target: match &media {
            Some(m) => SaveTarget {
                command: name.into(),
                device: m.dev.clone(),
                file: None,
                volume: m.vols.first().cloned().unwrap_or_else(|| "*MOUNTED".into()),
                label: if m.label.is_empty() {
                    libs.first().cloned().unwrap_or_default()
                } else {
                    m.label.clone()
                },
                seq: 0,
            },
            None => SaveTarget {
                command: name.into(),
                device: "*SAVF".into(),
                file: Some((savf_name.0.clone(), lib_or(&savf_name.1))),
                ..SaveTarget::default()
            },
        },
    };
    // The save file: found, allowed, and empty unless CLEAR says.
    let savf_id = if media.is_some() {
        None
    } else {
        Some(in_store(system, |txn, qsys, names| {
            save_target(txn, qsys, names, &savf_name, &clear)
        })?)
    };
    let base = SaveOptions {
        target_savf: savf_id,
        ..base
    };
    let kind = match name {
        "SAVLIB" => SaveKind::Library,
        "SAVCHGOBJ" => SaveKind::Changed,
        "SAVCFG" => SaveKind::Config,
        _ => SaveKind::Objects,
    };
    let since = refdate(parms);
    let mut changed_since = std::collections::HashMap::new();
    if name == "SAVCHGOBJ" {
        for lib in &libs {
            let t = since.unwrap_or_else(|| last_savlib(system, lib));
            changed_since.insert(lib.clone(), t);
        }
    }
    let head = header(system, name, parms, kind);
    let spool = if base.spool {
        spool_parts(system, &libs)?
    } else {
        std::collections::HashMap::new()
    };
    let jobh = job(system);
    let (store, qsys) = storage(system)?;
    let hooks = SvcHooks { qsys, spool };
    // A save of several libraries to media is a file per library, as
    // IBM writes it; to a save file it is one stream.
    let per_lib =
        media.as_ref().is_some_and(|m| m.label.is_empty()) && name != "SAVCFG" && libs.len() > 1;
    let new_writer = || {
        Writer::new(Vec::new(), &head).map(|mut w| {
            let _ = w.parameters(&parms.to_line(name));
            w
        })
    };
    let mut w = new_writer().map_err(Fail::ended)?;
    let mut streams: Vec<(String, Vec<u8>)> = Vec::new();
    let mut reports: Vec<SaveReport> = Vec::new();
    let mut refused: Vec<CmdMsg> = Vec::new();
    for lib in &libs {
        if omit_libs.iter().any(|o| saverst::select::generic(o, lib)) {
            continue;
        }
        let opts = SaveOptions {
            omit: omit
                .iter()
                .filter(|(_, l)| saverst::select::generic(l, lib))
                .map(|(s, _)| s.clone())
                .collect(),
            changed_since: changed_since.get(lib).copied(),
            ..base.clone()
        };
        match saverst::save_library(store, jobh, &hooks, qsys, lib, &opts, &mut w) {
            Ok(r) => {
                let ns = r.not_saved();
                reports.push(r);
                if per_lib {
                    let next = new_writer().map_err(Fail::ended)?;
                    let (_, b) = std::mem::replace(&mut w, next)
                        .finish(u32::try_from(ns).unwrap_or(u32::MAX))
                        .map_err(Fail::ended)?;
                    streams.push((lib.clone(), b));
                }
            }
            Err(saverst::Error::Refused { msgid, .. }) => {
                reports.push(SaveReport {
                    library: lib.clone(),
                    objects: Vec::new(),
                    notes: Vec::new(),
                });
                refused.push(refused_lib(msgid, lib, now));
            }
            Err(e) => return Err(Fail::ended(e)),
        }
    }
    if !per_lib {
        let not_saved: usize = reports.iter().map(SaveReport::not_saved).sum();
        let (_, b) = w
            .finish(u32::try_from(not_saved).unwrap_or(u32::MAX))
            .map_err(Fail::ended)?;
        streams.push((libs.first().cloned().unwrap_or_default(), b));
    }
    // Where each library's save went.
    let mut dests: Vec<Dest> = Vec::new();
    if let Some(m) = &media {
        let mut cur = m.clone();
        let n = streams.len();
        for (i, (lib, bytes)) in streams.iter().enumerate() {
            let mut each = cur.clone();
            if i + 1 < n {
                each.end_opt = tape::EndOpt::Leave;
            }
            let (label, target) = if name == "SAVCFG" {
                ("QFILEIOC".to_string(), OptTarget::System("SAVCFG"))
            } else {
                (lib.clone(), OptTarget::Library(lib))
            };
            let w = stream::save(system, &each, &label, target, bytes)?;
            dests.push(media_dest(&m.dev, &w));
            // The next library's file follows where this one ended.
            cur.seq = 0;
            if let Some(at) = w
                .volumes
                .last()
                .and_then(|l| cur.vols.iter().position(|v| v == l))
            {
                cur.vols = cur.vols[at..].to_vec();
            }
        }
    } else if let Some(savf_id) = savf_id {
        let bytes = streams.pop().map(|(_, b)| b).unwrap_or_default();
        write_savf(system, savf_id, &savf_name, &bytes)?;
        let q = Dest::Savf(savf_name.0.clone(), lib_or(&savf_name.1));
        dests.push(q);
    }
    let mut reply = save_reply(&reports, &dests, now);
    reply.messages.splice(0..0, refused);
    if parms.text("OUTPUT") == "*OUTFILE" {
        let q = (savf_name.0.clone(), lib_or(&savf_name.1));
        let (_, m) = super::savout::save(system, parms, name, &reports, &q, now)?;
        reply.messages.extend(m);
    }
    Ok(reply)
}

/// The messages of a save: each object not saved, each library's end,
/// and the escape when not everything was saved. `dests` is where each
/// library went (one for all of them when the save was one stream).
fn save_reply(reports: &[SaveReport], dests: &[Dest], now: u64) -> CmdReply {
    let mut reply = CmdReply::default();
    let none = Dest::Savf(String::new(), String::new());
    let mut ends = Vec::new();
    for (i, r) in reports.iter().enumerate() {
        for n in &r.notes {
            reply.messages.push(message(n.msgid, INFO, n.data()));
        }
        for (id, data) in r.objects.iter().filter_map(msg::of_result) {
            reply.messages.push(message(id, DIAG, data));
        }
        let dest = dests.get(i).or(dests.last()).unwrap_or(&none);
        ends.push(msg::save_end(r, dest, now));
    }
    let done = |id: &str| matches!(id, msg::SAVED_LIB | msg::SAVED_LIB_MEDIA);
    if let [(id, data)] = ends.as_slice() {
        if done(id) {
            reply.messages.push(message(id, COMP, data.clone()));
        } else {
            reply.escape = Some(message(id, ESC, data.clone()));
        }
        return reply;
    }
    let full = ends.iter().filter(|(id, _)| done(id)).count();
    let none = ends
        .iter()
        .filter(|(id, _)| *id == msg::NONE_FOR_LIB)
        .count();
    let partial = ends.len() - full - none;
    for (id, data) in ends {
        let kind = if done(id) { COMP } else { DIAG };
        reply.messages.push(message(id, kind, data));
    }
    // A library with nothing in it does not fail the save: only objects
    // not saved, or nothing saved at all, end it with the escape.
    let saved: usize = reports.iter().map(SaveReport::saved).sum();
    let not: usize = reports.iter().map(SaveReport::not_saved).sum();
    if not > 0 || saved == 0 {
        reply.escape = Some(message(
            msg::SAVED_LIBS_PARTIAL,
            ESC,
            msg::libraries_end(full, none, partial, now),
        ));
    }
    reply
}

fn rst_option(s: &str) -> RstOption {
    match s {
        "*NEW" => RstOption::New,
        "*OLD" => RstOption::Old,
        "*FREE" => RstOption::Free,
        _ => RstOption::All,
    }
}

fn mbr_option(s: &str) -> MbrOpt {
    match s {
        "*ALL" => MbrOpt::All,
        "*NEW" => MbrOpt::New,
        "*OLD" => MbrOpt::Old,
        _ => MbrOpt::Match,
    }
}

#[allow(clippy::too_many_lines)]
fn rst(system: &mut System, name: &str, parms: &Parms) -> Result<CmdReply, Fail> {
    let dev = values(parms, "DEV");
    let media = media_of(parms);
    if media.is_none() && dev.first().map(String::as_str) != Some("*SAVF") {
        return Err(no_device(&dev));
    }
    let alw = values(parms, "ALWOBJDIF");
    if alw.iter().any(|v| v == "*ALL") {
        if alw.len() > 1 {
            return Err(Fail::new("CPD0094", &[Field::Char("ALWOBJDIF", 10)]));
        }
        if name != "RSTCFG" && parms.text("MBROPT") != "*ALL" {
            return Err(Fail::new("CPD3768", &[]));
        }
    }
    let savf_name = parms.qual("SAVF");
    let saved = if name == "RSTCFG" {
        vec!["QSYS".to_string()]
    } else {
        values(parms, "SAVLIB")
    };
    let rstlib = parms.text("RSTLIB");
    let target = (!rstlib.is_empty() && rstlib != "*SAVLIB").then(|| rstlib.to_ascii_uppercase());
    let select: Vec<ObjSel> = if name == "RSTCFG" {
        let wanted: Vec<u16> = values(parms, "OBJTYPE")
            .iter()
            .filter_map(|t| type_code(t))
            .filter(|t| CFG_TYPES.contains(t))
            .collect();
        let types = if wanted.is_empty() {
            CFG_TYPES.to_vec()
        } else {
            wanted
        };
        values(parms, "OBJ")
            .iter()
            .flat_map(|o| {
                types.iter().map(|t| ObjSel {
                    name: o.clone(),
                    type_code: Some(*t),
                })
            })
            .collect()
    } else if name == "RSTOBJ" {
        let types: Vec<Option<u16>> = values(parms, "OBJTYPE")
            .iter()
            .map(|t| type_code(t))
            .collect();
        values(parms, "OBJ")
            .iter()
            .flat_map(|o| {
                types.iter().map(|t| ObjSel {
                    name: o.clone(),
                    type_code: *t,
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    let opts = RestoreOptions {
        option: rst_option(&parms.text("OPTION")),
        mbropt: mbr_option(&parms.text("MBROPT")),
        allow: AllowDiff::from_values(&values(parms, "ALWOBJDIF")),
        select,
        omit: objsels(parms, "OMITOBJ")
            .into_iter()
            .map(|(s, _)| s)
            .collect(),
        libraries: saved
            .iter()
            .filter(|l| !matches!(l.as_str(), "*NONSYS" | "*ALLUSR" | "*IBM"))
            .cloned()
            .collect(),
        omit_libraries: values(parms, "OMITLIB"),
        target_library: target,
        private_auth: parms.text("PVTAUT") == "*YES",
        default_owner: "QDFTOWN".into(),
        system: "QS400".into(),
        now: now_unix(),
        verify: crate::stgsvc::security::sysval_num(system, "QVFYOBJRST", 3),
        convert: convert_level(system, parms),
        allow_restore: {
            let v = crate::stgsvc::security::sysval_text(system, "QALWOBJRST");
            AllowRestore::from_value(if v.is_empty() { "*ALL" } else { &v })
        },
    };
    let first = saved.first().cloned().unwrap_or_default();
    // A save of several libraries on media is a file per library.
    let named: Vec<String> = saved
        .iter()
        .filter(|l| !l.starts_with('*'))
        .cloned()
        .collect();
    let reads: Vec<(String, Result<Vec<u8>, Fail>)> = if let Some(m) = &media {
        let each = Media {
            seq: 0,
            ..m.clone()
        };
        let read = |system: &mut System, l: &str| {
            stream::restore(system, &each, Some(l), OptTarget::Library(l)).map_err(Fail::from)
        };
        if name == "RSTCFG" {
            vec![(
                first.clone(),
                stream::restore(system, m, Some("QFILEIOC"), OptTarget::System("SAVCFG"))
                    .map_err(Fail::from),
            )]
        } else if name == "RSTLIB" && named.len() > 1 && m.label.is_empty() {
            named.iter().map(|l| (l.clone(), read(system, l))).collect()
        } else if name == "RSTLIB" && named.is_empty() && m.label.is_empty() {
            // *NONSYS, *ALLUSR, *IBM: every library file on the medium.
            match stream::file_labels(system, m) {
                Ok(labels) => {
                    let libs = library_files(&labels, &saved, &values(parms, "OMITLIB"));
                    if libs.is_empty() {
                        return Err(Fail::new(msg::NONE_IN_SAVE, &[Field::Char(&first, 10)])
                            .because(format!("no library saves on device {}", m.dev)));
                    }
                    libs.iter().map(|l| (l.clone(), read(system, l))).collect()
                }
                Err(e) => vec![(first.clone(), Err(Fail::from(e)))],
            }
        } else {
            let label = (!first.starts_with('*')).then_some(first.as_str());
            vec![(
                first.clone(),
                stream::restore(system, m, label, OptTarget::Library(&first)).map_err(Fail::from),
            )]
        }
    } else {
        vec![(first.clone(), savf_bytes(system, &savf_name))]
    };
    let mut all: Option<CmdReply> = None;
    for (lib, read) in reads {
        let one = read
            .and_then(|s| restore_stream(system, name, parms, &opts, &savf_name, &lib, &s))
            .unwrap_or_else(Fail::reply);
        all = Some(match all {
            None => one,
            Some(mut a) => {
                a.messages.extend(one.messages);
                if a.escape.is_none() {
                    a.escape = one.escape;
                }
                a
            }
        });
    }
    all.ok_or_else(|| Fail::new(msg::NONE_IN_SAVE, &[Field::Char(&first, 10)]))
}

/// Restore one save stream of library `saved` read from a save file or
/// a medium.
fn restore_stream(
    system: &mut System,
    name: &str,
    parms: &Parms,
    opts: &RestoreOptions,
    savf_name: &(String, String),
    saved: &str,
    stream: &[u8],
) -> Result<CmdReply, Fail> {
    if stream.is_empty() {
        return Err(savf_empty(savf_name));
    }
    let jobh = job(system);
    let (store, qsys) = storage(system)?;
    let hooks = SvcHooks::new(qsys);
    let mut reader = Reader::new(Cursor::new(stream)).map_err(bad_save)?;
    let r = reader.header().target;
    let cur = release::RELEASE;
    if (r.0, r.1, r.2) > (cur.v, cur.r, cur.m) {
        return Err(bad_save(format!(
            "The save is for release V{}R{}M{}.",
            r.0, r.1, r.2
        )));
    }
    // The hooks decide who may restore; the objects are made by the
    // system so they keep their saved owners.
    let r =
        sec::guard::as_system(|| saverst::restore(store, jobh, &hooks, qsys, &mut reader, opts))
            .map_err(Fail::ended)?;
    let (mut made, mut kept) = (0, 0);
    if parms.text("SPLFDTA") != "*NONE" {
        for (lib, q, part) in &r.spool {
            let (m, k) = restore_spool(system, &format!("{lib}/{q}"), part);
            made += m;
            kept += k;
        }
    }
    // The library saved: the file's label, else what the objects say.
    let saved = if saved.is_empty() || saved.starts_with('*') {
        r.objects
            .first()
            .map(|o| o.library.clone())
            .or_else(|| r.created_libraries.first().cloned())
            .unwrap_or_default()
    } else {
        saved.to_string()
    };
    let to = opts.target_library.clone().unwrap_or_else(|| saved.clone());
    let from = savf_of(parms, savf_name);
    let mut reply = rst_reply(&r, (&saved, &to), &from, made, now_unix());
    if kept > 0 {
        let data = Data::new().bin4(kept).into_bytes();
        reply.messages.push(message(msg::SPOOL_EXISTS, INFO, data));
    }
    if parms.text("OUTPUT") == "*OUTFILE" {
        let (_, m) = super::savout::restore(system, parms, name, &r, savf_name, now_unix())?;
        reply.messages.extend(m);
    }
    Ok(reply)
}

/// `CPF3743`: the stream is not a save this release can read; `why`
/// fills the message's `&1`.
fn bad_save(why: impl Display) -> Fail {
    Fail::new(BAD_SAVE, &[Field::Char(&why.to_string(), 364)])
}

/// The save file a restore read, blank for media.
fn savf_of(parms: &Parms, savf_name: &(String, String)) -> (String, String) {
    if media_of(parms).is_some() {
        (String::new(), String::new())
    } else {
        (savf_name.0.clone(), lib_or(&savf_name.1))
    }
}

/// The spool parts of every output queue in `libs`: each file's
/// attribute record and data.
fn spool_parts(
    system: &mut System,
    libs: &[String],
) -> Result<std::collections::HashMap<String, Vec<u8>>, Fail> {
    let mut queues = Vec::new();
    for lib in libs {
        let names = in_store(system, |txn, qsys, _| {
            let Ok(found) = qsname::Name::parse(lib).map(|n| ops::find_library(txn, qsys, n))
            else {
                return Ok(Vec::new());
            };
            let Ok(found) = found else {
                return Ok(Vec::new());
            };
            Ok(store::obj::library::Library::load(txn, found.id)
                .and_then(|l| l.list(txn, Some(types::OUTQ)))
                .map_err(Fail::ended)?
                .into_iter()
                .map(|(k, _)| k.name.to_string())
                .collect::<Vec<_>>())
        })?;
        queues.extend(names.into_iter().map(|n| format!("{lib}/{n}")));
    }
    let mut out = std::collections::HashMap::new();
    for q in queues {
        let Ok((_, _, files)) = crate::stgsvc::spool::on_queue(system, &q) else {
            continue;
        };
        let mut b = Vec::new();
        b.extend_from_slice(&u32::try_from(files.len()).unwrap_or(0).to_be_bytes());
        for f in &files {
            let (attrs, data) = crate::stgsvc::spool::data(system, f.id).map_err(Fail::ended)?;
            let rec = attrs.encode();
            b.extend_from_slice(&u32::try_from(rec.len()).unwrap_or(0).to_be_bytes());
            b.extend_from_slice(&rec);
            b.extend_from_slice(&(data.len() as u64).to_be_bytes());
            b.extend_from_slice(&data);
        }
        out.insert(q, b);
    }
    Ok(out)
}

/// Bring back the spooled files of a restored output queue that are
/// not on the system (`SPLFDTA(*NEW)`): how many, and how many were
/// there already.
fn restore_spool(system: &mut System, q: &str, part: &[u8]) -> (usize, usize) {
    let take = |at: &mut usize, n: usize| -> Option<&[u8]> {
        let s = part.get(*at..*at + n)?;
        *at += n;
        Some(s)
    };
    let mut at = 0;
    let Some(count) = take(&mut at, 4).map(|b| u32::from_be_bytes(b.try_into().expect("4"))) else {
        return (0, 0);
    };
    let (mut made, mut kept) = (0, 0);
    for _ in 0..count {
        let Some(len) = take(&mut at, 4).map(|b| u32::from_be_bytes(b.try_into().expect("4")))
        else {
            break;
        };
        let Some(rec) = take(&mut at, len as usize) else {
            break;
        };
        let Ok(saved) = wm::obj::splf::SplfAttrs::decode(rec) else {
            break;
        };
        let Some(dlen) = take(&mut at, 8).map(|b| u64::from_be_bytes(b.try_into().expect("8")))
        else {
            break;
        };
        let Some(data) = take(&mut at, usize::try_from(dlen).unwrap_or(usize::MAX)) else {
            break;
        };
        // Saved before the published PP codes: converted as at IPL.
        let old =
            wm::obj::splf::record_version(rec).is_ok_and(|v| v == wm::obj::splf::VERSION_OLD_PP);
        let converted = old
            .then(|| crate::stgsvc::spool::published(&saved, data))
            .flatten();
        let data = converted.as_deref().unwrap_or(data);
        let exists = system.spool.files.values().any(|e| {
            e.attrs.job == saved.job
                && e.attrs.name == saved.name
                && e.attrs.created == saved.created
        });
        if exists {
            kept += 1;
            continue;
        }
        let mut attrs = saved.clone();
        attrs.outq = q.to_string();
        let Ok(id) = crate::stgsvc::spool::create(system, attrs) else {
            continue;
        };
        let _ = crate::stgsvc::spool::append(system, id, data, saved.pages, saved.records);
        let hold = saved.status == wm::obj::splf::Status::Hld;
        let _ = crate::stgsvc::spool::close(system, id, hold);
        let _ = crate::stgsvc::spool::update(system, id, |a| {
            a.number = saved.number;
            a.created = saved.created;
            Ok(())
        });
        made += 1;
    }
    (made, kept)
}

/// The kind a note goes to the job log as: a diagnostic when something
/// was lost, else information.
fn note_kind(id: &str) -> u8 {
    if matches!(id, msg::AUTL_MISSING | msg::JOURNAL_MISSING) {
        DIAG
    } else {
        INFO
    }
}

/// The messages of one library's restore: the objects `OPTION` left
/// out, the notes, the names not in the save, each object not restored,
/// the changes accepted and the end. `(saved, to)` are the library
/// saved and the one restored into, `savf` the save file read (blank
/// for media), `spooled` the spooled files restored.
fn rst_reply(
    r: &RestoreReport,
    (saved, to): (&str, &str),
    savf: &(String, String),
    spooled: usize,
    now: u64,
) -> CmdReply {
    let mut reply = CmdReply::default();
    for n in &r.skipped {
        reply.messages.push(message(n.msgid, INFO, n.data()));
    }
    for n in &r.notes {
        reply
            .messages
            .push(message(n.msgid, note_kind(n.msgid), n.data()));
    }
    if r.restored() == 0 && r.not_restored() == 0 && !r.not_found.is_empty() {
        // Nothing named was there: once, for the save file (rule 24).
        let data = Data::new().char(&savf.0, 10).char(&savf.1, 10).into_bytes();
        reply.messages.push(message(msg::NOT_IN_SAVE, DIAG, data));
    } else {
        for n in &r.not_found {
            let data = Data::new()
                .char(n, 10)
                .char("*ALL", 7)
                .char(saved, 10)
                .char(&savf.0, 10)
                .char(&savf.1, 10)
                .into_bytes();
            reply.messages.push(message(msg::NOT_IN_LIST, DIAG, data));
        }
    }
    for o in &r.objects {
        let Some((id, data)) = msg::of_result(o) else {
            continue;
        };
        reply.messages.push(message(id, DIAG, data));
        // These reasons are followed by the general one (rules 6-11).
        if matches!(
            id,
            msg::JOURNAL_EXISTS
                | msg::DTAQ_EXISTS
                | msg::RECEIVER_ATTACHED
                | msg::NOT_RESTORED_BASED_ON
                | msg::NOT_RESTORED_MEMBERS
        ) {
            let data = msg::object(msg::NOT_RESTORED, &o.name, o.type_code, &o.library);
            reply.messages.push(message(msg::NOT_RESTORED, DIAG, data));
        }
    }
    if r.changes > 0 {
        let data = Data::new().bin4(r.changes).char(to, 10).into_bytes();
        reply.messages.push(message(msg::CHANGES, DIAG, data));
    }
    let (id, data) = msg::restore_end(r, saved, to, spooled, now);
    if id == msg::RESTORED_LIB {
        reply.messages.push(message(id, COMP, data));
    } else {
        reply.escape = Some(message(id, ESC, data));
    }
    reply
}

/// The save of a security command: from the device, or the save file.
fn read_savf(system: &mut System, parms: &Parms) -> Result<Vec<u8>, Fail> {
    let dev = values(parms, "DEV");
    if let Some(m) = media_of(parms) {
        return Ok(stream::restore(
            system,
            &m,
            Some(SECDTA_LABEL),
            OptTarget::System("SAVSECDTA"),
        )?);
    }
    if dev.first().map(String::as_str) != Some("*SAVF") {
        return Err(no_device(&dev));
    }
    read_stream(system, &parms.qual("SAVF"))
}

/// `SAVSAVFDTA SAVF() DEV()`: the save file's stream onto tape or
/// optical as the save that made it would have written it: labeled
/// with the library saved (`QFILEIOC`, `QFILEUPR` for the system's
/// saves), so `DSPTAP` shows the original save and `RSTLIB`/`RSTOBJ`
/// read it back; `CPC3724` when it is written.
fn savsavfdta(system: &mut System, parms: &Parms) -> Result<CmdReply, Fail> {
    // BLOCKED(rel): SAVSAVFDTA's OUTPUT(*PRINT|*OUTFILE) listing and
    // USEOPTBLK(*NO) are not made; the file is written in 64 KiB blocks.
    // COMPACT is the device's (*DEV), and a virtual device compacts nothing.
    for (k, dft) in [("OUTPUT", "*NONE"), ("USEOPTBLK", "*YES")] {
        let v = parms.name(k);
        if !v.is_empty() && v != dft {
            return Err(Fail::new(NOT_AVAILABLE, &[]).because(format!("{k}({v})")));
        }
    }
    let Some(m) = media_of(parms) else {
        return Err(no_device(&values(parms, "DEV")));
    };
    let savf_name = parms.qual("SAVF");
    let savlib = lib_or(&savf_name.1);
    let stream = read_stream(system, &savf_name)?;
    let (desc, _) = listing(&stream)?;
    // The listing's first two rows: the save command and the library.
    let command = desc.first().map(|(_, v)| v.clone()).unwrap_or_default();
    let lib = desc.get(1).map(|(_, v)| v.clone()).unwrap_or_default();
    let (label, target) = match command.as_str() {
        "SAVSECDTA" => (SECDTA_LABEL.to_string(), OptTarget::System("SAVSECDTA")),
        "SAVCFG" => ("QFILEIOC".to_string(), OptTarget::System("SAVCFG")),
        _ if lib.is_empty() => {
            return Err(Fail::new(
                "CPF3814",
                &[Field::Char(&savf_name.0, 10), Field::Char(&savlib, 10)],
            ));
        }
        _ => (lib.clone(), OptTarget::Library(&lib)),
    };
    let w = stream::save(system, &m, &label, target, &stream)?;
    let micros = i64::try_from(now_unix()).unwrap_or(0) * 1_000_000;
    Ok(msgout::completion(
        "CPC3724",
        &[
            Field::Char("", 18),
            Field::Char(&w.volumes.join(" "), 70),
            Field::Dts(micros),
            Field::Char("", 4),
            Field::Char("", 15),
            Field::Bin4(0),
            Field::Char("", 6),
            Field::Char(&m.dev, 10),
            Field::Char(&savf_name.0, 10),
            Field::Char(&savlib, 10),
            Field::Char("", 82),
            Field::Char("", 455),
            Field::Char("", 10),
            Field::Char(w.volumes.last().map_or("", String::as_str), 32),
        ],
    ))
}

/// `SAVSECDTA`: profiles and authorization lists to a save file.
fn savsecdta(system: &mut System, parms: &Parms) -> Result<CmdReply, Fail> {
    const NAME: &str = "SAVSECDTA";
    need(SpecialAuth::SAVSYS, "*SAVSYS", "save security data")?;
    let dev = values(parms, "DEV");
    let media = media_of(parms);
    if media.is_none() && dev.first().map(String::as_str) != Some("*SAVF") {
        return Err(no_device(&dev));
    }
    let savf_name = parms.qual("SAVF");
    let clear = parms.text("CLEAR");
    let savf_id = if media.is_some() {
        None
    } else {
        Some(in_store(system, |txn, qsys, names| {
            save_target(txn, qsys, names, &savf_name, &clear)
        })?)
    };
    let head = header(system, NAME, parms, SaveKind::Security);
    let now = now_unix();
    let (store, qsys) = storage(system)?;
    let hooks = SvcHooks::new(qsys);
    let mut w = Writer::new(Vec::new(), &head).map_err(Fail::ended)?;
    let _ = w.parameters(&parms.to_line(NAME));
    let rep =
        sec::guard::as_system(|| saverst::secdta::save_security(store, &hooks, qsys, now, &mut w))
            .map_err(Fail::ended)?;
    let (_, bytes) = w
        .finish(u32::try_from(rep.not_saved()).unwrap_or(0))
        .map_err(Fail::ended)?;
    let dest = if let Some(m) = &media {
        let w = stream::save(system, m, SECDTA_LABEL, OptTarget::System(NAME), &bytes)?;
        media_dest(&m.dev, &w)
    } else {
        if let Some(savf_id) = savf_id {
            write_savf(system, savf_id, &savf_name, &bytes)?;
        }
        Dest::Savf(savf_name.0.clone(), lib_or(&savf_name.1))
    };
    Ok(save_reply(&[rep], &[dest], now))
}

/// The tape file label of `SAVSECDTA` (IBM's name for the profiles file).
const SECDTA_LABEL: &str = "QFILEUPR";
/// Profiles restored, all of them.
const RESTORED_PROFILES: &str = "CPC3705";
/// Profiles restored, some not.
const RESTORED_PROFILES_PARTIAL: &str = "CPF3775";
/// Every authority of a profile restored.
const AUTH_RESTORED: &str = "CPC3706";
/// Some authorities of a profile left pending.
const AUTH_PARTIAL: &str = "CPF3845";
/// `RSTAUT` restored every authority of every profile.
const AUTH_ALL: &str = "CPC3710";
/// `RSTAUT` left some authorities pending.
const AUTH_NOT_ALL: &str = "CPF3776";
/// A prestart job's name where IBM's messages give one: none here.
const NO_JOB: &str = "*N";
/// A profile with no authorities to restore.
const NO_AUTHORITIES: &str = "CPF3787";

/// `RSTUSRPRF`.
fn rstusrprf(system: &mut System, parms: &Parms) -> Result<CmdReply, Fail> {
    need(SpecialAuth::SAVSYS, "*SAVSYS", "restore user profiles")?;
    let stream = read_savf(system, parms)?;
    let users = values(parms, "USRPRF");
    let now = now_unix();
    let opts = saverst::secdta::ProfileOptions {
        new_only: users.iter().any(|u| u == "*NEW"),
        users: users.into_iter().filter(|u| !u.starts_with('*')).collect(),
        omit: values(parms, "OMITUSRPRF")
            .into_iter()
            .filter(|u| u != "*NONE")
            .collect(),
        now,
    };
    let jobh = job(system);
    let (store, qsys) = storage(system)?;
    let hooks = SvcHooks::new(qsys);
    let mut reader = Reader::new(Cursor::new(stream.as_slice())).map_err(bad_save)?;
    let rep = sec::guard::as_system(|| {
        saverst::secdta::restore_profiles(store, jobh, &hooks, qsys, &mut reader, &opts)
    })
    .map_err(Fail::ended)?;
    // Restored profiles and lists: what was built from them is stale.
    sec::auth::cache::bump_epoch();
    let mut reply = CmdReply::default();
    for (id, data) in rep.objects.iter().filter_map(msg::of_result) {
        reply.messages.push(message(id, DIAG, data));
    }
    // The job calls QIBM_QSY_RST_PROFILE for each profile restored.
    reply.notify = rep
        .objects
        .iter()
        .filter(|o| o.msgid.is_none() && o.type_code == store::obj::types::USRPRF)
        .map(|o| format!("QIBM_QSY_RST_PROFILE RSTP0100 {}", o.name))
        .collect();
    let autls_not = rep
        .objects
        .iter()
        .filter(|o| o.msgid.is_some() && o.type_code == store::obj::types::AUTL)
        .count();
    let data = Data::new()
        .bin4(rep.profiles())
        .bin4(rep.profiles_not())
        .dts(now)
        .bin4(0)
        .bin4(rep.autls())
        .bin4(autls_not)
        .bin4(0)
        .bin4(0)
        .char("", 10)
        .bin4(0)
        .bin4(0)
        .into_bytes();
    if rep.profiles_not() == 0 && autls_not == 0 {
        reply.messages.push(message(RESTORED_PROFILES, COMP, data));
    } else {
        reply.escape = Some(message(RESTORED_PROFILES_PARTIAL, ESC, data));
    }
    Ok(reply)
}

/// `RSTAUT`.
fn rstaut(system: &mut System, parms: &Parms) -> Result<CmdReply, Fail> {
    need(SpecialAuth::SECADM, "*SECADM", "restore authority")?;
    let users: Vec<String> = values(parms, "USRPRF")
        .into_iter()
        .filter(|u| u != "*ALL")
        .collect();
    let now = now_unix();
    let (store, qsys) = storage(system)?;
    let hooks = SvcHooks::new(qsys);
    let done = sec::guard::as_system(|| saverst::secdta::rstaut(store, &hooks, qsys, &users))
        .map_err(Fail::ended)?;
    let mut reply = CmdReply::default();
    // Profiles: all restored, some, none, nothing to restore.
    let (mut all, mut some, mut none, mut nothing) = (0, 0, 0, 0);
    for d in &done {
        match (d.restored, d.not_restored) {
            (0, 0) => nothing += 1,
            (_, 0) => all += 1,
            (0, _) => none += 1,
            _ => some += 1,
        }
        if d.restored == 0 && d.not_restored == 0 {
            let data = Data::new().char(&d.profile, 10).into_bytes();
            reply.messages.push(message(NO_AUTHORITIES, INFO, data));
        } else if d.not_restored == 0 {
            let data = Data::new()
                .char(&d.profile, 10)
                .dts(now)
                .char(NO_JOB, 22)
                .into_bytes();
            reply.messages.push(message(AUTH_RESTORED, COMP, data));
        } else {
            let data = Data::new()
                .bin4(d.restored)
                .char(&d.not_restored.to_string(), 10)
                .dts(now)
                .char(&d.profile, 10)
                .char(NO_JOB, 22)
                .char(NO_JOB, 22)
                .into_bytes();
            reply.messages.push(message(AUTH_PARTIAL, DIAG, data));
        }
    }
    if some + none == 0 {
        let data = Data::new().bin4(all).dts(now).bin4(nothing).into_bytes();
        reply.messages.push(message(AUTH_ALL, COMP, data));
    } else {
        let data = Data::new()
            .bin4(all)
            .bin4(some)
            .dts(now)
            .bin4(none)
            .bin4(nothing)
            .into_bytes();
        reply.escape = Some(message(AUTH_NOT_ALL, ESC, data));
    }
    Ok(reply)
}

/// IFS saves to a save file completed (rule 25).
const SAVED_LINKS: &str = "CPC370D";
/// The same to media.
const SAVED_LINKS_MEDIA: &str = "CPC370C";
/// IFS saves to a save file with links not saved.
const SAVED_LINKS_PARTIAL: &str = "CPF3838";
/// The same to media.
const SAVED_LINKS_PARTIAL_MEDIA: &str = "CPF3837";
/// IFS restores from a save file completed (rule 25).
const RESTORED_LINKS: &str = "CPC370F";
/// The same from media.
const RESTORED_LINKS_MEDIA: &str = "CPC370E";
/// IFS restores from a save file with links not restored.
const RESTORED_LINKS_PARTIAL: &str = "CPF383E";
/// The same from media.
const RESTORED_LINKS_PARTIAL_MEDIA: &str = "CPF3839";
/// A link not saved or not restored.
const LINK_FAILED: &str = "CPD3775";

/// `OBJ((path *INCLUDE|*OMIT [new]) ...)`: (path, include, new name).
fn ifs_objects(parms: &Parms) -> Vec<(String, bool, Option<String>)> {
    let mut out = Vec::new();
    for v in parms.list("OBJ") {
        let part = |e: &[Option<Value>], i: usize| {
            e.get(i)
                .and_then(Option::as_ref)
                .map(|x| x.source().trim().trim_matches('\'').to_string())
        };
        let (path, inc, new) = match v {
            Value::Elems(e) => (
                part(e, 0).unwrap_or_default(),
                part(e, 1).is_none_or(|x| !matches!(x.as_str(), "O" | "*OMIT")),
                part(e, 2).filter(|n| !n.is_empty() && n != "*SAME"),
            ),
            other => (
                other.source().trim().trim_matches('\'').to_string(),
                true,
                None,
            ),
        };
        out.push((if path.is_empty() { ".".into() } else { path }, inc, new));
    }
    if out.is_empty() {
        out.push((".".into(), true, None));
    }
    out
}

/// The entries `names` of directory `dir` that the last part `pattern`
/// of a `SAV` object matches (`*` any characters, case ignored), as
/// paths, without the omitted ones.
fn expand(dir: &str, names: &[String], pattern: &str, omit: &[String]) -> Vec<String> {
    let pat = pattern.to_ascii_uppercase();
    let (head, tail) = pat.split_once('*').unwrap_or((pat.as_str(), ""));
    let tail = tail.trim_end_matches('*');
    let mut out: Vec<String> = names
        .iter()
        .filter(|n| {
            let u = n.to_ascii_uppercase();
            u.len() >= head.len() + tail.len() && u.starts_with(head) && u.ends_with(tail)
        })
        .map(|n| format!("{}/{n}", dir.trim_end_matches('/')))
        .filter(|p| {
            !omit
                .iter()
                .any(|o| path_under(&p.to_ascii_uppercase(), &o.to_ascii_uppercase()))
        })
        .collect();
    out.sort();
    out
}

/// Whether `path` is `pattern` or below it (a trailing `*` matches a
/// prefix of the last part).
fn path_under(path: &str, pattern: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('*') {
        return path.starts_with(prefix);
    }
    path == pattern || path.starts_with(&format!("{}/", pattern.trim_end_matches('/')))
}

/// The save file a `DEV('/QSYS.LIB/LIB.LIB/NAME.FILE')` names.
fn savf_path(dev: &str) -> Option<(String, String)> {
    let comps: Vec<&str> = dev
        .trim_matches('\'')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    match comps.as_slice() {
        [q, lib, file] if q.eq_ignore_ascii_case("QSYS.LIB") => {
            let lib = lib
                .strip_suffix(".LIB")
                .or_else(|| lib.strip_suffix(".lib"))?;
            let file = file
                .strip_suffix(".FILE")
                .or_else(|| file.strip_suffix(".file"))?;
            Some((file.to_ascii_uppercase(), lib.to_ascii_uppercase()))
        }
        _ => None,
    }
}

/// The save stream side of `Ifs::save`: one object per link.
struct LinkSink<'w> {
    w: &'w mut Writer<Vec<u8>>,
    omit: Vec<String>,
    open_data: u64,
    skipping: bool,
    saved: usize,
    failed: Option<savfmt::Error>,
}

impl LinkSink<'_> {
    fn fail(&mut self, e: savfmt::Error) -> ifs::Errno {
        self.failed = Some(e);
        ifs::Errno::EIO
    }
}

impl ifs::save::SaveSink for LinkSink<'_> {
    fn descriptor(&mut self, d: &ifs::save::Descriptor) -> ifs::Result<()> {
        self.skipping = self.omit.iter().any(|o| path_under(&d.path, o));
        if self.skipping {
            return Ok(());
        }
        let meta = d.encode();
        let desc = savfmt::ObjectDescriptor {
            flags: savfmt::object::flags::IFS,
            size_bytes: meta.len() as u64 + d.size,
            part_count: if d.size > 0 { 2 } else { 1 },
            ..savfmt::ObjectDescriptor::default()
        };
        let r = (|| {
            self.w.begin_object(desc, d.path.as_bytes())?;
            self.w.part(savfmt::Part::IFS_META, &meta)?;
            if d.size > 0 {
                self.w.begin_part(savfmt::Part::IFS_DATA, d.size)?;
            } else {
                self.w.end_object()?;
            }
            Ok(())
        })();
        r.map_err(|e| self.fail(e))?;
        self.open_data = d.size;
        self.saved += 1;
        Ok(())
    }

    fn data(&mut self, chunk: &[u8]) -> ifs::Result<()> {
        if self.skipping {
            return Ok(());
        }
        let r = self.w.write_part(chunk).and_then(|()| {
            self.open_data = self.open_data.saturating_sub(chunk.len() as u64);
            if self.open_data == 0 {
                self.w.end_object().map(|_| ())
            } else {
                Ok(())
            }
        });
        r.map_err(|e| self.fail(e))
    }
}

/// The device a `SAV`/`RST` `DEV` path names: `'/QSYS.LIB/TAP01.DEVD'`
/// or the name alone.
fn device_path(dev: &str) -> Option<String> {
    let d = dev.trim().trim_matches('\'');
    let comps: Vec<&str> = d.split('/').filter(|s| !s.is_empty()).collect();
    match comps.as_slice() {
        [q, devd] if q.eq_ignore_ascii_case("QSYS.LIB") => {
            let up = devd.to_ascii_uppercase();
            up.strip_suffix(".DEVD").map(str::to_string)
        }
        _ => stream::is_device(d).then(|| d.to_ascii_uppercase()),
    }
}

/// The label and optical file of `SAV` and `RST` on media.
const IFS_LABEL: &str = "QSAVIFS";

/// `SAV` to a save file or a device.
#[allow(clippy::too_many_lines)]
fn sav(system: &mut System, parms: &Parms) -> Result<CmdReply, Fail> {
    const NAME: &str = "SAV";
    let dev = values(parms, "DEV");
    let media = dev
        .first()
        .and_then(|d| device_path(d))
        .map(|d| stream::of(parms, &d));
    let savf_name = dev.first().and_then(|d| savf_path(d));
    let savf_id = match (&media, &savf_name) {
        (Some(_), _) => None,
        (None, None) => return Err(no_device(&dev)),
        (None, Some(savf_name)) => Some(in_store(system, |txn, qsys, names| {
            let id = find_savf(txn, qsys, names, savf_name)?;
            write_allowed(txn, qsys, id, (&savf_name.0, &lib_or(&savf_name.1)))?;
            Ok(id)
        })?),
    };
    let objs = ifs_objects(parms);
    let head = header(system, NAME, parms, SaveKind::Ifs);
    let mut w = Writer::new(Vec::new(), &head).map_err(Fail::ended)?;
    let _ = w.parameters(&parms.to_line(NAME));
    let omit: Vec<String> = objs.iter().filter(|o| !o.1).map(|o| o.0.clone()).collect();
    let mut sink = LinkSink {
        w: &mut w,
        omit,
        open_data: 0,
        skipping: false,
        saved: 0,
        failed: None,
    };
    let walked = crate::stgsvc::ifs::with_job_ifs(system, |ifs, cx| {
        let mut errs = Vec::new();
        let omitted: Vec<String> = objs.iter().filter(|o| !o.1).map(|o| o.0.clone()).collect();
        for (path, _, _) in objs.iter().filter(|o| o.1) {
            // A pattern in the last part (`'/*'`) names the directory's
            // matching entries.
            let paths = match path.rsplit_once('/').filter(|(_, last)| last.contains('*')) {
                Some((dir, last)) => {
                    let dir = if dir.is_empty() { "/" } else { dir };
                    match ifs.read_dir(cx, dir) {
                        Ok(ents) => {
                            let names: Vec<String> = ents.into_iter().map(|e| e.name).collect();
                            expand(dir, &names, last, &omitted)
                        }
                        Err(e) => {
                            errs.push((path.clone(), e.to_string()));
                            continue;
                        }
                    }
                }
                None => vec![path.clone()],
            };
            for p in paths {
                if let Err(e) = ifs.save(cx, &p, &mut sink) {
                    errs.push((p, e.to_string()));
                }
            }
        }
        errs
    });
    let Some(errs) = walked else {
        return Err(Fail::new(NOT_AVAILABLE, &[]).because("the integrated file system"));
    };
    if let Some(e) = sink.failed.take() {
        return Err(Fail::ended(e));
    }
    let saved = sink.saved;
    let (_, bytes) = w
        .finish(u32::try_from(errs.len()).unwrap_or(0))
        .map_err(Fail::ended)?;
    let now = now_unix();
    let dest = if let Some(m) = &media {
        let w = stream::save(system, m, IFS_LABEL, OptTarget::Ifs(1), &bytes)?;
        media_dest(&m.dev, &w)
    } else {
        let savf_name = savf_name.unwrap_or_default();
        if let Some(savf_id) = savf_id {
            write_savf(system, savf_id, &savf_name, &bytes)?;
        }
        Dest::Savf(dev.first().cloned().unwrap_or_default(), String::new())
    };
    let mut reply = CmdReply::default();
    links_failed(&mut reply, &errs);
    let (done, partial) = match dest {
        Dest::Media { .. } => (SAVED_LINKS_MEDIA, SAVED_LINKS_PARTIAL_MEDIA),
        Dest::Savf(..) => (SAVED_LINKS, SAVED_LINKS_PARTIAL),
    };
    let data = links_end(saved, errs.len(), &dest, now);
    if errs.is_empty() {
        reply.messages.push(message(done, COMP, data));
    } else {
        reply.escape = Some(message(partial, ESC, data));
    }
    Ok(reply)
}

/// `CPD3775` for each link not saved or restored, its cause before it.
fn links_failed(reply: &mut CmdReply, errs: &[(String, String)]) {
    for (path, e) in errs {
        reply
            .messages
            .push(msgout::diagnostic("CPF9897", &[Field::Char(e, 512)]));
        let data = Data::new().vary4(path).into_bytes();
        reply.messages.push(message(LINK_FAILED, DIAG, data));
    }
}

/// The data of `SAV`'s and `RST`'s last message: to or from volumes,
/// or a save file (its path).
fn links_end(ok: usize, not: usize, dest: &Dest, now: u64) -> Vec<u8> {
    let d = Data::new().bin4(ok).bin4(not);
    match dest {
        Dest::Media {
            volumes,
            device,
            last,
        } => d
            .char(volumes, 2475)
            .dts(now)
            .char("", 4)
            .char("", 18)
            .char(last, 32)
            .vary4("")
            .char(device, 10),
        Dest::Savf(path, _) => d.dts(now).vary4(path),
    }
    .into_bytes()
}

/// `RST` from a save file or a device.
#[allow(clippy::too_many_lines)]
fn rst_ifs(system: &mut System, parms: &Parms) -> Result<CmdReply, Fail> {
    let dev = values(parms, "DEV");
    let media = dev
        .first()
        .and_then(|d| device_path(d))
        .map(|d| stream::of(parms, &d));
    let (stream, dest) = match (&media, dev.first().and_then(|d| savf_path(d))) {
        (Some(m), _) => {
            let s = stream::restore(system, m, Some(IFS_LABEL), OptTarget::Ifs(1))?;
            let dest = Dest::Media {
                volumes: m.vols.join(" "),
                device: m.dev.clone(),
                last: m.vols.last().cloned().unwrap_or_default(),
            };
            (s, dest)
        }
        (None, None) => return Err(no_device(&dev)),
        (None, Some(savf_name)) => (
            read_stream(system, &savf_name)?,
            Dest::Savf(dev.first().cloned().unwrap_or_default(), String::new()),
        ),
    };
    let objs = ifs_objects(parms);
    let option = parms.text("OPTION");
    let opts = ifs::save::RestoreOpts {
        new_only: option == "*NEW",
        old_only: option == "*OLD",
        private_auth: parms.text("PVTAUT") == "*YES",
    };
    let mut reader = Reader::new(Cursor::new(stream.as_slice())).map_err(bad_save)?;
    let mut links = Vec::new();
    loop {
        match reader.next_object() {
            Ok(Some(savfmt::Scan::Object(o))) if o.descriptor.has(savfmt::object::flags::IFS) => {
                links.push(*o);
            }
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(e) => return Err(Fail::ended(e)),
        }
    }
    let done = crate::stgsvc::ifs::with_job_ifs(system, |ifs, cx| {
        let mut moved: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let (mut ok, mut not) = (0usize, Vec::new());
        for o in &links {
            let Ok(d) = ifs::save::Descriptor::decode(o.part(savfmt::Part::IFS_META)) else {
                not.push(("*N".to_string(), "the saved link is damaged".to_string()));
                continue;
            };
            let pick = objs
                .iter()
                .filter(|(p, _, _)| path_under(&d.path, p))
                .max_by_key(|(p, _, _)| p.len());
            let Some((from, true, new)) = pick else {
                continue;
            };
            let to = new
                .as_ref()
                .map_or_else(|| d.path.clone(), |n| d.path.replacen(from.as_str(), n, 1));
            let hard = d.hard_link_of.as_ref().and_then(|h| moved.get(h).cloned());
            match ifs.restore(
                cx,
                &d,
                o.part(savfmt::Part::IFS_DATA),
                &to,
                hard.as_deref(),
                opts,
            ) {
                Ok(ifs::save::Restored::Skipped) => {}
                Ok(_) => {
                    ok += 1;
                    moved.insert(d.path.clone(), to);
                }
                Err(e) => not.push((to, e.to_string())),
            }
        }
        (ok, not)
    });
    let Some((ok, not)) = done else {
        return Err(Fail::new(NOT_AVAILABLE, &[]).because("the integrated file system"));
    };
    let mut reply = CmdReply::default();
    links_failed(&mut reply, &not);
    let (done, partial) = match dest {
        Dest::Media { .. } => (RESTORED_LINKS_MEDIA, RESTORED_LINKS_PARTIAL_MEDIA),
        Dest::Savf(..) => (RESTORED_LINKS, RESTORED_LINKS_PARTIAL),
    };
    let data = links_end(ok, not.len(), &dest, now_unix());
    if not.is_empty() {
        // Every link selected came back.
        reply.messages.push(message(done, COMP, data));
    } else {
        reply.escape = Some(message(partial, ESC, data));
    }
    Ok(reply)
}

/// A save's description (name, value) and its rows.
pub type SavfListing = (Vec<(String, String)>, Vec<Vec<String>>);

/// What a save file holds, from its index alone: the save's description
/// and one row per object (object, type, library, attribute, size,
/// text, owner, `YES` or `NO` for its data saved). `DSPSAVF`'s panel
/// and its text form.
pub fn savf_listing(system: &mut System, file: &str, lib: &str) -> Result<SavfListing, String> {
    savf_contents(system, file, lib).map_err(|f| f.text())
}

fn savf_contents(system: &mut System, file: &str, lib: &str) -> Result<SavfListing, Fail> {
    let q = (file.to_ascii_uppercase(), lib.to_ascii_uppercase());
    let stream = savf_bytes(system, &q)?;
    if stream.is_empty() {
        return Ok((vec![("Records".into(), "0".into())], Vec::new()));
    }
    listing(&stream)
}

/// What a save stream holds, from its index alone: [`savf_listing`]'s
/// description and rows, for a save file or a save on media.
pub fn stream_listing(stream: &[u8]) -> Result<SavfListing, String> {
    listing(stream).map_err(|f| f.text())
}

fn listing(stream: &[u8]) -> Result<SavfListing, Fail> {
    let mut cur = Cursor::new(stream);
    let (head, trailer, index) = savfmt::read_index(&mut cur).map_err(bad_save)?;
    let parameters = Reader::new(Cursor::new(stream))
        .ok()
        .and_then(|mut r| {
            let _ = r.next_object();
            r.parameters().map(str::to_string)
        })
        .unwrap_or_default();
    let saved_lib = index
        .iter()
        .find(|e| e.owner_index == savfmt::object::NO_OWNER && e.type_code != types::LIB)
        .map(|e| savfmt::text(&e.library))
        .unwrap_or_default();
    let records = savf::records(stream.len() as u64);
    let desc = vec![
        ("Save command".into(), head.command.clone()),
        ("Library saved".into(), saved_lib),
        ("Objects displayed".into(), trailer.objects.to_string()),
        ("Records".into(), records.to_string()),
        (
            "Release level".into(),
            format!("V{}R{}M{}", head.release.0, head.release.1, head.release.2),
        ),
        (
            "Target release".into(),
            format!("V{}R{}M{}", head.target.0, head.target.1, head.target.2),
        ),
        (
            "Data compressed".into(),
            if head.compression == savfmt::Compression::None {
                "No"
            } else {
                "Yes"
            }
            .into(),
        ),
        ("System serial number".into(), head.serial.clone()),
        ("Save date/time".into(), head.created.to_string()),
        ("Command".into(), parameters),
    ];
    let mut rows = Vec::new();
    for e in index
        .iter()
        .filter(|e| e.owner_index == savfmt::object::NO_OWNER)
    {
        let (d, tail) = savfmt::descriptor_at(&mut cur, e.offset).map_err(bad_save)?;
        let ty = if d.has(savfmt::object::flags::IFS) {
            "*IFS".to_string()
        } else {
            types::type_by_code(d.type_code)
                .map_or_else(|| format!("{:04X}", d.type_code), |t| t.name.to_string())
        };
        let name = if d.has(savfmt::object::flags::IFS) {
            String::from_utf8_lossy(&tail).into_owned()
        } else {
            d.object_name()
        };
        rows.push(vec![
            name,
            ty,
            d.library_name(),
            savfmt::text(&d.attribute),
            d.size_bytes.to_string(),
            savfmt::text(&d.text),
            savfmt::text(&d.owner),
            if d.has(savfmt::object::flags::FREED) {
                "NO"
            } else {
                "YES"
            }
            .to_string(),
        ]);
    }
    Ok((desc, rows))
}

/// `DSPSAVF`'s text form.
fn dspsavf(system: &mut System, parms: &Parms) -> Result<CmdReply, Fail> {
    let (file, lib) = parms.qual("FILE");
    let (desc, rows) = savf_contents(system, &file, &lib_or(&lib))?;
    let mut text = String::new();
    for (k, v) in &desc {
        let _ = writeln!(text, "{k:<24} {v}");
    }
    let _ = writeln!(
        text,
        "{:<10} {:<10} {:<10} {:<10} {:>12}  Text",
        "Object", "Type", "Library", "Attribute", "Size"
    );
    for r in &rows {
        let _ = writeln!(
            text,
            "{:<10} {:<10} {:<10} {:<10} {:>12}  {}",
            r[0], r[1], r[2], r[3], r[4], r[5]
        );
    }
    Ok(CmdReply {
        text,
        ..CmdReply::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_string()).collect()
    }

    #[test]
    fn library_files_after_a_system_save() {
        let tape = s(&[
            "QS400MANIFEST",
            "QS400LICA",
            "QS400LICB",
            "QS400LPQS00SS1",
            "QSAVSECDTA",
            "QSAVCFG",
            "QGPL",
            "QUSRSYS",
            "PAYROLL",
            "QSYS",
            "QSAVIFS",
            "PAYROLL",
            "QSYS2",
            "QFILEIOC",
        ]);
        assert_eq!(
            library_files(&tape, &s(&["*NONSYS"]), &[]),
            s(&["QGPL", "QUSRSYS", "PAYROLL", "QSYS2"])
        );
        assert_eq!(
            library_files(&tape, &s(&["*ALLUSR"]), &s(&["PAY*"])),
            s(&["QGPL", "QUSRSYS", "QSYS2"])
        );
        assert!(library_files(&tape, &s(&["*IBM"]), &[]).is_empty());
        assert_eq!(
            library_files(&s(&["QS400LPQS00WDS", "QDEVTOOLS"]), &s(&["*IBM"]), &[]),
            s(&["QDEVTOOLS"])
        );
    }
}

#[cfg(test)]
mod reply_tests {
    use super::*;

    fn obj(name: &str, msgid: Option<&'static str>) -> saverst::ObjResult {
        saverst::ObjResult {
            library: "TAPLIB2".into(),
            name: name.into(),
            type_code: types::DTAARA,
            msgid,
            ..saverst::ObjResult::default()
        }
    }

    fn shown(m: &CmdMsg) -> String {
        crate::stgsvc::hist::message_text(&m.msgid, &m.data)
    }

    #[test]
    fn an_empty_library_does_not_fail_a_save_of_several() {
        let full = SaveReport {
            library: "TAPLIB2".into(),
            objects: vec![obj("DA1", None)],
            notes: Vec::new(),
        };
        let empty = SaveReport {
            library: "EMPTY".into(),
            objects: Vec::new(),
            notes: Vec::new(),
        };
        let dest = Dest::Savf("S".into(), "QGPL".into());
        let r = save_reply(&[full, empty], &[dest], 0);
        assert!(r.escape.is_none(), "{:?}", r.escape);
        assert!(r.messages.iter().any(|m| m.msgid == msg::NONE_FOR_LIB));
        let failed = SaveReport {
            library: "BAD".into(),
            objects: vec![obj("DA2", Some("CPF3761"))],
            notes: Vec::new(),
        };
        let dest = Dest::Savf("S".into(), "QGPL".into());
        let r = save_reply(&[failed, SaveReport::default()], &[dest], 0);
        assert_eq!(
            r.escape.map(|e| e.msgid).as_deref(),
            Some(msg::SAVED_LIBS_PARTIAL)
        );
    }

    #[test]
    fn rstlib_completes_with_cpc3703_and_its_data() {
        let r = RestoreReport {
            objects: (0..6).map(|i| obj(&format!("DA{i}"), None)).collect(),
            ..RestoreReport::default()
        };
        let none = (String::new(), String::new());
        let reply = rst_reply(&r, ("TAPLIB", "TAPLIB2"), &none, 0, 1_767_225_600);
        assert!(reply.text.is_empty() && reply.escape.is_none());
        let m = reply.messages.last().unwrap();
        assert_eq!((m.msgid.as_str(), m.kind), ("CPC3703", COMP));
        assert_eq!(&m.data[..8], &[0, 0, 0, 6, 0, 0, 0, 0]);
        assert_eq!(ccsid::decode_trimmed(&m.data[8..18]), "TAPLIB");
        assert_eq!(ccsid::decode_trimmed(&m.data[18..28]), "TAPLIB2");
        assert_eq!(
            shown(m),
            "6 objects saved from library TAPLIB were restored to library TAPLIB2."
        );
    }

    #[test]
    fn a_partial_restore_ends_with_cpf3773_after_its_diagnostics() {
        let r = RestoreReport {
            objects: vec![obj("A", None), obj("B", Some(msg::IN_USE))],
            ..RestoreReport::default()
        };
        let savf = ("S71".to_string(), "QGPL".to_string());
        let reply = rst_reply(&r, ("TAPLIB", "TAPLIB2"), &savf, 0, 0);
        let d = &reply.messages[0];
        assert_eq!((d.msgid.as_str(), d.kind), ("CPF3761", DIAG));
        assert_eq!(shown(d), "*DTAARA B in TAPLIB2 is in use by another job.");
        let e = reply.escape.unwrap();
        assert_eq!(e.msgid, "CPF3773");
        assert_eq!(&e.data[..8], &[0, 0, 0, 1, 0, 0, 0, 1]);
    }

    #[test]
    fn a_failure_is_its_escape_with_the_cause_before_it() {
        let r = Fail::new(
            SAVF_EMPTY,
            &[Field::Char("S71", 10), Field::Char("QGPL", 10)],
        )
        .because("checked")
        .reply();
        assert!(r.text.is_empty());
        assert_eq!(r.messages[0].msgid, "CPF9897");
        let e = r.escape.unwrap();
        assert_eq!(e.msgid, "CPF3707");
        assert_eq!(shown(&e), "Save file S71 in QGPL holds no save.");
        let r = Fail::from("CPF9814 device X not found".to_string()).reply();
        assert_eq!(r.escape.unwrap().msgid, "CPF9814");
    }
}

#[cfg(test)]
mod sav_tests {
    use super::*;

    #[test]
    fn a_pattern_names_the_directory_entries() {
        let names: Vec<String> = ["QSYS.LIB", "home", "QOpenSys", "savsys", "rcvdata"]
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        let omit = vec!["/QSYS.LIB".to_string(), "/savsys".to_string()];
        assert_eq!(
            expand("/", &names, "*", &omit),
            vec!["/QOpenSys", "/home", "/rcvdata"]
        );
        assert_eq!(expand("/", &names, "Q*", &omit), vec!["/QOpenSys"]);
        assert_eq!(
            expand("/home", &["a.txt".into(), "b.log".into()], "*.TXT", &[]),
            vec!["/home/a.txt"]
        );
    }
}

#[cfg(test)]
mod auth_tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;

    use blockdev::{BlockDev, MemBlockDev};
    use sec::auth::bits::Auth;
    use sec::auth::cache::Session;
    use sec::auth::grant;
    use sec::auth::principal::Principal;
    use sec::cmd::{Env, Keywords, usrprf};
    use sec::guard::{SecGuard, as_job};
    use sec::profile::shipped;
    use store::{FormatParams, Store, StoreConfig};

    use super::*;

    const NOW: u64 = 1_767_225_600;

    fn sysval(n: &str) -> String {
        if n == "QSECURITY" {
            "40".into()
        } else {
            String::new()
        }
    }

    fn create(t: &mut Txn<'_>, lib: ObjectId, name: &str, type_code: u16) -> ObjectId {
        ops::create(
            t,
            &ops::CreateParams {
                library: lib,
                name: qsname::Name::parse(name).unwrap(),
                type_code,
                attribute: String::new(),
                text: String::new(),
                owner: "QSECOFR".into(),
                system: "QS400".into(),
                now: NOW,
                by_system: true,
            },
        )
        .unwrap()
    }

    /// A guarded store with APPLIB/SF (`*PUBLIC *USE`) and the sessions
    /// of BOB and QSECOFR.
    fn setup() -> (
        Store,
        ObjectId,
        ObjectId,
        Rc<RefCell<Session>>,
        Rc<RefCell<Session>>,
    ) {
        let d = Arc::new(MemBlockDev::with_seed(16384, 7));
        let params = FormatParams {
            asp_number: 1,
            asp_uuid: [4; 16],
            unit_uuids: vec![[5; 16]],
            unit_serials: Vec::new(),
            wal_pages: 128,
            now: NOW,
        };
        let mut store = Store::format(
            vec![Box::new(d) as Box<dyn BlockDev>],
            &params,
            StoreConfig::default(),
        )
        .unwrap();
        let qsys = {
            let mut t = store.begin().unwrap();
            let q = ops::bootstrap(&mut t, NOW, "QS400").unwrap();
            t.commit().unwrap();
            q
        };
        let g = SecGuard::new(Some(qsys));
        g.set_sysval("QCRTAUT", "*USE");
        store.set_guard(Some(g));
        let mut t = store.begin().unwrap();
        let app = create(&mut t, qsys, "APPLIB", types::LIB);
        let sf = savf::create(
            &mut t,
            &ops::CreateParams {
                library: app,
                name: qsname::Name::parse("SF").unwrap(),
                type_code: types::SAVF,
                attribute: String::new(),
                text: "before".into(),
                owner: "QSECOFR".into(),
                system: "QS400".into(),
                now: NOW,
                by_system: true,
            },
            SavfAttr::default(),
        )
        .unwrap();
        savf::write(&mut t, sf, b"stream").unwrap();
        shipped::install(&mut t, qsys, 40, 3, NOW).unwrap();
        let secofr = Principal::load(&mut t, qsys, "QSECOFR", 40).unwrap();
        let env = Env {
            qsys,
            actor: &secofr,
            now: NOW,
            sysval: &sysval,
        };
        usrprf::crtusrprf(&mut t, &env, &Keywords::new(&[("USRPRF", "BOB")])).unwrap();
        grant::set_public(&mut t, app, Some(Auth::USE)).unwrap();
        grant::set_public(&mut t, sf, Some(Auth::USE)).unwrap();
        let bob = Rc::new(RefCell::new(Session::new(
            Principal::load(&mut t, qsys, "BOB", 40).unwrap(),
        )));
        let secofr = Rc::new(RefCell::new(Session::new(secofr)));
        t.commit().unwrap();
        (store, qsys, app, bob, secofr)
    }

    fn change() -> SavfChange {
        SavfChange {
            max: 99,
            wait: -2,
            share: "*SAME".into(),
            text: "after".into(),
        }
    }

    #[test]
    fn chgsavf_needs_objmgt() {
        let (mut store, qsys, app, bob, secofr) = setup();
        let names = ops::NameContext {
            curlib: Some(app),
            libl: vec![qsys, app],
        };
        let file = ("SF".to_string(), "APPLIB".to_string());
        let mut t = store.begin().unwrap();
        let e = as_job(&bob, || {
            change_savf(&mut t, qsys, &names, &file, &change(), NOW)
        })
        .unwrap_err();
        assert!(e.text().starts_with("CPF2189"), "{}", e.text());
        let id = find_savf(&mut t, qsys, &names, &file).unwrap();
        assert_eq!(savf::attr(&mut t, id).unwrap().max_records, 0);
        as_job(&secofr, || {
            change_savf(&mut t, qsys, &names, &file, &change(), NOW)
        })
        .unwrap();
        assert_eq!(savf::attr(&mut t, id).unwrap().max_records, 99);
    }

    #[test]
    fn the_licpgm_list_needs_use() {
        let (mut store, qsys, app, bob, _) = setup();
        let names = ops::NameContext {
            curlib: Some(app),
            libl: vec![qsys, app],
        };
        let mut t = store.begin().unwrap();
        let read =
            |t: &mut Txn<'_>| super::super::lpapi::media_savf(t, qsys, &names, "APPLIB", "SF");
        assert_eq!(as_job(&bob, || read(&mut t)).unwrap(), b"stream");
        let id = find_savf(&mut t, qsys, &names, &("SF".into(), "APPLIB".into())).unwrap();
        grant::set_public(&mut t, id, Some(Auth::EXCLUDE)).unwrap();
        let e = as_job(&bob, || read(&mut t)).unwrap_err();
        assert!(e.starts_with("CPF2189"), "{e}");
    }

    fn auth(words: &[&str]) -> Auth {
        Auth::parse(words, false).unwrap()
    }

    /// APPLIB/EMPTY, a save file with no records (`*PUBLIC *USE`).
    fn empty_savf(t: &mut Txn<'_>, app: ObjectId) -> ObjectId {
        let id = savf::create(
            t,
            &ops::CreateParams {
                library: app,
                name: qsname::Name::parse("EMPTY").unwrap(),
                type_code: types::SAVF,
                attribute: String::new(),
                text: String::new(),
                owner: "QSECOFR".into(),
                system: "QS400".into(),
                now: NOW,
                by_system: true,
            },
            SavfAttr::default(),
        )
        .unwrap();
        grant::set_public(t, id, Some(Auth::USE)).unwrap();
        id
    }

    /// A session for a new profile `name` with special authority `spc`.
    fn user_with(t: &mut Txn<'_>, qsys: ObjectId, name: &str, spc: &str) -> Rc<RefCell<Session>> {
        let secofr = Principal::load(t, qsys, "QSECOFR", 40).unwrap();
        let env = Env {
            qsys,
            actor: &secofr,
            now: NOW,
            sysval: &sysval,
        };
        usrprf::crtusrprf(
            t,
            &env,
            &Keywords::new(&[("USRPRF", name), ("SPCAUT", spc)]),
        )
        .unwrap();
        Rc::new(RefCell::new(Session::new(
            Principal::load(t, qsys, name, 40).unwrap(),
        )))
    }

    #[test]
    fn savsys_does_not_cover_the_save_file() {
        let (mut store, qsys, app, _, _) = setup();
        let names = ops::NameContext {
            curlib: Some(app),
            libl: vec![qsys, app],
        };
        let mut t = store.begin().unwrap();
        empty_savf(&mut t, app);
        let saver = user_with(&mut t, qsys, "SAVER", "*SAVSYS");
        let empty = ("EMPTY".to_string(), "APPLIB".to_string());
        let full = ("SF".to_string(), "APPLIB".to_string());
        // *PUBLIC *USE: no *ADD to save into, *USE to restore from.
        let e = as_job(&saver, || {
            save_target(&mut t, qsys, &names, &empty, "*NONE")
        })
        .unwrap_err();
        assert!(e.text().starts_with("CPF2189"), "{}", e.text());
        let sf = find_savf(&mut t, qsys, &names, &full).unwrap();
        as_job(&saver, || may_read_savf(&mut t, qsys, sf, "SF")).unwrap();
        grant::set_public(&mut t, sf, Some(Auth::EXCLUDE)).unwrap();
        let e = as_job(&saver, || may_read_savf(&mut t, qsys, sf, "SF")).unwrap_err();
        assert!(e.starts_with("CPF2189"), "{e}");
    }

    #[test]
    fn use_and_add_save_into_an_empty_save_file() {
        let (mut store, qsys, app, bob, _) = setup();
        let names = ops::NameContext {
            curlib: Some(app),
            libl: vec![qsys, app],
        };
        let mut t = store.begin().unwrap();
        let id = empty_savf(&mut t, app);
        let empty = ("EMPTY".to_string(), "APPLIB".to_string());
        grant::set_private(&mut t, qsys, id, "BOB", Some(auth(&["*USE", "*ADD"]))).unwrap();
        assert_eq!(
            as_job(&bob, || save_target(&mut t, qsys, &names, &empty, "*NONE")).unwrap(),
            id
        );
    }

    #[test]
    fn a_save_file_with_records_needs_objmgt() {
        let (mut store, qsys, app, bob, _) = setup();
        let names = ops::NameContext {
            curlib: Some(app),
            libl: vec![qsys, app],
        };
        let full = ("SF".to_string(), "APPLIB".to_string());
        let mut t = store.begin().unwrap();
        let sf = find_savf(&mut t, qsys, &names, &full).unwrap();
        grant::set_private(&mut t, qsys, sf, "BOB", Some(auth(&["*USE", "*ADD"]))).unwrap();
        let e = as_job(&bob, || save_target(&mut t, qsys, &names, &full, "*ALL")).unwrap_err();
        assert!(e.text().starts_with("CPF2189"), "{}", e.text());
        grant::set_private(
            &mut t,
            qsys,
            sf,
            "BOB",
            Some(auth(&["*USE", "*ADD", "*OBJMGT"])),
        )
        .unwrap();
        assert_eq!(
            as_job(&bob, || save_target(&mut t, qsys, &names, &full, "*ALL")).unwrap(),
            sf
        );
        // Allowed, but still not cleared without CLEAR.
        let e = as_job(&bob, || save_target(&mut t, qsys, &names, &full, "*NONE")).unwrap_err();
        assert!(e.text().starts_with(SAVF_NOT_EMPTY), "{}", e.text());
    }

    #[test]
    fn allobj_passes() {
        let (mut store, qsys, app, _, secofr) = setup();
        let names = ops::NameContext {
            curlib: Some(app),
            libl: vec![qsys, app],
        };
        let full = ("SF".to_string(), "APPLIB".to_string());
        let mut t = store.begin().unwrap();
        let sf = find_savf(&mut t, qsys, &names, &full).unwrap();
        grant::set_public(&mut t, sf, Some(Auth::EXCLUDE)).unwrap();
        let all = user_with(&mut t, qsys, "ALLER", "*ALLOBJ");
        for s in [&secofr, &all] {
            assert_eq!(
                as_job(s, || save_target(&mut t, qsys, &names, &full, "*REPLACE")).unwrap(),
                sf
            );
            as_job(s, || may_read_savf(&mut t, qsys, sf, "SF")).unwrap();
        }
    }
}
