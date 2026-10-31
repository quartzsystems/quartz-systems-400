// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Save and restore commands on save files: `CRTSAVF`, `CHGSAVF`,
//! `CLRSAVF`, `SAVLIB`, `SAVOBJ`, `RSTLIB`, `RSTOBJ`. The engine is
//! `save/saverst`; this is parameters, the save file, authority and the
//! messages.

use std::fmt::Write as _;
use std::io::Cursor;

use cmd::Value;
use cmd::block::Parms;
use saverst::{
    AllowDiff, AllowRestore, Hooks, MbrOpt, ObjSel, RestoreOptions, RestoreReport, RstOption, SaveOptions,
    SaveReport, SaveTarget, msg,
};
use savfmt::{Compression, MediaHeader, Reader, SaveKind, Writer};
use sec::profile::SpecialAuth;
use stg::proto::{CmdMsg, CmdReply};
use store::obj::header::ObjectHeader;
use store::obj::savf::{self, SavfAttr};
use store::obj::{JobHandle, ObjectId, ops, types};
use store::txn::Txn;

use super::direct::{ctx, failed, in_store, lib_or, library};
use crate::ipl::system::{System, now_unix};
use crate::stgsvc::security::actor;

/// Run `name`, or `None` when it is not one of these.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    let r = match name {
        "CRTSAVF" => crtsavf(system, parms),
        "CHGSAVF" => chgsavf(system, parms),
        "CLRSAVF" => clrsavf(system, parms),
        "SAVLIB" | "SAVOBJ" => return Some(save(system, name, parms)),
        "RSTLIB" | "RSTOBJ" => return Some(rst(system, name, parms)),
        "SAVSECDTA" => return Some(savsecdta(system, parms)),
        "RSTUSRPRF" => return Some(rstusrprf(system, parms)),
        "RSTAUT" => return Some(rstaut(system, parms)),
        _ => return None,
    };
    Some(match r {
        Ok(reply) => reply,
        Err(e) => failed(name, &e),
    })
}

/// Save file not empty and `CLEAR(*NONE)`.
const SAVF_NOT_EMPTY: &str = "CPA4067";
/// The save needs more records than the save file allows.
const SAVF_FULL: &str = "CPF3797";
/// A device other than `*SAVF` before the media layer is wired.
const NO_DEVICE: &str = "CPF9814";
/// Save active and other functions this release does not have.
const NOT_AVAILABLE: &str = "CPF3700";
/// A target release this release cannot write.
const BAD_TGTRLS: &str = "CPF3743";
/// Save file not found.
const NO_SAVF: &str = "CPF9812";

