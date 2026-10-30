// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The TCP/IP configuration files in `QUSRSYS`: created when missing
//! (with a new system's configuration in them), read whole, and
//! rewritten a table at a time.

use dds::{DataType, FieldDef, PfDefinition, RecordFormat};
use qsname::{MemberSpec, Name};
use store::obj::library::Library;
use store::obj::{ObjectId, types};
use store::txn::Txn;
use tcpcfg::Config;
use tcpcfg::files::{self, Layout, Table, Ty, defs};

use crate::ipl::system::System;

/// The library the files live in.
pub const LIB: &str = "QUSRSYS";

fn lib_of(txn: &mut Txn<'_>, qsys: ObjectId) -> Result<ObjectId, String> {
    wm::obj::library(txn, qsys, LIB).map_err(|e| format!("{LIB}: {e}"))
}

fn find(txn: &mut Txn<'_>, lib: ObjectId, file: &str) -> Result<Option<ObjectId>, String> {
    let n = Name::parse(file).map_err(|e| e.to_string())?;
    Library::load(txn, lib)
        .and_then(|l| l.lookup(txn, types::FILE, n))
        .map_err(|e| e.to_string())
}

/// A layout as a physical file definition.
fn definition(l: &Layout) -> Result<PfDefinition, String> {
    let mut f = RecordFormat::new(Name::parse(l.format).map_err(|e| e.to_string())?);
    for fd in l.fields {
        let ty = match fd.ty {
            Ty::A => DataType::Char,
            Ty::B => DataType::Binary,
            Ty::G => DataType::Graphic,
        };
        let mut x = FieldDef::new(Name::parse(fd.name).map_err(|e| e.to_string())?, ty, fd.len, 0);
        x.text = Some(fd.text.to_string());
        if fd.ty == Ty::G {
            x.ccsid = Some(13488);
        }
        f.fields.push(x);
    }
    f.lay_out();
    if f.record_len as usize != l.record_len() {
        return Err(format!("{}: record length {} is not {}", l.file, f.record_len, l.record_len()));
    }
    Ok(PfDefinition {
        format: f,
        unique: None,
        dup_order: None,
        reference: None,
        refaccpth: None,
        altseq: None,
        ccsid: None,
        text: Some(l.text.to_string()),
    })
}

fn read(txn: &mut Txn<'_>, file: ObjectId) -> Result<Vec<Vec<u8>>, String> {
    let m = db::Member::open_by(txn, file, MemberSpec::First).map_err(|e| e.to_string())?;
    Ok(m.scan(txn, 1, usize::MAX)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|s| s.record)
        .collect())
}

