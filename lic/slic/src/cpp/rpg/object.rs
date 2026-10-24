// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The compiled IR into the store: the `*MODULE`, and for a program or
//! service program the bind with the binding directories the command
//! and the control specification name.

use ile::store::{Ctx, ObjSpec};
use ile::{ActGrpAttr, BindOptions, CreateData, ModuleObject, PgmKind, UserProfile};
use qsname::{LibSpec, Name};
use store::Txn;
use store::obj::library::Library;
use store::obj::{JobHandle, ObjectId, ops, types};

/// What a compile makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// `CRTRPGMOD`, `CRTSQLRPGI OBJTYPE(*MODULE)`.
    Module,
    /// `CRTBNDRPG`, `CRTSQLRPGI OBJTYPE(*PGM)`.
    Program,
    /// `CRTSQLRPGI OBJTYPE(*SRVPGM)`.
    SrvPgm,
}

/// The object to make.
#[derive(Clone, Debug)]
pub struct Made {
    /// Its kind.
    pub shape: Shape,
    /// Its name.
    pub name: String,
    /// Its text.
    pub text: String,
    /// `REPLACE(*YES)`.
    pub replace: bool,
    /// `USRPRF(*OWNER)`.
    pub usrprf_owner: bool,
    /// Binding directories, `LIB/NAME` or `NAME`.
    pub bnddir: Vec<String>,
    /// The debug source view (`DBGVIEW(*SOURCE)`, `*COPY`, `*LIST`,
    /// `*ALL`), when one was asked for.
    pub source: Option<Vec<String>>,
}

/// A binding directory named as `LIB/NAME` or `NAME`.
fn bnddir_spec(s: &str) -> Result<ObjSpec, String> {
    let (lib, name) = s.rsplit_once('/').map_or(("*LIBL", s), |(l, n)| (l, n));
    let name = Name::parse(name).map_err(|e| format!("CPD0078 BNDDIR({s}): {e}"))?;
    let lib = match lib {
        "*LIBL" | "" => LibSpec::Libl,
        "*CURLIB" => LibSpec::Curlib,
        l => LibSpec::Name(Name::parse(l).map_err(|e| format!("CPD0078 BNDDIR({s}): {e}"))?),
    };
    Ok(ObjSpec { lib, name })
}

/// Make the object in library `lib_id`.
///
/// # Errors
///
/// A message identifier and text: the IR refused, the object exists
/// and `REPLACE(*NO)`, or the bind failed.
pub fn make(
    txn: &mut Txn<'_>,
    c: &Ctx,
    lib_id: ObjectId,
    made: &Made,
    qir_text: &str,
    create: CreateData,
    now: u64,
) -> Result<(), String> {
    let module = qir::parse(qir_text)
        .map_err(|e| format!("CPF0006 the generated IR does not parse: {e}"))?;
    qir::verify(&module).map_err(|e| format!("CPF0006 the generated IR does not verify: {e}"))?;
    // The control specification has the last word on the group.
    let actgrp = if module.dftactgrp {
        ActGrpAttr::Default
    } else {
        match &module.actgrp {
            qir::ActGrp::New => ActGrpAttr::New,
            qir::ActGrp::Caller => ActGrpAttr::Caller,
            qir::ActGrp::Named(n) => ActGrpAttr::Named(n.clone()),
        }
    };
    let mut object = ModuleObject::compile(module, create, now).map_err(|e| format!("CPF0006 {e}"))?;
    object.source.clone_from(&made.source);
    let n = Name::parse(&made.name).map_err(|e| format!("CPD0078 PGM({}): {e}", made.name))?;
    let lib_name = ops::describe(txn, lib_id).map_err(|e| e.to_string())?.name;
    let ty = match made.shape {
        Shape::Module => types::MODULE,
        Shape::Program => types::PGM,
        Shape::SrvPgm => types::SRVPGM,
    };
    let l = Library::load(txn, lib_id).map_err(|e| e.to_string())?;
    if let Some(old) = l.lookup(txn, ty, n).map_err(|e| e.to_string())? {
        if !made.replace {
            return Err(format!(
                "CPF5D0D {} already exists in {lib_name}",
                made.name
            ));
        }
        ops::delete(txn, JobHandle(0), old).map_err(|e| e.to_string())?;
    }
    if made.shape != Shape::Module
        && let Some(old) = l.lookup(txn, types::MODULE, n).map_err(|e| e.to_string())?
    {
        // A module of the program's name is in the way of the binder's
        // temporary one.
        ops::delete(txn, JobHandle(0), old).map_err(|e| e.to_string())?;
    }
    let m =
        ile::store::crtmod(txn, c, lib_id, n, &object, &made.text).map_err(|e| e.to_string())?;
    if made.shape == Shape::Module {
        return Ok(());
    }
    let kind = if made.shape == Shape::SrvPgm {
        PgmKind::SrvPgm
    } else {
        PgmKind::Pgm
    };
    let mut options = BindOptions::new(&made.name, kind);
    options.attr.actgrp = actgrp;
    if made.usrprf_owner {
        options.attr.usrprf = UserProfile::Owner;
    }
    let spec = ObjSpec {
        lib: LibSpec::Name(lib_name),
        name: n,
    };
    // BNDDIR(*NONE) names no directory, however qualified.
    let dirs = made
        .bnddir
        .iter()
        .filter(|b| !b.rsplit('/').next().unwrap_or(b).starts_with('*'))
        .map(|b| bnddir_spec(b))
        .collect::<Result<Vec<_>, _>>()?;
    let bound = ile::store::crtpgm(txn, c, lib_id, &[spec], &dirs, options, &made.text)
        .map_err(|e| format!("CPF5D12 {e}"));
    // The module was the binder's input only.
    ops::delete(txn, JobHandle(0), m).map_err(|e| e.to_string())?;
    bound.map(|_| ())
}
