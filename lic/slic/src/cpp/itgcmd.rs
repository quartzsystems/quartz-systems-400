// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CHKOBJITG`: object signatures checked against the keys the system
//! trusts (`/qs400/keys/system.pub`), one line per violation and a count,
//! and with `OUTFILE` the violations in IBM's model format. As on IBM i only the types whose content does not change in use are
//! checked for signatures (programs, service programs, modules,
//! commands); every type is checked for damage.

use std::fmt::Write as _;

use cmd::block::Parms;
use saverst::sign::{self, Integrity};
use stg::proto::CmdReply;
use store::obj::{ops, types};

use super::direct::{failed, in_store};
use super::savout::{self, Fld, Row};
use super::savcmd::trusted_keys;
use crate::ipl::system::{System, now_unix};

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

/// A violation: its `AIVIOL` code (Quartz's choice; the oracle had
/// none to show) and the line's words.
type Kind = (&'static str, &'static str);
const DAMAGED: Kind = ("*DAMAGED", "damaged");
const UNSIGNED: Kind = ("*UNSIGNED", "not signed");
const UNTRUSTED: Kind = ("*UNTRUSTED", "signed by a key this system does not trust");
const MISMATCH: Kind = ("*SIGNATURE", "signature does not match the object");

/// An object that failed the check.
struct Violation {
    lib: String,
    name: String,
    ty: String,
    owner: String,
    what: Kind,
}

/// `QSYCHKIT`, the format of IBM's model outfile `QASYCHKI`: 23 fields,
/// 10151 bytes, as the V7R1M0 oracle's `DSPFFD` shows it
/// (`golden/verified/oracle71/questions/dspffd-qasychki-*`).
const QSYCHKIT: &[Fld] = &[
    ("AIDCEN", 'A', 1, 0), ("AIDDAT", 'A', 6, 0), ("AIDTIM", 'A', 6, 0),
    ("AISYST", 'A', 8, 0), ("AIOIND", 'A', 1, 0), ("AINAME", 'A', 10, 0),
    ("AILIB", 'A', 10, 0), ("AITYPE", 'A', 8, 0), ("AIOWNR", 'A', 10, 0),
    ("AIVIOL", 'A', 10, 0), ("AITRUN", 'A', 1, 0), ("AICCSD", 'B', 5, 0),
    ("AICTID", 'A', 2, 0), ("AILNID", 'A', 3, 0), ("AIRES1", 'A', 3, 0),
    ("AIPBYT", 'P', 5, 0), ("AIFLID", 'A', 16, 0), ("AIRES2", 'A', 20, 0),
    ("AIPATH", 'V', 9997, 0), ("AILASP", 'A', 10, 0), ("AILASN", 'A', 5, 0),
    ("AIOASP", 'A', 10, 0), ("AIOASN", 'A', 5, 0),
];

/// One outfile record: when (`CYYMMDD`, `HHMMSS`), where, the object
/// by name (`AIOIND` 0) in the system ASP.
fn outfile_row(v: &Violation, sys: &str, now: u64) -> Row {
    let s = savout::stamp(now);
    vec![
        ("AIDCEN", s[..1].to_string()),
        ("AIDDAT", s[1..7].to_string()),
        ("AIDTIM", s[7..13].to_string()),
        ("AISYST", sys.to_string()),
        ("AIOIND", "0".into()),
        ("AINAME", v.name.clone()),
        ("AILIB", v.lib.clone()),
        ("AITYPE", v.ty.clone()),
        ("AIOWNR", v.owner.clone()),
        ("AIVIOL", v.what.0.into()),
        ("AITRUN", "0".into()),
        ("AILASP", "*SYSBAS".into()),
        ("AILASN", "00001".into()),
        ("AIOASP", "*SYSBAS".into()),
        ("AIOASN", "00001".into()),
    ]
}

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
                    Some(DAMAGED)
                } else if !signatures || !SIGNABLE.contains(&k.type_code) {
                    None
                } else {
                    match sign::check(txn, id, &keys).map_err(|e| e.to_string())? {
                        Integrity::Good => None,
                        Integrity::Unsigned => unsigned_too.then_some(UNSIGNED),
                        Integrity::UnknownKey => Some(UNTRUSTED),
                        Integrity::Bad => Some(MISMATCH),
                    }
                };
                if let Some(w) = what {
                    let tname = types::type_by_code(k.type_code).map_or("*UNKNOWN", |t| t.name);
                    let owner = store::obj::header::text_of(&h.owner).trim().to_string();
                    lines.push(Violation {
                        lib: lname.clone(),
                        name: k.name.to_string(),
                        ty: tname.to_string(),
                        owner,
                        what: w,
                    });
                }
            }
        }
        Ok((checked, lines))
    })?;
    let mut text = String::new();
    for v in &lines {
        let _ = writeln!(text, "  {}/{} {}: {}", v.lib, v.name, v.ty, v.what.1);
    }
    let n = lines.len();
    // As on the system, an outfile only when there is something in it.
    if !parms.qual("OUTFILE").0.is_empty() && n > 0 {
        let sys = system.system_name();
        let rows: Vec<Row> = lines.iter().map(|v| outfile_row(v, &sys, now_unix())).collect();
        text.push_str(&savout::write(system, parms, "QSYCHKIT", QSYCHKIT, &rows)?);
    }
    let (id, line) = if n == 0 {
        (CLEAN, format!("{CLEAN} {checked} objects checked, no violations"))
    } else {
        (VIOLATIONS, format!("{VIOLATIONS} {checked} objects checked, {n} with violations"))
    };
    let _ = writeln!(text, "{line}");
    Ok(super::done(text, id, &[(&checked.to_string(), 10), (&n.to_string(), 10)]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_outfile_is_ibms_model_format() {
        assert_eq!(QSYCHKIT.len(), 23);
        let f = savout::model("QSYCHKIT", QSYCHKIT);
        assert_eq!(f.record_len, 10151);
        let v = Violation {
            lib: "QGPL".into(),
            name: "X".into(),
            ty: "*PGM".into(),
            owner: "QSECOFR".into(),
            what: MISMATCH,
        };
        let row = outfile_row(&v, "S1", 1_793_349_915);
        let get = |k: &str| row.iter().find(|(n, _)| *n == k).map(|(_, v)| v.as_str());
        assert_eq!(get("AIDCEN"), Some("1"));
        assert_eq!(get("AIDDAT"), Some("261030"));
        assert_eq!(get("AIDTIM"), Some("084515"));
        assert_eq!(get("AIVIOL"), Some("*SIGNATURE"));
        assert!(savout::encode(&f, &row).is_ok());
    }
}