fn write(txn: &mut Txn<'_>, file: ObjectId, recs: &[Vec<u8>]) -> Result<(), String> {
    let mut m = db::Member::open_by(txn, file, MemberSpec::First).map_err(|e| e.to_string())?;
    m.clear(txn).map_err(|e| e.to_string())?;
    let nulls = vec![0u8; usize::from(m.format().null_map_len)];
    for r in recs {
        m.write_new(txn, r, &nulls).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Create the files that are missing, each with a new system's
/// records; how many were made. The `QTCPMSG` message file comes with
/// them on a system installed before it existed.
pub fn install(txn: &mut Txn<'_>, qsys: ObjectId, stamp: &wm::obj::Stamp) -> Result<usize, String> {
    let ctx = msgf::store::Ctx {
        now: stamp.now,
        owner: stamp.owner.clone(),
        system: stamp.system.clone(),
    };
    let mut made = msgf::store::install_builtin(txn, &ctx, qsys)
        .map_err(|e| format!("message files: {e}"))?
        .len();
    let lib = lib_of(txn, qsys)?;
    let fresh = Config::default();
    for t in Table::ALL {
        let recs = files::encode(&fresh, *t);
        for l in t.files() {
            let data = recs
                .iter()
                .find(|(f, _)| *f == l.file)
                .map(|(_, r)| r.clone())
                .unwrap_or_default();
            if find(txn, lib, l.file)?.is_some() {
                continue;
            }
            let def = definition(l)?;
            let opts = db::CreateFileOpts {
                text: l.text.to_string(),
                owner: "QSYS".into(),
                system: stamp.system.clone(),
                now: stamp.now,
                ..db::CreateFileOpts::default()
            };
            let name = Name::parse(l.file).map_err(|e| e.to_string())?;
            let id = db::file::create_pf(txn, lib, name, &def, &opts).map_err(|e| format!("{}: {e}", l.file))?;
            write(txn, id, &data)?;
            made += 1;
        }
    }
    if find(txn, lib, defs::QATOCRSI.file)?.is_none() {
        let def = definition(&defs::QATOCRSI)?;
        let name = Name::parse(defs::QATOCRSI.file).map_err(|e| e.to_string())?;
        db::file::create_pf(
            txn,
            lib,
            name,
            &def,
            &db::CreateFileOpts {
                text: defs::QATOCRSI.text.to_string(),
                owner: "QSYS".into(),
                ..db::CreateFileOpts::default()
            },
        )
        .map_err(|e| e.to_string())?;
        made += 1;
    }
    Ok(made)
}

/// Run `f` in a transaction on ASP 1, as the system.
fn with_store<T>(
    system: &mut System,
    f: impl FnOnce(&mut Txn<'_>, ObjectId) -> Result<T, String>,
) -> Result<T, String> {
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    sec::guard::as_system(|| crate::svc::db::with_txn(&mut s.store, |txn| f(txn, qsys)))
}

/// The configuration from the files.
pub fn load(system: &mut System) -> Result<Config, String> {
    with_store(system, |txn, qsys| {
        let lib = lib_of(txn, qsys)?;
        let mut failure = None;
        let cfg = files::decode(&mut |file| {
            let got = find(txn, lib, file).and_then(|id| match id {
                Some(id) => read(txn, id),
                None => Ok(Vec::new()),
            });
            got.unwrap_or_else(|e| {
                failure.get_or_insert(e);
                Vec::new()
            })
        });
        match failure {
            Some(e) => Err(e),
            None => Ok(cfg),
        }
    })
}

/// Rewrite a table's files.
pub fn save(system: &mut System, cfg: &Config, t: Table) -> Result<(), String> {
    with_store(system, |txn, qsys| {
        let lib = lib_of(txn, qsys)?;
        for (file, recs) in files::encode(cfg, t) {
            let id = find(txn, lib, file)?.ok_or_else(|| format!("CPF9812 File {file} in library {LIB} not found."))?;
            write(txn, id, &recs)?;
        }
        Ok(())
    })
}

/// `CPYTCPHT`: the IPv4 host table records into `lib/file` (created in
/// the `QHOSTS` format when missing), replacing its records or added.
pub fn copy_to(system: &mut System, lib: &str, file: &str, recs: &[Vec<u8>], add: bool) -> Result<(), String> {
    with_store(system, |txn, qsys| {
        let l = wm::obj::library(txn, qsys, lib).map_err(|e| format!("CPF2110 {lib}: {e}"))?;
        let id = if let Some(id) = find(txn, l, file)? {
            id
        } else {
            let def = definition(&defs::QATOCHOST)?;
            let name = Name::parse(file).map_err(|e| e.to_string())?;
            db::file::create_pf(txn, l, name, &def, &db::CreateFileOpts::default()).map_err(|e| e.to_string())?
        };
        if add {
            let mut m = db::Member::open_by(txn, id, MemberSpec::First).map_err(|e| e.to_string())?;
            let nulls = vec![0u8; usize::from(m.format().null_map_len)];
            for r in recs {
                m.write_new(txn, r, &nulls).map_err(|e| e.to_string())?;
            }
            Ok(())
        } else {
            write(txn, id, recs)
        }
    })
}
