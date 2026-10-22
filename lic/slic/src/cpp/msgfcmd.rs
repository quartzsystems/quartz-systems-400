// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The message file commands on the `msgf` store functions: `CRTMSGF`,
//! `CHGMSGF`, `ADDMSGD`, `CHGMSGD`, `RMVMSGD`, `MRGMSGF`.

use cmd::Value;
use cmd::block::Parms;
use msgf::desc::{MsgDesc, VarType};
use qsname::Name;
use stg::proto::CmdReply;
use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use super::direct::{failed, in_store, lib_or, library, plain};
use crate::ipl::system::{System, now_unix};

/// Run `name`, or `None` when it is not a message file command.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    let r = match name {
        "CRTMSGF" => crtmsgf(system, parms),
        "CHGMSGF" => chgmsgf(system, parms),
        "ADDMSGD" | "CHGMSGD" => addmsgd(system, name, parms),
        "RMVMSGD" => rmvmsgd(system, parms),
        "MRGMSGF" => mrgmsgf(system, parms),
        _ => return None,
    };
    Some(match r {
        Ok(text) => plain(text),
        Err(e) => failed(name, &e),
    })
}

/// A message file by `(name, library)`.
fn find(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    names: &ops::NameContext,
    (name, lib): &(String, String),
) -> Result<ObjectId, String> {
    wm::obj::find(txn, qsys, names, &lib_or(lib), name, types::MSGF)
        .map_err(|_| format!("CPF2407 message file {}/{name} not found", lib_or(lib)))
}

fn crtmsgf(system: &mut System, parms: &Parms) -> Result<String, String> {
    let (n, l) = parms.qual("MSGF");
    let text = parms.text("TEXT");
    let owner = system.svc_user.clone().unwrap_or_else(|| "QSYS".into());
    let ctx = msgf::store::Ctx {
        now: now_unix(),
        owner,
        system: system.system_name(),
    };
    in_store(system, |txn, qsys, names| {
        let lib = library(txn, qsys, names, &l)?;
        let name = Name::parse(&n).map_err(|e| format!("CPD0078 MSGF({n}): {e}"))?;
        msgf::store::crtmsgf(txn, &ctx, lib, name, &text, false)
            .map(|_| format!("Message file {n} created in library {}.\n", lib_or(&l)))
            .map_err(|e| format!("CPF2112 {e}"))
    })
}

fn chgmsgf(system: &mut System, parms: &Parms) -> Result<String, String> {
    let q = parms.qual("MSGF");
    let text = parms.text("TEXT");
    let given = parms.specified("TEXT");
    let now = now_unix();
    in_store(system, |txn, qsys, names| {
        let id = find(txn, qsys, names, &q)?;
        if given {
            let t = if text == "*BLANK" { "" } else { &text };
            ops::change_text(txn, id, t, now).map_err(|e| e.to_string())?;
        }
        Ok(format!("Message file {} changed.\n", q.0))
    })
}

/// The words of one `FMT` element, as `VarType::parse` reads them.
fn fmt_words(v: &Value) -> String {
    match v {
        Value::Elems(parts) => parts
            .iter()
            .flatten()
            .map(Value::source)
            .collect::<Vec<_>>()
            .join(" "),
        other => other.source(),
    }
}

/// The description `ADDMSGD` and `CHGMSGD` describe.
fn desc_of(parms: &Parms) -> Result<MsgDesc, String> {
    let id = parms.name("MSGID");
    if !MsgDesc::valid_id(&id) {
        return Err(format!("CPD0084 MSGID({id}): three letters and four digits"));
    }
    let mut vars = Vec::new();
    for item in parms.list("FMT") {
        let words = fmt_words(item);
        if words.is_empty() || words.starts_with("*NONE") {
            continue;
        }
        vars.push(VarType::parse(&words).map_err(|e| format!("CPD0084 FMT({words}): {e}"))?);
    }
    let text = |kw: &str| {
        let t = parms.text(kw);
        if t == "*NONE" { String::new() } else { t }
    };
    let dft = text("DFT");
    let valid_replies: Vec<String> = parms
        .list("VALUES")
        .iter()
        .map(|v| v.source().to_ascii_uppercase())
        .filter(|v| v != "*NONE")
        .collect();
    let dump: Vec<u16> = parms
        .list("DMPLST")
        .iter()
        .filter_map(|v| v.source().parse().ok())
        .collect();
    Ok(MsgDesc {
        id,
        severity: u16::try_from(parms.int("SEV").unwrap_or(0)).unwrap_or(0),
        text: text("MSG"),
        help: text("SECLVL"),
        vars,
        dft_reply: (!dft.is_empty()).then_some(dft),
        valid_replies,
        dump,
    })
}

fn addmsgd(system: &mut System, name: &str, parms: &Parms) -> Result<String, String> {
    let q = parms.qual("MSGF");
    let d = desc_of(parms)?;
    let adding = name == "ADDMSGD";
    in_store(system, |txn, qsys, names| {
        let id = find(txn, qsys, names, &q)?;
        let msgid = d.id.clone();
        if adding {
            msgf::store::addmsgd(txn, id, d).map_err(|e| format!("CPF2499 {e}"))?;
            Ok(format!("Message {msgid} added to message file {}.\n", q.0))
        } else {
            msgf::store::chgmsgd(txn, id, d).map_err(|e| format!("CPF2419 {e}"))?;
            Ok(format!("Message {msgid} changed in message file {}.\n", q.0))
        }
    })
}

fn rmvmsgd(system: &mut System, parms: &Parms) -> Result<String, String> {
    let q = parms.qual("MSGF");
    let msgid = parms.name("MSGID");
    in_store(system, |txn, qsys, names| {
        let id = find(txn, qsys, names, &q)?;
        msgf::store::rmvmsgd(txn, id, &msgid).map_err(|e| format!("CPF2419 {e}"))?;
        Ok(format!("Message {msgid} removed from message file {}.\n", q.0))
    })
}

fn mrgmsgf(system: &mut System, parms: &Parms) -> Result<String, String> {
    let from = parms.qual("FROMMSGF");
    let to = parms.qual("TOMSGF");
    let replace = parms.yes("RPLMSGF");
    in_store(system, |txn, qsys, names| {
        let f = find(txn, qsys, names, &from)?;
        let t = find(txn, qsys, names, &to)?;
        let n = msgf::store::mrgmsgf(txn, f, t, replace).map_err(|e| format!("CPF2499 {e}"))?;
        Ok(format!(
            "{n} message description(s) merged from {} into {}.\n",
            from.0, to.0
        ))
    })
}
