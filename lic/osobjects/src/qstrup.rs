// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSYS/QSTRUP`, the startup program: CL source compiled against the
//! shipped command definitions, bound into a program.

use cmd::analyze::{Env, MemSource, Setting};
use ile::store::{Ctx, ObjSpec};
use ile::{ActGrpAttr, BindOptions, CreateData, ModuleObject, PgmKind};
use qsname::{LibSpec, Name};
use store::obj::library::Library;
use store::obj::ops::NameContext;
use store::obj::{JobHandle, ObjectId, ops, types};
use store::txn::Txn;

/// The startup program's CL source.
pub const SOURCE: &str = include_str!("../shipped/QSTRUP.clle");

/// A shipped CL program compiled against the shipped definitions
/// alone: the IR text.
pub fn compile_shipped(name: &str, source: &str) -> Result<String, String> {
    let mut mem = MemSource::default();
    for (n, _) in sys_cpps::defs::DEFS {
        if let Ok(o) = sys_cpps::install::object(n, 0) {
            mem.put(o.def);
        }
    }
    let mut env = Env::new(&mut mem);
    env.setting = Setting {
        interactive: false,
        program: true,
        exec: false,
        module: false,
        syntax_only: false,
    };
    let opts = cl::Options {
        name: name.to_string(),
        srcfile: "QCLSRC".into(),
        srclib: "QSYS".into(),
        srcmbr: name.to_string(),
        text: "Startup program".into(),
        ..cl::Options::default()
    };
    let c = cl::compile(source, &opts, &mut env, &mut cl::NoFiles);
    if !c.created {
        let why: Vec<String> = c.diags.iter().map(ToString::to_string).collect();
        return Err(why.join("; "));
    }
    Ok(c.qir)
}

/// Create program `lib/name` from QIR text when it does not exist yet:
/// a module, bound into the program, then dropped. Whether it was made.
pub fn ship_program(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    library: ObjectId,
    lib: &str,
    qir_text: &str,
    text: &str,
    stamp: &wm::obj::Stamp,
) -> Result<bool, String> {
    let module = qir::parse(qir_text).map_err(|e| format!("{e}"))?;
    let name = module.name.clone();
    let n = Name::parse(&name).map_err(|e| e.to_string())?;
    let l = Library::load(txn, library).map_err(|e| e.to_string())?;
    if l.lookup(txn, types::PGM, n)
        .map_err(|e| e.to_string())?
        .is_some()
    {
        return Ok(false);
    }
    let create = CreateData {
        compiler: "qirc".into(),
        ..CreateData::default()
    };
    let object = ModuleObject::compile(module, create, stamp.now).map_err(|e| e.to_string())?;
    let mut options = BindOptions::new(&name, PgmKind::Pgm);
    options.attr.actgrp = ActGrpAttr::Caller;
    let spec = ObjSpec {
        lib: LibSpec::Name(Name::parse(lib).map_err(|e| e.to_string())?),
        name: n,
    };
    let c = Ctx {
        qsys,
        names: NameContext {
            curlib: Some(library),
            libl: vec![library, qsys],
        },
        now: stamp.now,
        owner: stamp.owner.clone(),
        system: stamp.system.clone(),
    };
    let m = ile::store::crtmod(txn, &c, library, n, &object, text).map_err(|e| e.to_string())?;
    ile::store::crtpgm(txn, &c, library, &[spec], &[], options, text).map_err(|e| e.to_string())?;
    ops::delete(txn, JobHandle(0), m).map_err(|e| e.to_string())?;
    Ok(true)
}

/// `QSYS/QSTRUP` when missing; whether it was made.
pub fn install(txn: &mut Txn<'_>, qsys: ObjectId, stamp: &wm::obj::Stamp) -> Result<bool, String> {
    let qir = compile_shipped("QSTRUP", SOURCE)?;
    ship_program(txn, qsys, qsys, "QSYS", &qir, "Startup program", stamp)
}
