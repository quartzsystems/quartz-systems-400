// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*CMD` objects in the store: `CRTCMD`, `CHGCMD`, `CHGCMDDFT`,
//! `DLTCMD`, `DSPCMD`, the proxy commands, and the analyzer's source
//! of definitions resolved through a job's library list.

use std::fmt::Write as _;

use qsname::{LibSpec, Name};
use store::Txn;
use store::obj::space::Space;
use store::obj::{CreateParams, JobHandle, NameContext, ObjectId, guard, ops, types};

use crate::analyze::CmdSource;
use crate::def::{Allow, Mode, Qual, ThdSafe};
use crate::error::{CmdError, Diag, Result};
use crate::payload::{CmdObject, Override};
use crate::token::{self, Options, Tok};

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

/// A library's name.
fn lib_name(txn: &mut Txn<'_>, id: ObjectId) -> String {
    ops::describe(txn, id)
        .map(|h| h.name.to_string())
        .unwrap_or_default()
}

fn write_payload(txn: &mut Txn<'_>, id: ObjectId, o: &CmdObject) -> Result<()> {
    let bytes = o.encode();
    let space = Space::ensure(txn, id, u64::try_from(bytes.len()).expect("fits"), 0)?;
    space.write(txn, 0, &bytes)?;
    Ok(())
}

/// Read a `*CMD`, checking the authority to run it.
pub fn load(txn: &mut Txn<'_>, id: ObjectId) -> Result<CmdObject> {
    guard::check_id(txn, "cmd.run", id)?;
    load_unchecked(txn, id)
}

fn load_unchecked(txn: &mut Txn<'_>, id: ObjectId) -> Result<CmdObject> {
    let (space, _) = Space::load(txn, id)?;
    let len = usize::try_from(space.bytes()).expect("fits");
    let bytes = space.read(txn, 0, len)?;
    CmdObject::decode(&bytes)
}

/// `CRTCMD`: create `lib/name` holding `object`; with `replace`, an
/// existing command is replaced.
pub fn crtcmd(
    txn: &mut Txn<'_>,
    ctx: &Ctx,
    library: ObjectId,
    name: Name,
    object: &CmdObject,
    replace: bool,
    by_system: bool,
) -> Result<ObjectId> {
    let lib = store::obj::library::Library::load(txn, library)?;
    if let Some(old) = lib.lookup(txn, types::CMD, name)? {
        if !replace {
            return Err(CmdError::with_data(
                "CPF0201",
                &[(&name.to_string(), 10), (&lib_name(txn, library), 10)],
            ));
        }
        ops::delete(txn, JobHandle(0), old)?;
    }
    let id = ops::create(
        txn,
        &CreateParams {
            library,
            name,
            type_code: types::CMD,
            attribute: String::new(),
            text: object.def.text.clone(),
            owner: ctx.owner.clone(),
            system: ctx.system.clone(),
            now: ctx.now,
            by_system,
        },
    )?;
    write_payload(txn, id, object)?;
    Ok(id)
}

/// What `CHGCMD` may change.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Changes {
    /// `PGM`.
    pub cpp: Option<Qual>,
    /// `VLDCKR`; `Some(None)` for `*NONE`.
    pub vldckr: Option<Option<Qual>>,
    /// `MODE`.
    pub mode: Option<Mode>,
    /// `ALLOW`.
    pub allow: Option<Allow>,
    /// `ALWLMTUSR`.
    pub alwlmtusr: Option<bool>,
    /// `PMTOVRPGM`.
    pub pmtovrpgm: Option<Option<Qual>>,
    /// `HLPPNLGRP`.
    pub hlppnlgrp: Option<Option<Qual>>,
    /// `HLPID`.
    pub hlpid: Option<Option<String>>,
    /// `MSGF`.
    pub msgf: Option<Qual>,
    /// `PRDLIB`.
    pub prdlib: Option<Option<String>>,
    /// `CURLIB`.
    pub curlib: Option<Option<String>>,
    /// `THDSAFE`.
    pub thdsafe: Option<ThdSafe>,
    /// `TEXT`.
    pub text: Option<String>,
}

/// `CHGCMD`.
pub fn chgcmd(txn: &mut Txn<'_>, id: ObjectId, c: &Changes, now: u64) -> Result<()> {
    guard::check_id(txn, "cmd.change", id)?;
    let mut o = load_unchecked(txn, id)?;
    let d = &mut o.def;
    if let Some(v) = &c.cpp {
        d.cpp.clone_from(v);
    }
    if let Some(v) = &c.vldckr {
        d.vldckr.clone_from(v);
    }
    if let Some(v) = c.mode {
        d.mode = v;
    }
    if let Some(v) = c.allow {
        d.allow = v;
    }
    if let Some(v) = c.alwlmtusr {
        d.alwlmtusr = v;
    }
    if let Some(v) = &c.pmtovrpgm {
        d.pmtovrpgm.clone_from(v);
    }
    if let Some(v) = &c.hlppnlgrp {
        d.hlppnlgrp.clone_from(v);
    }
    if let Some(v) = &c.hlpid {
        d.hlpid.clone_from(v);
    }
    if let Some(v) = &c.msgf {
        d.msgf.clone_from(v);
    }
    if let Some(v) = &c.prdlib {
        d.prdlib.clone_from(v);
    }
    if let Some(v) = &c.curlib {
        d.curlib.clone_from(v);
    }
    if let Some(v) = c.thdsafe {
        d.thdsafe = v;
    }
    if let Some(t) = &c.text {
        d.text.clone_from(t);
        ops::change_text(txn, id, t, now)?;
    }
    write_payload(txn, id, &o)
}

