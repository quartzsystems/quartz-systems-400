// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Network attributes (`CHGNETA`, `DSPNETA`, `RTVNETA`, `QWCRNETA`):
//! `KEY=VALUE;` pairs in the character data area `QSYS/QNETATR`, made
//! at the first IPL. `SYSNAME` follows `QSYSNAME` unless changed.

use std::collections::BTreeMap;

use qsname::Name;
use store::obj::dtaara::{self, DataAreaType, DataAreaValue};
use store::obj::{CreateParams, ObjectId, types};
use store::txn::Txn;

/// The data area.
pub const AREA: &str = "QNETATR";

/// Every attribute with its shipped value, in `DSPNETA` order.
pub const DEFAULTS: &[(&str, &str)] = &[
    ("SYSNAME", ""),
    ("PNDSYSNAME", "*NONE"),
    ("LCLNETID", "APPN"),
    ("LCLCPNAME", ""),
    ("LCLLOCNAME", ""),
    ("DFTMODE", "BLANK"),
    ("NODETYPE", "*ENDNODE"),
    ("DTACPR", "*NONE"),
    ("DTACPRINM", "*NONE"),
    ("MAXINTSSN", "200"),
    ("RAR", "0"),
    ("NETSERVER", "*NONE"),
    ("ALRSTS", "*OFF"),
    ("ALRPRIFP", "*NO"),
    ("ALRDFTFP", "*NO"),
    ("ALRLOGSTS", "*NONE"),
    ("ALRBCKFP", "*NONE"),
    ("ALRRQSFP", "*NONE"),
    ("ALRCTLD", "*NONE"),
    ("ALRHLDCNT", "0"),
    ("ALRFTR", "*NONE"),
    ("MSGQ", "QSYS/QSYSOPR"),
    ("OUTQ", "QGPL/QPRINT"),
    ("JOBACN", "*FILE"),
    ("MAXHOP", "16"),
    ("DDMACC", "*OBJAUT"),
    ("PCSACC", "*OBJAUT"),
    ("DFTCNNLST", "QESDFTCNNLST"),
    ("ALWANYNET", "*NO"),
    ("NWSDOMAIN", "*NONE"),
    ("ALWVRTIP", "*NO"),
    ("ALWHPRTWR", "*NO"),
    ("VRTAUTODEV", "0"),
    ("HPRPTHTMR", "*NONE"),
    ("ALWADDCLU", "*NONE"),
    ("MDMCNTRYID", "US"),
];

/// The attributes that are numbers (`bin4` to the API, `*DEC` to CL).
pub const NUMERIC: &[&str] = &["MAXINTSSN", "RAR", "ALRHLDCNT", "MAXHOP", "VRTAUTODEV"];

/// Whether `name` is a network attribute.
#[must_use]
pub fn is_attribute(name: &str) -> bool {
    DEFAULTS.iter().any(|(k, _)| *k == name)
}

fn parse(text: &str) -> BTreeMap<String, String> {
    text.split(';')
        .filter_map(|kv| kv.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

fn unparse(map: &BTreeMap<String, String>) -> String {
    map.iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(";")
}

/// The shipped values, the system name filled in.
#[must_use]
pub fn defaults(system_name: &str) -> BTreeMap<String, String> {
    DEFAULTS
        .iter()
        .map(|(k, v)| {
            let v = match *k {
                "SYSNAME" | "LCLCPNAME" | "LCLLOCNAME" => system_name,
                _ => v,
            };
            ((*k).to_string(), v.to_string())
        })
        .collect()
}

/// The data area in `QSYS`, if made.
pub fn find(txn: &mut Txn<'_>, qsys: ObjectId) -> Option<ObjectId> {
    let lib = store::obj::library::Library::load(txn, qsys).ok()?;
    lib.lookup(txn, types::DTAARA, Name::parse(AREA).ok()?).ok()?
}

/// Read the area: the shipped values under whatever it holds.
pub fn read_area(txn: &mut Txn<'_>, id: ObjectId, system_name: &str) -> Result<BTreeMap<String, String>, String> {
    let mut map = defaults(system_name);
    if let DataAreaValue::Char(s) = dtaara::read(txn, id).map_err(|e| e.to_string())? {
        for (k, v) in parse(&s) {
            map.insert(k, v);
        }
    }
    Ok(map)
}

/// Read the attributes; the shipped values when the area is absent.
pub fn read(txn: &mut Txn<'_>, qsys: ObjectId, system_name: &str) -> Result<BTreeMap<String, String>, String> {
    match find(txn, qsys) {
        Some(id) => read_area(txn, id, system_name),
        None => Ok(defaults(system_name)),
    }
}

/// Write the attributes back.
pub fn write(txn: &mut Txn<'_>, qsys: ObjectId, map: &BTreeMap<String, String>) -> Result<(), String> {
    let id = find(txn, qsys).ok_or("CPF9801 data area QSYS/QNETATR not found")?;
    dtaara::write(txn, id, &DataAreaValue::Char(unparse(map))).map_err(|e| e.to_string())
}

/// Make the area at the first IPL; whether it was made now.
pub fn ensure(txn: &mut Txn<'_>, qsys: ObjectId, now: u64) -> Result<bool, String> {
    if find(txn, qsys).is_some() {
        return Ok(false);
    }
    let params = CreateParams {
        library: qsys,
        name: Name::parse(AREA).expect("a name"),
        type_code: types::DTAARA,
        attribute: String::new(),
        text: "Network attributes".into(),
        owner: "QSYS".into(),
        system: "QS400".into(),
        now,
        by_system: true,
    };
    dtaara::create(
        txn,
        &params,
        DataAreaType::Char { len: 2000 },
        &DataAreaValue::Char(String::new()),
    )
    .map_err(|e| e.to_string())?;
    Ok(true)
}
