// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The objects in the store: creating, reading and updating `*MODULE`,
//! `*PGM`, `*SRVPGM` and `*BNDDIR` objects, resolving the names the
//! commands take, and `QUSAPIBD` at the end of every bind.

use qsname::{LibSpec, Name};
use store::Txn;
use store::obj::space::Space;
use store::obj::{CreateParams, NameContext, ObjectId, ops, types};

use crate::binder::{BindInput, BindOptions, Diag, Resolved, SrvPgmInfo, bind};
use crate::error::{IleError, Result};
use crate::object::{BndDir, BndDirEntry, BndEntryKind, ModuleObject, PgmKind, ProgramObject};

/// What every operation needs: QSYS, the name context, who and when.
#[derive(Clone, Debug)]
pub struct Ctx {
    /// `QSYS`.
    pub qsys: ObjectId,
    /// Library list and current library.
    pub names: NameContext,
    /// Unix seconds.
    pub now: u64,
    /// Owner profile.
    pub owner: String,
    /// System name.
    pub system: String,
}

impl Ctx {
    fn params(
        &self,
        library: ObjectId,
        name: Name,
        type_code: u16,
        attribute: &str,
        text: &str,
    ) -> CreateParams {
        CreateParams {
            library,
            name,
            type_code,
            attribute: attribute.into(),
            text: text.into(),
            owner: self.owner.clone(),
            system: self.system.clone(),
            now: self.now,
            by_system: false,
        }
    }

    /// Resolve an object of a type.
    pub fn resolve(
        &self,
        txn: &mut Txn<'_>,
        lib: LibSpec,
        name: Name,
        type_code: u16,
    ) -> Result<ObjectId> {
        Ok(ops::resolve(txn, self.qsys, &self.names, lib, name, type_code)?.id)
    }
}

fn write_payload(txn: &mut Txn<'_>, id: ObjectId, bytes: &[u8]) -> Result<()> {
    let space = Space::ensure(txn, id, u64::try_from(bytes.len()).expect("fits"), 0)?;
    space.write(txn, 0, bytes)?;
    Ok(())
}

fn read_payload(txn: &mut Txn<'_>, id: ObjectId) -> Result<Vec<u8>> {
    let (space, _) = Space::load(txn, id)?;
    let len = usize::try_from(space.bytes()).expect("fits");
    Ok(space.read(txn, 0, len)?)
}

/// The library name of an object, for displays and errors.
pub fn library_name(txn: &mut Txn<'_>, id: ObjectId) -> Result<String> {
    let h = ops::describe(txn, id)?;
    let lib = ops::describe(txn, h.library)?;
    Ok(lib.name.to_string())
}

/// Create a `*MODULE`.
pub fn crtmod(
    txn: &mut Txn<'_>,
    ctx: &Ctx,
    library: ObjectId,
    name: Name,
    m: &ModuleObject,
    text: &str,
) -> Result<ObjectId> {
    let id = ops::create(
        txn,
        &ctx.params(library, name, types::MODULE, m.ir.lang.name(), text),
    )?;
    write_payload(txn, id, &m.encode())?;
    Ok(id)
}

/// Read a `*MODULE`.
pub fn load_module(txn: &mut Txn<'_>, id: ObjectId) -> Result<ModuleObject> {
    if id.type_code != types::MODULE {
        return Err(wrong_type(txn, id, "*MODULE")?);
    }
    ModuleObject::decode(&read_payload(txn, id)?)
}

/// Read a `*PGM` or `*SRVPGM`.
pub fn load_program(txn: &mut Txn<'_>, id: ObjectId) -> Result<ProgramObject> {
    if id.type_code != types::PGM && id.type_code != types::SRVPGM {
        return Err(wrong_type(txn, id, "*PGM")?);
    }
    let h = ops::describe(txn, id)?;
    let mut p = ProgramObject::decode(&read_payload(txn, id)?)?;
    p.name = h.name.to_string();
    Ok(p)
}