/// `CHGCMDDFT NEWDFT('KWD(value) ...')`: the overrides parsed and
/// checked against the definition, then kept with the object.
pub fn chgcmddft(txn: &mut Txn<'_>, id: ObjectId, newdft: &str) -> Result<Vec<Override>> {
    guard::check_id(txn, "cmd.change", id)?;
    let mut o = load_unchecked(txn, id)?;
    let overrides = parse_newdft(&o.def, newdft)?;
    for n in &overrides {
        o.overrides.retain(|x| x.kwd != n.kwd);
        o.overrides.push(n.clone());
    }
    write_payload(txn, id, &o)?;
    Ok(overrides)
}

/// The `NEWDFT` string into overrides, each keyword one of the
/// definition's with a default (`CPD0202`, `CPD0203`).
pub fn parse_newdft(def: &crate::def::CmdDef, newdft: &str) -> Result<Vec<Override>> {
    let toks = token::tokenize_list(newdft, Options::default())?;
    let mut out = Vec::new();
    for t in toks {
        match t {
            Tok::Kw { name, args, .. } => {
                let Some(p) = def.parm(&name) else {
                    return Err(Diag::new("CPD0202")
                        .data(&[(&name, 10), (&def.name, 10)])
                        .detail(format!("{name} is not a keyword of {}", def.name))
                        .into());
                };
                if p.dft.is_none() && p.constant.is_none() && p.required() {
                    return Err(Diag::new("CPD0203")
                        .data(&[(&name, 10), (&def.name, 10)])
                        .detail(format!("{name} is required and has no default to change"))
                        .into());
                }
                out.push(Override {
                    kwd: name,
                    dft: Tok::join(&args),
                });
            }
            other => {
                return Err(Diag::new("CPD0202")
                    .data(&[(&other.text(), 10), (&def.name, 10)])
                    .detail(format!("{} is not KEYWORD(value)", other.text()))
                    .into());
            }
        }
    }
    Ok(out)
}

/// `DLTCMD`.
pub fn dltcmd(txn: &mut Txn<'_>, job: JobHandle, id: ObjectId) -> Result<()> {
    ops::delete(txn, job, id)?;
    Ok(())
}

/// `CRTPRXCMD`.
pub fn crtprxcmd(
    txn: &mut Txn<'_>,
    ctx: &Ctx,
    library: ObjectId,
    name: Name,
    target: Qual,
    text: &str,
    replace: bool,
) -> Result<ObjectId> {
    let o = CmdObject::proxy(&name.to_string(), target, text, ctx.now);
    crtcmd(txn, ctx, library, name, &o, replace, false)
}

/// `CHGPRXCMD`: a new target and text.
pub fn chgprxcmd(
    txn: &mut Txn<'_>,
    id: ObjectId,
    target: Option<Qual>,
    text: Option<&str>,
    now: u64,
) -> Result<()> {
    guard::check_id(txn, "cmd.change", id)?;
    let mut o = load_unchecked(txn, id)?;
    if o.proxy.is_none() {
        return Err(CmdError::detail("CPF0006", "not a proxy command"));
    }
    if let Some(t) = target {
        o.proxy = Some(t);
    }
    if let Some(t) = text {
        o.def.text = t.into();
        ops::change_text(txn, id, t, now)?;
    }
    write_payload(txn, id, &o)
}

/// Find `name` as `CRTCMD` and friends name a command: in a library,
/// `*CURLIB` or through the list.
pub fn find(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    names: &NameContext,
    name: &str,
    lib: &str,
) -> Result<ops::Found> {
    let n = Name::parse(name).map_err(|e| CmdError::detail("CPD0078", e.to_string()))?;
    let spec = match lib.to_ascii_uppercase().as_str() {
        "*LIBL" | "" => LibSpec::Libl,
        "*CURLIB" => LibSpec::Curlib,
        l => LibSpec::Name(Name::parse(l).map_err(|e| CmdError::detail("CPD0078", e.to_string()))?),
    };
    Ok(ops::resolve(txn, qsys, names, spec, n, types::CMD)?)
}

