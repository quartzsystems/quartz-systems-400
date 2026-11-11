// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*MSGF` objects in the store: `CRTMSGF`, `ADDMSGD`, `CHGMSGD`,
//! `RMVMSGD`, `DSPMSGD`, `MRGMSGF`, `RTVMSG`, and installing the system
//! files into `QSYS` on first IPL.

use std::fmt::Write as _;

use qsname::{LibSpec, Name};
use store::Txn;
use store::obj::library::Library;
use store::obj::space::Space;
use store::obj::{CreateParams, NameContext, ObjectId, ops, types};

use crate::desc::MsgDesc;
use crate::error::Result;
use crate::payload;
use crate::source::{MsgFile, builtin, product_files};

/// Who creates and when.
#[derive(Clone, Debug)]
pub struct Ctx {
    /// Unix seconds.
    pub now: u64,
    /// Owner profile.
    pub owner: String,
    /// System name.
    pub system: String,
}

fn write_payload(txn: &mut Txn<'_>, id: ObjectId, f: &MsgFile) -> Result<()> {
    let bytes = payload::encode(f);
    let space = Space::ensure(txn, id, u64::try_from(bytes.len()).expect("fits"), 0)?;
    space.write(txn, 0, &bytes)?;
    Ok(())
}

/// Read a `*MSGF`.
pub fn load(txn: &mut Txn<'_>, id: ObjectId) -> Result<MsgFile> {
    let h = ops::describe(txn, id)?;
    let (space, _) = Space::load(txn, id)?;
    let len = usize::try_from(space.bytes()).expect("fits");
    let bytes = space.read(txn, 0, len)?;
    payload::decode(&bytes, &h.name.to_string(), &h.text_string())
}

/// `CRTMSGF`: an empty message file.
pub fn crtmsgf(
    txn: &mut Txn<'_>,
    ctx: &Ctx,
    library: ObjectId,
    name: Name,
    text: &str,
    by_system: bool,
) -> Result<ObjectId> {
    let id = ops::create(
        txn,
        &CreateParams {
            library,
            name,
            type_code: types::MSGF,
            attribute: String::new(),
            text: text.into(),
            owner: ctx.owner.clone(),
            system: ctx.system.clone(),
            now: ctx.now,
            by_system,
        },
    )?;
    write_payload(
        txn,
        id,
        &MsgFile {
            ccsid: 37,
            name: name.to_string(),
            text: text.into(),
            msgs: Vec::new(),
        },
    )?;
    Ok(id)
}

/// `ADDMSGD`.
pub fn addmsgd(txn: &mut Txn<'_>, id: ObjectId, d: MsgDesc) -> Result<()> {
    let mut f = load(txn, id)?;
    f.add(d)?;
    write_payload(txn, id, &f)
}

/// `CHGMSGD`.
pub fn chgmsgd(txn: &mut Txn<'_>, id: ObjectId, d: MsgDesc) -> Result<()> {
    let mut f = load(txn, id)?;
    f.change(d)?;
    write_payload(txn, id, &f)
}

/// `RMVMSGD`.
pub fn rmvmsgd(txn: &mut Txn<'_>, id: ObjectId, msgid: &str) -> Result<MsgDesc> {
    let mut f = load(txn, id)?;
    let d = f.remove(msgid)?;
    write_payload(txn, id, &f)?;
    Ok(d)
}

/// `MRGMSGF`: descriptions of `from` into `to`; returns how many.
pub fn mrgmsgf(txn: &mut Txn<'_>, from: ObjectId, to: ObjectId, replace: bool) -> Result<usize> {
    let src = load(txn, from)?;
    let mut dst = load(txn, to)?;
    let n = dst.merge(&src, replace);
    write_payload(txn, to, &dst)?;
    Ok(n)
}

/// `RTVMSG`: a description from a file resolved by name.
pub fn rtvmsg(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    names: &NameContext,
    msgid: &str,
    msgf: Name,
    lib: LibSpec,
) -> Result<Option<MsgDesc>> {
    let found = ops::resolve(txn, qsys, names, lib, msgf, types::MSGF)?;
    Ok(load(txn, found.id)?.find(msgid).cloned())
}

/// `DSPMSGD`: one description as text.
#[must_use]
pub fn dspmsgd(file: &str, d: &MsgDesc) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Message file . . . . . . :   {file}");
    let _ = writeln!(out, "Message ID . . . . . . . :   {}", d.id);
    let _ = writeln!(out, "Severity . . . . . . . . :   {:02}", d.severity);
    let _ = writeln!(out, "Message . . . . . . . . . :   {}", d.text);
    if !d.help.is_empty() {
        let _ = writeln!(out, "Cause . . . . . . . . . . :   {}", d.help);
    }
    for (i, v) in d.vars.iter().enumerate() {
        let _ = writeln!(out, "  &{:<3} {}", i + 1, v.declaration());
    }
    if let Some(r) = &d.dft_reply {
        let _ = writeln!(out, "Default reply . . . . . . :   {r}");
    }
    if !d.valid_replies.is_empty() {
        let _ = writeln!(
            out,
            "Valid replies . . . . . . :   {}",
            d.valid_replies.join(" ")
        );
    }
    out
}

