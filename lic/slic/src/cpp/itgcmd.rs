// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CHKOBJITG`: object signatures checked against the keys the system
//! trusts (`/qs400/keys/system.pub`), one line per violation and a count.
//! As on IBM i only the types whose content does not change in use are
//! checked for signatures (programs, service programs, modules,
//! commands); every type is checked for damage.

use std::fmt::Write as _;

use cmd::block::Parms;
use saverst::sign::{self, Integrity};
use stg::proto::CmdReply;
use store::obj::{ops, types};

use super::direct::{failed, in_store};
use super::savcmd::trusted_keys;
use crate::ipl::system::System;

/// Run `name`, or `None` when it is not this.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    let r = match name {
        "CHKOBJITG" => chkobjitg(system, parms),
        _ => return None,
    };
    Some(r.unwrap_or_else(|e| failed(name, &e)))
}

/// Objects checked, none with a violation.
const CLEAN: &str = "CPC39E0";
/// Objects checked, some with violations.
const VIOLATIONS: &str = "CPF39E1";

/// The types whose signatures are checked.
const SIGNABLE: &[u16] = &[types::PGM, types::SRVPGM, types::MODULE, types::CMD];

/// A name against `*ALL`, a generic `ABC*` or an exact name.
fn matches(pattern: &str, name: &str) -> bool {
    match pattern {
        "" | "*ALL" => true,
        p if p.ends_with('*') => name.starts_with(&p[..p.len() - 1]),
        p => p == name,
    }
}

fn chkobjitg(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    crate::stgsvc::security::require(sec::profile::SpecialAuth::AUDIT, "check object integrity")?;
    let (obj, lib) = parms.qual("OBJ");
    let (obj, lib) = (obj.to_ascii_uppercase(), lib.to_ascii_uppercase());
    let ty = parms.text("OBJTYPE").to_ascii_uppercase();
    let type_code = match ty.as_str() {
        "" | "*ALL" => None,
        t => Some(types::type_by_name(t).ok_or_else(|| format!("CPD0084 OBJTYPE({t}) is not valid"))?.code),
    };
    let owner = parms.text("USRPRF").to_ascii_uppercase();
    let chksig = parms.text("CHKSIG").to_ascii_uppercase();
    let unsigned_too = chksig == "*ALL";
    let signatures = chksig != "*NONE";
    let keys = trusted_keys();
    let (checked, lines) = in_store(system, |txn, qsys, _| {
        let mut libs = Vec::new();
        for (k, id) in ops::list(txn, qsys, Some(types::LIB)).map_err(|e| e.to_string())? {
            if matches(&lib, &k.name.to_string()) {
                libs.push((k.name.to_string(), id));
            }
        }
        if matches(&lib, "QSYS") && !libs.iter().any(|(n, _)| n == "QSYS") {
            libs.insert(0, ("QSYS".into(), qsys));
        }
        let mut checked = 0usize;
        let mut lines = Vec::new();
        for (lname, lid) in libs {
            for (k, id) in ops::list(txn, lid, type_code).map_err(|e| e.to_string())? {
                if k.type_code == types::LIB || !matches(&obj, &k.name.to_string()) {
                    continue;
                }
                let (_, h) = ops::read_header(txn, id).map_err(|e| e.to_string())?;
                if !matches(&owner, store::obj::header::text_of(&h.owner).trim()) {
                    continue;
                }
                checked += 1;
                let what = if h.damaged {
                    Some("damaged")
                } else if !signatures || !SIGNABLE.contains(&k.type_code) {
                    None
                } else {
                    match sign::check(txn, id, &keys).map_err(|e| e.to_string())? {
                        Integrity::Good => None,
                        Integrity::Unsigned => unsigned_too.then_some("not signed"),
                        Integrity::UnknownKey => Some("signed by a key this system does not trust"),
                        Integrity::Bad => Some("signature does not match the object"),
                    }
                };
                if let Some(w) = what {
                    let tname = types::type_by_code(k.type_code).map_or("*UNKNOWN", |t| t.name);
                    lines.push(format!("  {lname}/{} {tname}: {w}", k.name));
                }
            }
        }
        Ok((checked, lines))
    })?;
    let mut text = String::new();
    for l in &lines {
        let _ = writeln!(text, "{l}");
    }
    let n = lines.len();
    let (id, line) = if n == 0 {
        (CLEAN, format!("{CLEAN} {checked} objects checked, no violations"))
    } else {
        (VIOLATIONS, format!("{VIOLATIONS} {checked} objects checked, {n} with violations"))
    };
    let _ = writeln!(text, "{line}");
    Ok(super::done(text, id, &[(&checked.to_string(), 10), (&n.to_string(), 10)]))
}