/// `DSPCMD`: the command's attributes as text.
#[must_use]
pub fn dspcmd(o: &CmdObject, lib: &str) -> String {
    let mut out = String::new();
    let d = &o.def;
    let _ = writeln!(out, "Command  . . . . . . . . . . . . . . :   {}", d.name);
    let _ = writeln!(out, "  Library  . . . . . . . . . . . . . :     {lib}");
    if let Some(t) = &o.proxy {
        let _ = writeln!(out, "Proxy target command . . . . . . . . :   {}", t.name);
        let _ = writeln!(out, "  Library  . . . . . . . . . . . . . :     {}", t.lib);
        let _ = writeln!(out, "Text . . . . . . . . . . . . . . . . :   {}", d.text);
        return out;
    }
    let _ = writeln!(
        out,
        "Program to process command . . . . . :   {}",
        d.cpp.name
    );
    let _ = writeln!(
        out,
        "  Library  . . . . . . . . . . . . . :     {}",
        d.cpp.lib
    );
    let q = |o: &Option<Qual>| o.as_ref().map_or("*NONE".to_string(), Qual::text);
    let _ = writeln!(
        out,
        "Validity checking program  . . . . . :   {}",
        q(&d.vldckr)
    );
    let _ = writeln!(
        out,
        "Mode in which valid  . . . . . . . . :   {}",
        d.mode.names().join(" ")
    );
    let _ = writeln!(
        out,
        "Where allowed to run . . . . . . . . :   {}",
        d.allow.names().join(" ")
    );
    let _ = writeln!(
        out,
        "Allow limited user . . . . . . . . . :   {}",
        if d.alwlmtusr { "*YES" } else { "*NO" }
    );
    let _ = writeln!(
        out,
        "Maximum positional parameters  . . . :   {}",
        d.maxpos.map_or("*NONE".to_string(), |m| m.to_string())
    );
    let _ = writeln!(
        out,
        "Prompt override program  . . . . . . :   {}",
        q(&d.pmtovrpgm)
    );
    let _ = writeln!(
        out,
        "Message file . . . . . . . . . . . . :   {}",
        d.msgf.text()
    );
    let _ = writeln!(
        out,
        "Help panel group . . . . . . . . . . :   {}",
        q(&d.hlppnlgrp)
    );
    let _ = writeln!(
        out,
        "Help identifier  . . . . . . . . . . :   {}",
        d.hlpid.as_deref().unwrap_or("*NONE")
    );
    let _ = writeln!(
        out,
        "Product library  . . . . . . . . . . :   {}",
        d.prdlib.as_deref().unwrap_or("*NONE")
    );
    let _ = writeln!(
        out,
        "Current library  . . . . . . . . . . :   {}",
        d.curlib.as_deref().unwrap_or("*NOCHG")
    );
    let _ = writeln!(
        out,
        "Threadsafe . . . . . . . . . . . . . :   {}",
        match d.thdsafe {
            ThdSafe::No => "*NO",
            ThdSafe::Cond => "*COND",
            ThdSafe::Yes => "*YES",
        }
    );
    let _ = writeln!(out, "Text . . . . . . . . . . . . . . . . :   {}", d.text);
    if !o.overrides.is_empty() {
        let _ = writeln!(out, "Defaults changed (CHGCMDDFT):");
        for ov in &o.overrides {
            let _ = writeln!(out, "  {:<10} {}", ov.kwd, ov.dft);
        }
    }
    let _ = writeln!(
        out,
        "Parameters . . . . . . . . . . . . . :   {}",
        d.parms.len()
    );
    for p in &d.parms {
        let _ = writeln!(
            out,
            "  {:<10} {:<10} {:<12} {}",
            p.kwd,
            p.ty.text(),
            format_len(p),
            p.dft.as_deref().unwrap_or("")
        );
    }
    out
}

fn format_len(p: &crate::def::Parm) -> String {
    let l = p.length();
    if p.ty == crate::def::ParmType::Dec {
        format!("({} {})", l.len, l.dec)
    } else if p.ty.is_group() {
        String::new()
    } else {
        format!("({})", l.len)
    }
}

/// The analyzer's source over a store: `*CMD` objects found through
/// the job's library list, with the authority to run them checked.
pub struct StoreSource<'t, 'a> {
    /// The transaction.
    pub txn: &'t mut Txn<'a>,
    /// `QSYS`.
    pub qsys: ObjectId,
    /// The job's names.
    pub names: NameContext,
}

impl CmdSource for StoreSource<'_, '_> {
    fn lookup(&mut self, name: &str, lib: Option<&str>) -> Result<Option<(CmdObject, String)>> {
        let found = match find(
            self.txn,
            self.qsys,
            &self.names,
            name,
            lib.unwrap_or("*LIBL"),
        ) {
            Ok(f) => f,
            Err(e)
                if matches!(
                    e.escape.msgid.as_str(),
                    "CPF9801" | "CPF9810" | "CPF2105" | "CPF9815" | "CPF9812"
                ) =>
            {
                return Ok(None);
            }
            Err(e) => return Err(e),
        };
        let o = load(self.txn, found.id)?;
        Ok(Some((o, lib_name(self.txn, found.library))))
    }
}
