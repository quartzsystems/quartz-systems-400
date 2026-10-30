// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QSH's requests in the service: the system-level environment
//! variables, kept in the user space `QUSRSYS/QP0ZSYSENV` (made on the
//! first change), and the profiles' user and group IDs. Changing a
//! system-level variable takes `*ALLOBJ`.

use qsname::Name;
use sec::auth::principal::Principal;
use sec::profile::SpecialAuth;
use stg::qsh::{EnvVar, ProfileIds, QshOp, QshReply, set_mode};
use store::obj::ObjectId;
use store::obj::library::Library;
use store::obj::types;
use store::txn::Txn;

use crate::ipl::system::System;

/// The library of the variables' space.
const LIB: &str = "QUSRSYS";
/// The space.
const NAME: &str = "QP0ZSYSENV";
/// Most variables a level holds.
pub const MAX_VARS: usize = 4095;

/// Answer one request for `me`.
pub fn request(system: &mut System, me: &Principal, op: QshOp) -> QshReply {
    if matches!(op, QshOp::SysEnvSet { .. } | QshOp::SysEnvRemove { .. })
        && me.require(SpecialAuth::ALLOBJ, "change system-level environment variables").is_err()
    {
        return QshReply::failed(
            "CPFA98E",
            format!("User {} may not change system-level environment variables.", me.user),
        );
    }
    let Some(s) = system.storage.as_mut() else {
        return QshReply::failed("CPF9898", "The object store is not open.");
    };
    let Some(qsys) = s.qsys else {
        return QshReply::failed("CPF9898", "The object store is not initialized.");
    };
    let mut reply = None;
    let r = crate::svc::db::with_txn(&mut s.store, |txn| {
        let out = match op {
            QshOp::SysEnv => QshReply::Env {
                vars: load(txn, qsys)?,
            },
            QshOp::SysEnvSet { var, mode } => {
                let mut vars = load(txn, qsys)?;
                match change(&mut vars, var, mode) {
                    Ok(()) => {
                        save(txn, qsys, &vars)?;
                        QshReply::Done
                    }
                    Err(f) => f,
                }
            }
            QshOp::SysEnvRemove { name } => {
                let mut vars = load(txn, qsys)?;
                if name == "*ALL" {
                    vars.clear();
                } else if let Some(i) = vars.iter().position(|v| v.name == name) {
                    vars.remove(i);
                } else {
                    reply = Some(QshReply::failed(
                        "CPFA981",
                        format!("There is no environment variable {name} at level *SYS."),
                    ));
                    return Ok(());
                }
                save(txn, qsys, &vars)?;
                QshReply::Done
            }
            QshOp::Profiles => QshReply::Profiles {
                rows: profiles(txn, qsys)?,
            },
        };
        reply = Some(out);
        Ok(())
    });
    match (r, reply) {
        (Ok(()), Some(out)) => out,
        (Ok(()), None) => QshReply::Done,
        (Err(e), _) => QshReply::failed("CPF9898", e),
    }
}

/// Apply an add or change.
pub fn change(vars: &mut Vec<EnvVar>, var: EnvVar, mode: u8) -> Result<(), QshReply> {
    match (vars.iter().position(|v| v.name == var.name), mode) {
        (Some(_), set_mode::ADD) => Err(QshReply::failed(
            "CPFA980",
            format!("An environment variable {} is already at level *SYS.", var.name),
        )),
        (None, set_mode::CHANGE) => Err(QshReply::failed(
            "CPFA981",
            format!("There is no environment variable {} at level *SYS.", var.name),
        )),
        (Some(i), _) => {
            vars[i] = var;
            Ok(())
        }
        (None, _) if vars.len() >= MAX_VARS => Err(QshReply::failed(
            "CPFA984",
            format!("The limit of {MAX_VARS} environment variables has been reached."),
        )),
        (None, _) => {
            vars.push(var);
            Ok(())
        }
    }
}

fn space(txn: &mut Txn<'_>, qsys: ObjectId) -> Result<Option<ObjectId>, String> {
    let Ok(lib) = wm::obj::library(txn, qsys, LIB) else {
        return Ok(None);
    };
    let l = Library::load(txn, lib).map_err(|e| e.to_string())?;
    l.lookup(txn, types::USRSPC, Name::parse(NAME).expect("valid"))
        .map_err(|e| e.to_string())
}

fn load(txn: &mut Txn<'_>, qsys: ObjectId) -> Result<Vec<EnvVar>, String> {
    let Some(id) = space(txn, qsys)? else {
        return Ok(Vec::new());
    };
    let b = wm::obj::read(txn, id).map_err(|e| e.to_string())?;
    if b.is_empty() {
        return Ok(Vec::new());
    }
    ipc::decode(&b).map_err(|e| format!("{LIB}/{NAME}: {e}"))
}

fn save(txn: &mut Txn<'_>, qsys: ObjectId, vars: &Vec<EnvVar>) -> Result<(), String> {
    let bytes = ipc::encode(vars);
    if let Some(id) = space(txn, qsys)? {
        return wm::obj::write(txn, id, &bytes).map_err(|e| e.to_string());
    }
    let lib = wm::obj::library(txn, qsys, LIB).map_err(|e| e.to_string())?;
    let stamp = wm::obj::Stamp {
        owner: "QSYS".into(),
        system: "QS400".into(),
        now: crate::ipl::system::now_unix(),
    };
    wm::obj::create(
        txn,
        lib,
        Name::parse(NAME).expect("valid"),
        types::USRSPC,
        "System-level environment variables",
        &stamp,
        &bytes,
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

fn profiles(txn: &mut Txn<'_>, qsys: ObjectId) -> Result<Vec<ProfileIds>, String> {
    let mut rows = Vec::new();
    for n in sec::profile::objects::list(txn, qsys).map_err(|e| e.to_string())? {
        let Ok((_, p)) = sec::profile::objects::read(txn, qsys, &n) else {
            continue;
        };
        rows.push(ProfileIds {
            name: n,
            uid: p.uid,
            gid: p.gid,
            home: p.homedir.clone(),
        });
    }
    Ok(rows)
}