/// Install the system message files (`QCPFMSG`, `QCEEMSG`, `QSQLMSG`, `QSTXTMSG`,
/// `QRPGLEMSG`, `QRNXMSG`, `QTCPMSG`, `QZSHMSGF`, `QEDTMSG`) into `QSYS`
/// when absent; returns the objects created. A product library's files
/// come with its product ([`install_product_files`]).
pub fn install_builtin(txn: &mut Txn<'_>, ctx: &Ctx, qsys: ObjectId) -> Result<Vec<ObjectId>> {
    let mut made = Vec::new();
    let lib = Library::load(txn, qsys)?;
    for f in builtin() {
        let name = Name::parse(&f.name)?;
        if lib.lookup(txn, types::MSGF, name)?.is_some() {
            continue;
        }
        let id = crtmsgf(txn, ctx, qsys, name, &f.text, true)?;
        write_payload(txn, id, &f)?;
        made.push(id);
    }
    Ok(made)
}

/// The shipped descriptions an existing system message file lacks,
/// added (a system whose files an older release made, its code since
/// replaced by a PTF). Descriptions the file has are left as they are.
/// Returns how many were added.
pub fn add_missing_descriptions(txn: &mut Txn<'_>, qsys: ObjectId) -> Result<usize> {
    let mut n = 0;
    let lib = Library::load(txn, qsys)?;
    for f in builtin() {
        let name = Name::parse(&f.name)?;
        let Some(id) = lib.lookup(txn, types::MSGF, name)? else {
            continue;
        };
        let mut have = load(txn, id)?;
        let added = have.merge(&f, false);
        if added > 0 {
            write_payload(txn, id, &have)?;
            n += added;
        }
    }
    Ok(n)
}

/// Install a product library's message files (`QHTTPSVR/QZHBMSG`), and
/// the library, when absent; returns the objects created.
pub fn install_product_files(
    txn: &mut Txn<'_>,
    ctx: &Ctx,
    qsys: ObjectId,
    library: &str,
) -> Result<Vec<ObjectId>> {
    let mut made = Vec::new();
    for (lib_name, lib_text, f) in product_files() {
        if lib_name != library {
            continue;
        }
        let lname = Name::parse(lib_name)?;
        let found = Library::load(txn, qsys)?.lookup(txn, types::LIB, lname)?;
        let plib = if let Some(id) = found {
            id
        } else {
            let id = ops::create(
                txn,
                &CreateParams {
                    library: qsys,
                    name: lname,
                    type_code: types::LIB,
                    attribute: String::new(),
                    text: lib_text.into(),
                    owner: "QSYS".into(),
                    system: ctx.system.clone(),
                    now: ctx.now,
                    by_system: true,
                },
            )?;
            made.push(id);
            id
        };
        let name = Name::parse(&f.name)?;
        if Library::load(txn, plib)?
            .lookup(txn, types::MSGF, name)?
            .is_some()
        {
            continue;
        }
        let id = crtmsgf(txn, ctx, plib, name, &f.text, true)?;
        write_payload(txn, id, &f)?;
        made.push(id);
    }
    Ok(made)
}

/// The text of an object header as a string.
trait TextString {
    fn text_string(&self) -> String;
}

impl TextString for store::obj::ObjectHeader {
    fn text_string(&self) -> String {
        store::obj::header::text_of(&self.text)
    }
}

/// `CRTMSGF CCSID`, `CHGMSGF CCSID`: the CCSID the file's text is in.
pub fn set_ccsid(txn: &mut Txn<'_>, id: ObjectId, ccsid: u16) -> Result<()> {
    let mut f = load(txn, id)?;
    f.ccsid = ccsid;
    write_payload(txn, id, &f)
}

/// The library `name` in `QSYS`, made with `text` when absent; whether
/// it was made.
pub fn ensure_library(
    txn: &mut Txn<'_>,
    ctx: &Ctx,
    qsys: ObjectId,
    name: &str,
    text: &str,
) -> Result<(ObjectId, bool)> {
    let lname = Name::parse(name)?;
    if let Some(id) = Library::load(txn, qsys)?.lookup(txn, types::LIB, lname)? {
        return Ok((id, false));
    }
    let id = ops::create(
        txn,
        &CreateParams {
            library: qsys,
            name: lname,
            type_code: types::LIB,
            attribute: String::new(),
            text: text.into(),
            owner: "QSYS".into(),
            system: ctx.system.clone(),
            now: ctx.now,
            by_system: true,
        },
    )?;
    Ok((id, true))
}

/// A secondary language's library and message files (`INSLNG`): the
/// library made when absent, each file made, or replaced when
/// `replace`; returns the library and the objects created.
pub fn install_language(
    txn: &mut Txn<'_>,
    ctx: &Ctx,
    qsys: ObjectId,
    lang: &crate::nls::Language,
    replace: bool,
) -> Result<(ObjectId, Vec<ObjectId>)> {
    let (lib, made_lib) = ensure_library(txn, ctx, qsys, &lang.library(), &lang.library_text)?;
    let mut made = Vec::new();
    if made_lib {
        made.push(lib);
    }
    for f in lang.message_files() {
        let name = Name::parse(&f.name)?;
        match Library::load(txn, lib)?.lookup(txn, types::MSGF, name)? {
            Some(id) if replace => write_payload(txn, id, &f)?,
            Some(_) => {}
            None => {
                let id = crtmsgf(txn, ctx, lib, name, &f.text, true)?;
                write_payload(txn, id, &f)?;
                made.push(id);
            }
        }
    }
    Ok((lib, made))
}