/// Read a `*BNDDIR`.
pub fn load_bnddir(txn: &mut Txn<'_>, id: ObjectId) -> Result<BndDir> {
    if id.type_code != types::BNDDIR {
        return Err(wrong_type(txn, id, "*BNDDIR")?);
    }
    BndDir::decode(&read_payload(txn, id)?)
}

fn wrong_type(txn: &mut Txn<'_>, id: ObjectId, wanted: &str) -> Result<IleError> {
    let h = ops::describe(txn, id)?;
    Ok(IleError::WrongType {
        name: h.name.to_string(),
        found: store::obj::type_by_code(id.type_code)
            .map_or("?", |t| t.name)
            .into(),
        wanted: wanted.into(),
    })
}

/// Create a `*BNDDIR`, empty.
pub fn crtbnddir(
    txn: &mut Txn<'_>,
    ctx: &Ctx,
    library: ObjectId,
    name: Name,
    text: &str,
) -> Result<ObjectId> {
    let id = ops::create(txn, &ctx.params(library, name, types::BNDDIR, "", text))?;
    write_payload(txn, id, &BndDir::default().encode())?;
    Ok(id)
}

/// `ADDBNDDIRE`: add entries at the end or at a position.
pub fn addbnddire(
    txn: &mut Txn<'_>,
    id: ObjectId,
    entries: Vec<BndDirEntry>,
    position: Option<usize>,
) -> Result<()> {
    let mut d = load_bnddir(txn, id)?;
    let mut at = position;
    for e in entries {
        d.add(e, at);
        at = at.map(|p| p + 1);
    }
    write_payload(txn, id, &d.encode())
}

/// `RMVBNDDIRE`: remove entries; returns how many went.
pub fn rmvbnddire(
    txn: &mut Txn<'_>,
    id: ObjectId,
    name: &str,
    lib: Option<&str>,
    kind: BndEntryKind,
) -> Result<usize> {
    let mut d = load_bnddir(txn, id)?;
    let n = d.remove(name, lib, kind);
    write_payload(txn, id, &d.encode())?;
    Ok(n)
}

/// A module or binding directory the commands name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjSpec {
    /// Library.
    pub lib: LibSpec,
    /// Name.
    pub name: Name,
}

/// `CRTPGM` or `CRTSRVPGM`: bind the modules named, with the binding
/// directories named and `QUSAPIBD`, into a new object in `library`.
pub fn crtpgm(
    txn: &mut Txn<'_>,
    ctx: &Ctx,
    library: ObjectId,
    modules: &[ObjSpec],
    bnddirs: &[ObjSpec],
    options: BindOptions,
    text: &str,
) -> Result<(ObjectId, Vec<Diag>)> {
    let name = Name::parse(&options.name)?;
    let type_code = match options.attr.kind {
        PgmKind::Pgm => types::PGM,
        PgmKind::SrvPgm => types::SRVPGM,
    };
    let (program, diags) = bind_in_store(txn, ctx, modules, bnddirs, options)?;
    let id = ops::create(txn, &ctx.params(library, name, type_code, "", text))?;
    write_payload(txn, id, &program.encode())?;
    Ok((id, diags))
}

/// Bind without creating: the program object and the diagnostics.
pub fn bind_in_store(
    txn: &mut Txn<'_>,
    ctx: &Ctx,
    modules: &[ObjSpec],
    bnddirs: &[ObjSpec],
    mut options: BindOptions,
) -> Result<(ProgramObject, Vec<Diag>)> {
    options.attr.created = ctx.now;
    let mut listed = Vec::with_capacity(modules.len());
    for m in modules {
        let id = ctx.resolve(txn, m.lib, m.name, types::MODULE)?;
        let lib = library_name(txn, id)?;
        listed.push((load_module(txn, id)?, lib));
    }
    let mut dirs = Vec::new();
    for d in bnddirs {
        let id = ctx.resolve(txn, d.lib, d.name, types::BNDDIR)?;
        dirs.push(load_bnddir(txn, id)?);
    }
    if let Ok(id) = ctx.resolve(
        txn,
        LibSpec::Name(ops::qsys_name()),
        Name::parse("QUSAPIBD")?,
        types::BNDDIR,
    ) {
        dirs.push(load_bnddir(txn, id)?);
    }
    // Resolve every directory entry up front; the binder then works in memory.
    let mut found: Vec<(BndDirEntry, Option<Resolved>)> = Vec::new();
    for d in &dirs {
        for e in &d.entries {
            if found.iter().any(|(x, _)| x == e) {
                continue;
            }
            let r = resolve_entry(txn, ctx, e)?;
            found.push((e.clone(), r));
        }
    }
    let lookup = |e: &BndDirEntry| -> Option<Resolved> {
        found
            .iter()
            .find(|(x, _)| x == e)
            .and_then(|(_, r)| r.clone())
    };
    bind(BindInput {
        modules: listed,
        bnddirs: dirs,
        lookup: &lookup,
        options,
    })
}