fn message(id: &str, kind: u8, fields: &[(&str, usize)]) -> CmdMsg {
    CmdMsg {
        msgid: id.into(),
        kind,
        data: cmd::error::data(fields),
    }
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
struct SvcHooks {
    qsys: ObjectId,
}

impl Hooks for SvcHooks {
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
        if ok { Ok(()) } else { Err(msg::NOT_RESTORED_AUTH) }
    }

    fn set_private(
        &self,
        txn: &mut Txn<'_>,
        id: ObjectId,
        profile: [u8; 10],
        bits: u32,
    ) -> store::Result<()> {
        let name = store::obj::header::text_of(&profile);
        sec::auth::grant::set_private(txn, self.qsys, id, &name, Some(sec::auth::bits::Auth::of(store::obj::Authority(bits))))
            .map_err(|e| store::StoreError::BadValue { what: e.to_string() })
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
        trusted_keys().iter().any(|k| saverst::sign::key_id(k) == key_id)
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
fn trusted_keys() -> Vec<[u8; 32]> {
    std::fs::read(TRUSTED_KEYS)
        .map(|b| {
            b.as_chunks::<32>().0.to_vec()
        })
        .unwrap_or_default()
}

/// The save file a `FILE`/`SAVF` parameter names.
fn find_savf(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    names: &ops::NameContext,
    (name, lib): &(String, String),
) -> Result<ObjectId, String> {
    wm::obj::find(txn, qsys, names, &lib_or(lib), name, types::SAVF)
        .map_err(|_| format!("{NO_SAVF} save file {}/{name} not found", lib_or(lib)))
}

fn crtsavf(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    let (name, lib) = parms.qual("FILE");
    let text = parms.text("TEXT");
    let attr = SavfAttr {
        max_records: u32::try_from(parms.int("MAXRCDS").unwrap_or(0)).unwrap_or(0),
        wait_file: i32::try_from(parms.int("WAITFILE").unwrap_or(0)).unwrap_or(0),
        share: parms.text("SHARE") == "*YES",
    };
    let c = ctx(system);
    let owner = user(system);
    in_store(system, |txn, qsys, names| {
        let l = library(
            txn,
            qsys,
            names,
            if lib.is_empty() { "*CURLIB" } else { &lib },
        )?;
        let n = qsname::Name::parse(&name).map_err(|e| format!("CPD0074 {e}"))?;
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
        .map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPF7302")))?;
        Ok(())
    })?;
    Ok(CmdReply::default())
}

fn chgsavf(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    let file = parms.qual("FILE");
    let max = parms.int("MAXRCDS").unwrap_or(-1);
    let wait = parms.int("WAITFILE").unwrap_or(-2);
    let share = parms.text("SHARE");
    let text = parms.text("TEXT");
    let now = now_unix();
    in_store(system, |txn, qsys, names| {
        let id = find_savf(txn, qsys, names, &file)?;
        let mut a = savf::attr(txn, id).map_err(|e| e.to_string())?;
        if max >= 0 {
            a.max_records = u32::try_from(max).unwrap_or(u32::MAX);
        }
        if wait != -2 {
            a.wait_file = i32::try_from(wait).unwrap_or(0);
        }
        if share != "*SAME" {
            a.share = share == "*YES";
        }
        savf::set_attr(txn, id, a).map_err(|e| e.to_string())?;
        if text != "*SAME" {
            ops::change_text(txn, id, &text, now).map_err(|e| e.to_string())?;
        }
        Ok(())
    })?;
    Ok(CmdReply::default())
}

fn clrsavf(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    let file = parms.qual("FILE");
    in_store(system, |txn, qsys, names| {
        let id = find_savf(txn, qsys, names, &file)?;
        if !savsys() && !sec::guard::allowed(txn, qsys, "savf.clear", id) {
            return Err(format!("CPF2189 not authorized to save file {}", file.0));
        }
        savf::clear(txn, id).map_err(|e| e.to_string())
    })?;
    Ok(CmdReply::default())
}

/// Text values of a list parameter, uppercase.
fn values(parms: &Parms, kwd: &str) -> Vec<String> {
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

/// The libraries `*NONSYS`, `*ALLUSR` and `*IBM` mean.
fn expand_libs(system: &mut System, libs: &[String]) -> Result<Vec<String>, String> {
    let special = |l: &str| matches!(l, "*NONSYS" | "*ALLUSR" | "*IBM");
    if !libs.iter().any(|l| special(l)) {
        return Ok(libs.to_vec());
    }
    let all: Vec<String> = in_store(system, |txn, qsys, _| {
        Ok(ops::libraries(txn, qsys)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|(n, _)| n.to_string())
            .collect())
    })?;
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
    let now = now_unix();
    let mut id = [0u8; 16];
    id[..8].copy_from_slice(&now.to_be_bytes());
    id[8..12].copy_from_slice(&system.current_job.to_be_bytes());
    id[12..].copy_from_slice(&std::process::id().to_be_bytes());
    let r = release::RELEASE;
    MediaHeader {
        release: (r.v, r.r, r.m),
        target: (r.v, r.r, r.m),
        compression: compression(parms),
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

#[allow(clippy::too_many_lines)]
fn save(system: &mut System, name: &str, parms: &Parms) -> CmdReply {
    let dev = values(parms, "DEV");
    if dev.first().map(String::as_str) != Some("*SAVF") {
        return failed(
            name,
            &format!("{NO_DEVICE} device {} not found", dev.join(" ")),
        );
    }
    let savact = parms.text("SAVACT");
    if !savact.is_empty() && savact != "*NO" {
        return failed(
            name,
            &format!("{NOT_AVAILABLE} SAVACT({savact}) is not available"),
        );
    }
    let tgtrls = parms.text("TGTRLS");
    if !tgtrls_ok(&tgtrls) {
        return failed(
            name,
            &format!("{BAD_TGTRLS} target release {tgtrls} is not valid"),
        );
    }
    let savf_name = parms.qual("SAVF");
    let libs = values(parms, "LIB");
    let libs = match expand_libs(system, &libs) {
        Ok(l) => l,
        Err(e) => return failed(name, &e),
    };
    let omit_libs = values(parms, "OMITLIB");
    let omit = objsels(parms, "OMITOBJ");
    let select: Vec<ObjSel> = if name == "SAVOBJ" {
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
        update_history: parms.text("UPDHST") != "*NO",
        save_file_data: parms.text("SAVFDTA") == "*YES",
        private_auth: parms.text("PVTAUT") == "*YES",
        changed_since: None,
        now,
        target: SaveTarget {
            command: name.into(),
            device: "*SAVF".into(),
            file: Some((savf_name.0.clone(), lib_or(&savf_name.1))),
            ..SaveTarget::default()
        },
    };
    // The save file: found, allowed, and empty unless CLEAR says.
    let savf_id = match in_store(system, |txn, qsys, names| {
        let id = find_savf(txn, qsys, names, &savf_name)?;
        if !savsys() && !sec::guard::allowed(txn, qsys, "savf.write", id) {
            return Err(format!(
                "CPF2189 not authorized to save file {}",
                savf_name.0
            ));
        }
        let used = savf::len(txn, id).map_err(|e| e.to_string())?;
        if used > 0 && !matches!(clear.as_str(), "*ALL" | "*REPLACE" | "*AFTER") {
            return Err(format!(
                "{SAVF_NOT_EMPTY} save file {} already holds data",
                savf_name.0
            ));
        }
        Ok(id)
    }) {
        Ok(id) => id,
        Err(e) => return failed(name, &e),
    };
    let kind = if name == "SAVLIB" {
        SaveKind::Library
    } else {
        SaveKind::Objects
    };
    let head = header(system, name, parms, kind);
    let jobh = job(system);
    let Some(storage) = system.storage.as_mut() else {
        return failed(name, "CPF3700 object store not open");
    };
    let Some(qsys) = storage.qsys else {
        return failed(name, "CPF3700 object store not initialized");
    };
    let hooks = SvcHooks { qsys };
    let mut w = match Writer::new(Vec::new(), &head) {
        Ok(w) => w,
        Err(e) => return failed(name, &format!("CPF3793 {e}")),
    };
    let _ = w.parameters(&parms.to_line(name));
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
            ..base.clone()
        };
        match saverst::save_library(&mut storage.store, jobh, &hooks, qsys, lib, &opts, &mut w) {
            Ok(r) => reports.push(r),
            Err(saverst::Error::Refused { msgid, text }) => {
                reports.push(SaveReport {
                    library: lib.clone(),
                    objects: Vec::new(),
                });
                refused.push(message(msgid, 5, &[(&text, 10), (lib, 10)]));
            }
            Err(e) => return failed(name, &format!("CPF3793 {e}")),
        }
    }
    let not_saved: usize = reports.iter().map(SaveReport::not_saved).sum();
    let bytes = match w.finish(u32::try_from(not_saved).unwrap_or(u32::MAX)) {
        Ok((_, b)) => b,
        Err(e) => return failed(name, &format!("CPF3793 {e}")),
    };
    let wrote = in_store(system, |txn, _, _| {
        savf::write(txn, savf_id, &bytes).map_err(|e| match e {
            store::StoreError::BadValue { .. } => format!("{SAVF_FULL} {e}"),
            e => e.to_string(),
        })
    });
    if let Err(e) = wrote {
        return failed(name, &e);
    }
    let mut reply = save_reply(&reports);
    reply.messages.splice(0..0, refused);
    reply
}

fn save_reply(reports: &[SaveReport]) -> CmdReply {
    let mut reply = CmdReply::default();
    let mut text = String::new();
    for r in reports {
        for o in r.objects.iter().filter(|o| o.msgid.is_some()) {
            let id = o.msgid.unwrap_or(msg::NOT_SAVED_IN_USE);
            let _ = writeln!(text, "{id} {} in {} not saved", o.name, o.library);
            reply
                .messages
                .push(message(id, 5, &[(&o.name, 10), (&o.library, 10)]));
        }
        let n = r.saved().to_string();
        reply.messages.push(message(
            r.completion(),
            if r.not_saved() == 0 && r.saved() > 0 {
                4
            } else {
                5
            },
            &[(&n, 10), (&r.library, 10), (&r.not_saved().to_string(), 10)],
        ));
        let _ = writeln!(
            text,
            "{} {} objects saved from library {}",
            r.completion(),
            r.saved(),
            r.library
        );
    }
    let saved: usize = reports.iter().map(SaveReport::saved).sum();
    let not: usize = reports.iter().map(SaveReport::not_saved).sum();
    if not > 0 || saved == 0 {
        let id = match (reports.len(), saved) {
            (_, 0) => msg::NONE_FOR_LIB,
            (1, _) => msg::SAVED_LIB_PARTIAL,
            _ => msg::SAVED_LIBS_PARTIAL,
        };
        reply.escape = Some(message(
            id,
            7,
            &[
                (&saved.to_string(), 10),
                (&reports.len().to_string(), 10),
                (&not.to_string(), 10),
            ],
        ));
    }
    reply.text = text;
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

fn rst(system: &mut System, name: &str, parms: &Parms) -> CmdReply {
    let dev = values(parms, "DEV");
    if dev.first().map(String::as_str) != Some("*SAVF") {
        return failed(
            name,
            &format!("{NO_DEVICE} device {} not found", dev.join(" ")),
        );
    }
    let savf_name = parms.qual("SAVF");
    let saved = values(parms, "SAVLIB");
    let rstlib = parms.text("RSTLIB");
    let target = (!rstlib.is_empty() && rstlib != "*SAVLIB").then(|| rstlib.to_ascii_uppercase());
    let select: Vec<ObjSel> = if name == "RSTOBJ" {
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
        allow_restore: {
            let v = crate::stgsvc::security::sysval_text(system, "QALWOBJRST");
            AllowRestore::from_value(if v.is_empty() { "*ALL" } else { &v })
        },
    };
    let stream = match in_store(system, |txn, qsys, names| {
        let id = find_savf(txn, qsys, names, &savf_name)?;
        if !savsys() && !sec::guard::allowed(txn, qsys, "savf.read", id) {
            return Err(format!(
                "CPF2189 not authorized to save file {}",
                savf_name.0
            ));
        }
        savf::read(txn, id).map_err(|e| e.to_string())
    }) {
        Ok(s) => s,
        Err(e) => return failed(name, &e),
    };
    if stream.is_empty() {
        return failed(
            name,
            &format!("{} save file {} is empty", msg::NONE_FOR_LIB, savf_name.0),
        );
    }
    let jobh = job(system);
    let Some(storage) = system.storage.as_mut() else {
        return failed(name, "CPF3700 object store not open");
    };
    let Some(qsys) = storage.qsys else {
        return failed(name, "CPF3700 object store not initialized");
    };
    let hooks = SvcHooks { qsys };
    let mut reader = match Reader::new(Cursor::new(stream.as_slice())) {
        Ok(r) => r,
        Err(e) => return failed(name, &format!("CPF3743 {e}")),
    };
    let r = reader.header().target;
    let cur = release::RELEASE;
    if (r.0, r.1, r.2) > (cur.v, cur.r, cur.m) {
        return failed(name, "CPF3743 the save is for a later release");
    }
    // The hooks decide who may restore; the objects are made by the
    // system so they keep their saved owners.
    let store = &mut storage.store;
    let res = sec::guard::as_system(|| {
        saverst::restore(store, jobh, &hooks, qsys, &mut reader, &opts)
    });
    match res {
        Ok(r) => rst_reply(&r, opts.target_library.as_deref()),
        Err(e) => failed(name, &format!("CPF3793 {e}")),
    }
}

fn rst_reply(r: &RestoreReport, target: Option<&str>) -> CmdReply {
    let mut reply = CmdReply::default();
    let mut text = String::new();
    for (id, t) in &r.notes {
        reply.messages.push(message(id, 5, &[(t, 50)]));
        let _ = writeln!(text, "{id} {t}");
    }
    for o in r.objects.iter().filter(|o| o.msgid.is_some()) {
        let id = o.msgid.unwrap_or(msg::NOT_RESTORED_DAMAGED);
        reply
            .messages
            .push(message(id, 5, &[(&o.name, 10), (&o.library, 10)]));
        let _ = writeln!(text, "{id} {} in {} not restored", o.name, o.library);
    }
    let lib = target
        .map(str::to_string)
        .or_else(|| r.objects.first().map(|o| o.library.clone()))
        .unwrap_or_default();
    let n = r.restored().to_string();
    let not = r.not_restored().to_string();
    let id = r.completion();
    let m = message(
        id,
        if id == msg::RESTORED_LIB { 4 } else { 7 },
        &[(&n, 10), (&lib, 10), (&not, 10)],
    );
    let _ = writeln!(
        text,
        "{id} {n} objects restored to {lib}, {not} not restored"
    );
    if id == msg::RESTORED_LIB {
        reply.messages.push(m);
    } else {
        reply.escape = Some(m);
    }
    reply.text = text;
    reply
}

/// The save file of a security or configuration command, read.
fn read_savf(system: &mut System, name: &str, parms: &Parms) -> Result<Vec<u8>, Box<CmdReply>> {
    let dev = values(parms, "DEV");
    if dev.first().map(String::as_str) != Some("*SAVF") {
        return Err(Box::new(failed(name, &format!("{NO_DEVICE} device {} not found", dev.join(" ")))));
    }
    let savf_name = parms.qual("SAVF");
    let stream = in_store(system, |txn, qsys, names| {
        let id = find_savf(txn, qsys, names, &savf_name)?;
        if !savsys() && !sec::guard::allowed(txn, qsys, "savf.read", id) {
            return Err(format!("CPF2189 not authorized to save file {}", savf_name.0));
        }
        savf::read(txn, id).map_err(|e| e.to_string())
    })
    .map_err(|e| Box::new(failed(name, &e)))?;
    if stream.is_empty() {
        return Err(Box::new(failed(
            name,
            &format!("{} save file {} is empty", msg::NONE_FOR_LIB, savf_name.0),
        )));
    }
    Ok(stream)
}

/// `SAVSECDTA`: profiles and authorization lists to a save file.
fn savsecdta(system: &mut System, parms: &Parms) -> CmdReply {
    const NAME: &str = "SAVSECDTA";
    if let Err(e) = crate::stgsvc::security::require(SpecialAuth::SAVSYS, "save security data") {
        return failed(NAME, &format!("CPF2189 {e}"));
    }
    let dev = values(parms, "DEV");
    if dev.first().map(String::as_str) != Some("*SAVF") {
        return failed(NAME, &format!("{NO_DEVICE} device {} not found", dev.join(" ")));
    }
    let savf_name = parms.qual("SAVF");
    let clear = parms.text("CLEAR");
    let savf_id = match in_store(system, |txn, qsys, names| {
        let id = find_savf(txn, qsys, names, &savf_name)?;
        let used = savf::len(txn, id).map_err(|e| e.to_string())?;
        if used > 0 && !matches!(clear.as_str(), "*ALL" | "*REPLACE" | "*AFTER") {
            return Err(format!("{SAVF_NOT_EMPTY} save file {} already holds data", savf_name.0));
        }
        Ok(id)
    }) {
        Ok(id) => id,
        Err(e) => return failed(NAME, &e),
    };
    let head = header(system, NAME, parms, SaveKind::Security);
    let Some(storage) = system.storage.as_mut() else {
        return failed(NAME, "CPF3700 object store not open");
    };
    let Some(qsys) = storage.qsys else {
        return failed(NAME, "CPF3700 object store not initialized");
    };
    let hooks = SvcHooks { qsys };
    let mut w = match Writer::new(Vec::new(), &head) {
        Ok(w) => w,
        Err(e) => return failed(NAME, &format!("CPF3793 {e}")),
    };
    let _ = w.parameters(&parms.to_line(NAME));
    let store = &mut storage.store;
    let rep = match sec::guard::as_system(|| {
        saverst::secdta::save_security(store, &hooks, qsys, now_unix(), &mut w)
    }) {
        Ok(r) => r,
        Err(e) => return failed(NAME, &format!("CPF3793 {e}")),
    };
    let bytes = match w.finish(u32::try_from(rep.not_saved()).unwrap_or(0)) {
        Ok((_, b)) => b,
        Err(e) => return failed(NAME, &format!("CPF3793 {e}")),
    };
    if let Err(e) = in_store(system, |txn, _, _| {
        savf::write(txn, savf_id, &bytes).map_err(|e| format!("{SAVF_FULL} {e}"))
    }) {
        return failed(NAME, &e);
    }
    let n = rep.saved().to_string();
    CmdReply {
        text: format!("{SAVED_SECDTA} {n} profiles and authorization lists saved\n"),
        messages: vec![message(SAVED_SECDTA, 4, &[(&n, 10)])],
        ..CmdReply::default()
    }
}

/// `SAVSECDTA` completed.
const SAVED_SECDTA: &str = "CPC3701";
/// Profiles restored, all of them.
const RESTORED_PROFILES: &str = "CPC3705";
/// Profiles restored, some not.
const RESTORED_PROFILES_PARTIAL: &str = "CPF3775";
/// Authorities granted for a profile.
const AUTH_RESTORED: &str = "CPC3706";
/// Authorities left pending for a profile.
const AUTH_NOT_RESTORED: &str = "CPF3736";

/// `RSTUSRPRF`.
fn rstusrprf(system: &mut System, parms: &Parms) -> CmdReply {
    const NAME: &str = "RSTUSRPRF";
    if let Err(e) = crate::stgsvc::security::require(SpecialAuth::SAVSYS, "restore user profiles") {
        return failed(NAME, &format!("CPF2189 {e}"));
    }
    let stream = match read_savf(system, NAME, parms) {
        Ok(s) => s,
        Err(r) => return *r,
    };
    let users = values(parms, "USRPRF");
    let opts = saverst::secdta::ProfileOptions {
        new_only: users.iter().any(|u| u == "*NEW"),
        users: users.into_iter().filter(|u| !u.starts_with('*')).collect(),
        omit: values(parms, "OMITUSRPRF")
            .into_iter()
            .filter(|u| u != "*NONE")
            .collect(),
        now: now_unix(),
    };
    let jobh = job(system);
    let Some(storage) = system.storage.as_mut() else {
        return failed(NAME, "CPF3700 object store not open");
    };
    let Some(qsys) = storage.qsys else {
        return failed(NAME, "CPF3700 object store not initialized");
    };
    let hooks = SvcHooks { qsys };
    let mut reader = match Reader::new(Cursor::new(stream.as_slice())) {
        Ok(r) => r,
        Err(e) => return failed(NAME, &format!("CPF3743 {e}")),
    };
    let store = &mut storage.store;
    let rep = match sec::guard::as_system(|| {
        saverst::secdta::restore_profiles(store, jobh, &hooks, qsys, &mut reader, &opts)
    }) {
        Ok(r) => r,
        Err(e) => return failed(NAME, &format!("CPF3793 {e}")),
    };
    let mut reply = CmdReply::default();
    for o in rep.objects.iter().filter(|o| o.msgid.is_some()) {
        let id = o.msgid.unwrap_or(msg::NOT_RESTORED_DAMAGED);
        reply.messages.push(message(id, 5, &[(&o.name, 10), (&o.library, 10)]));
        let _ = writeln!(reply.text, "{id} {} not restored", o.name);
    }
    let n = rep.profiles().to_string();
    let not = rep.profiles_not().to_string();
    let _ = writeln!(
        reply.text,
        "{n} user profiles restored, {not} not; {} authorization lists restored",
        rep.autls()
    );
    let m = |id| message(id, 4, &[(&n, 10), (&not, 10)]);
    if rep.profiles_not() == 0 {
        reply.messages.push(m(RESTORED_PROFILES));
    } else {
        reply.escape = Some(CmdMsg {
            kind: 7,
            ..m(RESTORED_PROFILES_PARTIAL)
        });
    }
    reply
}

/// `RSTAUT`.
fn rstaut(system: &mut System, parms: &Parms) -> CmdReply {
    const NAME: &str = "RSTAUT";
    if let Err(e) = crate::stgsvc::security::require(SpecialAuth::SECADM, "restore authority") {
        return failed(NAME, &format!("CPF2189 {e}"));
    }
    let users: Vec<String> = values(parms, "USRPRF")
        .into_iter()
        .filter(|u| u != "*ALL")
        .collect();
    let Some(storage) = system.storage.as_mut() else {
        return failed(NAME, "CPF3700 object store not open");
    };
    let Some(qsys) = storage.qsys else {
        return failed(NAME, "CPF3700 object store not initialized");
    };
    let hooks = SvcHooks { qsys };
    let store = &mut storage.store;
    let done = match sec::guard::as_system(|| saverst::secdta::rstaut(store, &hooks, qsys, &users)) {
        Ok(d) => d,
        Err(e) => return failed(NAME, &format!("CPF3793 {e}")),
    };
    let mut reply = CmdReply::default();
    let mut partial = false;
    for d in &done {
        let (r, n) = (d.restored.to_string(), d.not_restored.to_string());
        let _ = writeln!(
            reply.text,
            "{} authorities restored for {}, {} not",
            d.restored, d.profile, d.not_restored
        );
        if d.not_restored == 0 {
            reply.messages.push(message(AUTH_RESTORED, 4, &[(&r, 10), (&d.profile, 10)]));
        } else {
            partial = true;
            reply
                .messages
                .push(message(AUTH_NOT_RESTORED, 5, &[(&r, 10), (&d.profile, 10), (&n, 10)]));
        }
    }
    if partial {
        reply.escape = Some(message(AUTH_NOT_RESTORED, 7, &[]));
    }
    reply
}
