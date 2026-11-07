// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Collection Services commands (`docs/PERF.md`): `STRPFRCOL`,
//! `ENDPFRCOL`, `CHKPFRCOL`, `CFGPFRCOL` and `CRTPFRDTA`, over the
//! collector in `stgsvc/perfcol.rs`.

use cmd::block::Parms;
use perf::config::{Config, interval_secs};
use perf::profile::Category;
use sec::profile::SpecialAuth;
use stg::proto::CmdReply;

use super::direct::failed;
use super::done;
use crate::ipl::system::System;
use crate::stgsvc::perfcol;

/// Collection Services started a collection.
const STARTED: &str = "CPIB920";
/// Collection Services ended a collection.
const ENDED: &str = "CPIB921";
/// Collection Services is active.
const ACTIVE: &str = "CPIB922";
/// The configuration changed.
const CONFIGURED: &str = "CPIB923";
/// Performance data created.
const CREATED: &str = "CPIB924";
/// Not active (escape).
const NOT_ACTIVE: &str = "CPFB994";

/// Run `name`, or `None` when it is not one of these.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    let r = match name {
        "STRPFRCOL" => strpfrcol(system, parms),
        "ENDPFRCOL" => endpfrcol(system, parms),
        "CHKPFRCOL" => chkpfrcol(system),
        "CFGPFRCOL" => cfgpfrcol(system, parms),
        "CRTPFRDTA" => crtpfrdta(system, parms),
        _ => return None,
    };
    Some(r.unwrap_or_else(|e| failed(name, &e)))
}

/// The current configuration in keyword form, for `QSPMTOVR`.
pub fn override_values(system: &mut System) -> String {
    perfcol::config(system).keyword_form()
}

fn strpfrcol(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    crate::stgsvc::security::require(SpecialAuth::JOBCTL, "start Collection Services")?;
    let prf = parms.name("COLPRF");
    let prf = if prf.is_empty() {
        "*CFG".to_string()
    } else {
        prf
    };
    let lib = perfcol::config(system).library;
    let name = perfcol::start(system, &prf, parms.yes("CYCCOL"))?;
    Ok(done(
        format!("Collection Services started collection {name} in {lib}.\n"),
        STARTED,
        &[(&name, 10), (&lib, 10)],
    ))
}

fn endpfrcol(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    crate::stgsvc::security::require(SpecialAuth::JOBCTL, "end Collection Services")?;
    let name = perfcol::end(system, parms.yes("FRCCOLEND"))?;
    Ok(done(
        format!("Collection Services ended collection {name}.\n"),
        ENDED,
        &[(&name, 10)],
    ))
}

fn chkpfrcol(system: &System) -> Result<CmdReply, String> {
    match perfcol::check(system) {
        Some((name, lib, n)) => Ok(done(
            format!(
                "Collection Services is active: collection {name} in {lib}, {n} interval(s).\n"
            ),
            ACTIVE,
            &[(&name, 10), (&lib, 10)],
        )),
        None => Err(format!("{NOT_ACTIVE} Collection Services is not active.")),
    }
}

fn cfgpfrcol(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    crate::stgsvc::security::require(SpecialAuth::JOBCTL, "configure Collection Services")?;
    let mut c: Config = perfcol::config(system);
    let same = |v: &str| v.is_empty() || v == "*SAME";
    let v = parms.name("INTERVAL");
    if !same(&v) {
        c.interval = interval_secs(&v).ok_or_else(|| {
            "CPFB94C INTERVAL must be .25, .50, 1.0, 5.0, 15.0, 30.0 or 60.0 minutes.".to_string()
        })?;
    }
    let v = parms.name("LIB");
    if !same(&v) {
        qsname::Name::parse(&v).map_err(|e| format!("CPD0074 LIB({v}): {e}"))?;
        c.library = v;
    }
    let v = parms.name("DFTCOLPRF");
    if !same(&v) {
        c.profile = v;
    }
    let v = parms.name("CYCTIME");
    if !same(&v) {
        let t: String = v.chars().filter(char::is_ascii_digit).collect();
        if t.len() != 6
            || t[..2].parse::<u8>().unwrap_or(99) > 23
            || t[2..4].parse::<u8>().unwrap_or(99) > 59
        {
            return Err(format!("CPD0084 CYCTIME({v})"));
        }
        c.cycle_time = t;
    }
    let v = parms.name("CYCITV");
    if !same(&v) {
        c.cycle_hours = v
            .parse()
            .ok()
            .filter(|h| (1..=24).contains(h))
            .ok_or_else(|| format!("CPD0084 CYCITV({v})"))?;
    }
    let ret = parms.elems("RETPERIOD");
    let units = ret
        .first()
        .copied()
        .flatten()
        .map(cmd::Value::source)
        .unwrap_or_default()
        .to_ascii_uppercase();
    let days = ret
        .get(1)
        .copied()
        .flatten()
        .is_some_and(|x| x.source().eq_ignore_ascii_case("*DAYS"));
    if units == "*PERM" {
        c.retention = None;
    } else if !same(&units) {
        let n: u16 = units
            .parse()
            .ok()
            .filter(|n| (1..=720).contains(n))
            .ok_or_else(|| format!("CPD0084 RETPERIOD({units})"))?;
        c.retention = Some((n, days));
    }
    let v = parms.name("CRTDBF");
    if !same(&v) {
        c.create_files = v == "*YES";
    }
    let v = parms.name("CRTPFRSUM");
    if !same(&v) {
        c.create_summary = v == "*YES";
    }
    perfcol::save_config(system, &c)?;
    Ok(done(
        format!(
            "Collection Services configuration changed: {}\n",
            c.keyword_form()
        ),
        CONFIGURED,
        &[],
    ))
}

fn crtpfrdta(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    let (name, lib) = parms.qual("FROMMGTCOL");
    let (name, lib) = (name.to_ascii_uppercase(), lib.to_ascii_uppercase());
    let lib = if lib.is_empty() {
        "QPFRDATA".to_string()
    } else {
        lib
    };
    let name = match name.as_str() {
        "*ACTIVE" => perfcol::check(system)
            .map(|(n, _, _)| n)
            .ok_or_else(|| format!("{NOT_ACTIVE} Collection Services is not active."))?,
        "*SELECT" => {
            return Err("CPD0084 FROMMGTCOL(*SELECT) needs a display; name the collection".into());
        }
        "" => return Err("CPD0071 FROMMGTCOL is required".into()),
        n => n.to_string(),
    };
    let c = perfcol::read(system, &lib, &name)?;
    let pick = |kwd: &str| {
        let v = parms.name(kwd);
        if v.is_empty() || v.starts_with("*FROMMGT") {
            None
        } else {
            Some(v)
        }
    };
    let member = pick("TOMBR").unwrap_or_else(|| name.clone());
    let tolib = pick("TOLIB").unwrap_or_else(|| {
        if c.header.dbf_library.is_empty() {
            lib.clone()
        } else {
            c.header.dbf_library.clone()
        }
    });
    let cats: Vec<Category> = parms
        .list("CGY")
        .iter()
        .filter_map(|v| Category::parse(&v.source()))
        .collect();
    let n = perfcol::write_files(
        system,
        &c,
        &tolib,
        &member,
        (!cats.is_empty()).then_some(&cats[..]),
        true,
    )?;
    Ok(done(
        format!(
            "{n} records written to the performance files of {name}, member {member} in {tolib}.\n"
        ),
        CREATED,
        &[(&name, 10), (&tolib, 10)],
    ))
}