fn resolve_entry(txn: &mut Txn<'_>, ctx: &Ctx, e: &BndDirEntry) -> Result<Option<Resolved>> {
    let lib = if e.lib == "*LIBL" {
        LibSpec::Libl
    } else {
        LibSpec::Name(Name::parse(&e.lib)?)
    };
    let name = Name::parse(&e.name)?;
    Ok(match e.kind {
        BndEntryKind::Module => match ctx.resolve(txn, lib, name, types::MODULE) {
            Ok(id) => {
                let l = library_name(txn, id)?;
                Some(Resolved::Module(load_module(txn, id)?, l))
            }
            Err(_) => None,
        },
        BndEntryKind::SrvPgm => match ctx.resolve(txn, lib, name, types::SRVPGM) {
            Ok(id) => {
                let p = load_program(txn, id)?;
                let l = library_name(txn, id)?;
                Some(Resolved::SrvPgm(SrvPgmInfo {
                    name: p.name.clone(),
                    lib: if e.lib == "*LIBL" { "*LIBL".into() } else { l },
                    exports: p.exports.iter().map(|x| (x.name.clone(), x.kind)).collect(),
                    signature: p
                        .signature()
                        .unwrap_or(crate::binder::signature::Signature([0; 16])),
                }))
            }
            Err(_) => None,
        },
    })
}

/// `UPDPGM` and `UPDSRVPGM`: replace the named bound modules with the
/// current `*MODULE` objects of the same names and bind again with the
/// program's attributes and exports. Modules not named stay as bound.
pub fn updpgm(
    txn: &mut Txn<'_>,
    ctx: &Ctx,
    pgm: ObjectId,
    replace: &[ObjSpec],
) -> Result<Vec<Diag>> {
    let old = load_program(txn, pgm)?;
    let mut modules: Vec<(ModuleObject, String)> = Vec::new();
    for bm in &old.modules {
        let fresh = replace
            .iter()
            .find(|r| r.name.to_string() == bm.module.name);
        match fresh {
            Some(r) => {
                let id = ctx.resolve(txn, r.lib, r.name, types::MODULE)?;
                let lib = library_name(txn, id)?;
                modules.push((load_module(txn, id)?, lib));
            }
            None => modules.push((bm.module.clone(), bm.lib.clone())),
        }
    }
    let export = match (&old.attr.kind, &old.binder_source) {
        (PgmKind::SrvPgm, Some(src)) => crate::binder::ExportSpec::Source(
            crate::binder::lang::parse_binder_source(src)?,
            src.clone(),
        ),
        _ => crate::binder::ExportSpec::All,
    };
    let options = BindOptions {
        name: old.name.clone(),
        attr: old.attr.clone(),
        entry_module: old
            .entry
            .map(|(mi, _)| old.modules[mi as usize].module.name.clone()),
        export,
        allow_unresolved: false,
    };
    let lookup = |_: &BndDirEntry| -> Option<Resolved> { None };
    let (program, diags) = bind(BindInput {
        modules,
        bnddirs: Vec::new(),
        lookup: &lookup,
        options,
    })?;
    write_payload(txn, pgm, &program.encode())?;
    Ok(diags)
}
