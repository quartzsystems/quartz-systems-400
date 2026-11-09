// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Panel data for the displays that had only a text listing
//! (`docs/CPP_AUDIT.md`): the PTFs of a product and one PTF's detail
//! (`DSPPTF`), the PTF groups (`WRKPTFGRP`) and the IPL attributes
//! (`DSPIPLA`), the cover letters (`DSPPTFCVR`) and the license keys
//! (`DSPLICKEY`), the backup status (`DSPBCKSTS`) and the media
//! displays (`cppmedia.rs`). Answered through request 270 (`sys2_panel`) for the
//! job's drivers in `work/qsjob/src/panels/info/`.

use stg::proto::Sys2Data;

use crate::cpp::{iplcmd, lpcmd, opacmd, ptfcmd};
use crate::ipl::system::System;

type Vars = Vec<(String, String)>;
type Rows = Vec<Vec<(String, String)>>;

/// The data of display `what`, or `None` when it is not one of these.
pub fn panel(system: &mut System, what: &str, args: &[String]) -> Option<Result<Sys2Data, String>> {
    let arg = |i: usize| args.get(i).cloned().unwrap_or_default();
    Some(match what {
        "PTF" => ptfs(system, &arg(0), &arg(1)),
        "PTFD" => ptf_detail(system, &arg(0)),
        "PTFGRP" => groups(system, &arg(0)),
        "IPLA" => Ok(ipla(system)),
        "PTFCVR" => covers(system, args),
        "LICKEY" => lickeys(system, args),
        "BCKSTS" => backup_status(system),
        "TAP" => super::cppmedia::tape(system, args),
        "TAPCTG" => super::cppmedia::cartridges(system, args),
        "OPT" => super::cppmedia::optical(system, args),
        "OPTVOLS" => super::cppmedia::volumes(system, args),
        _ => return None,
    })
}

fn pairs(p: &[(&str, String)]) -> Vars {
    p.iter()
        .map(|(k, v)| ((*k).to_string(), v.clone()))
        .collect()
}

/// `*ALL`, blank or a name: whether `name` is wanted.
fn wanted(which: &str, name: &str) -> bool {
    let w = which.trim();
    w.is_empty() || w == "*ALL" || w == "*ONLY" || w == name
}

/// `DSPPTF LICPGM() SELECT()`: the product's PTFs, `select` a
/// comma-separated list or empty.
fn ptfs(system: &mut System, product: &str, select: &str) -> Result<Sys2Data, String> {
    let db = ptfcmd::load_db(system)?;
    let sel: Vec<&str> = select
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "*ALL")
        .collect();
    let prod = if wanted(product, "") {
        release::PRODUCT_ID.to_string()
    } else {
        release::product_id(&product.trim().to_ascii_uppercase()).to_string()
    };
    let rows: Rows = db
        .ptfs
        .values()
        .filter(|r| r.product == prod)
        .filter(|r| sel.is_empty() || sel.contains(&r.ptf.as_str()))
        .map(|r| {
            pairs(&[
                ("OPT", String::new()),
                ("PTFID", r.ptf.clone()),
                ("PSTS", r.status.display().to_string()),
                ("PACT", ipl_action(r.action)),
                ("LOADED", r.loaded.to_string()),
            ])
        })
        .collect();
    let vars = pairs(&[
        ("PRODID", prod),
        ("RELEASE", ptfcmd::release_text()),
        ("IPLSRC", format!("##MACH#{:?}", system.boot.slot)),
    ]);
    Ok((vars, vec![("PTFS".into(), rows)]))
}

/// The list's IPL action: `None` when the next IPL leaves it.
fn ipl_action(a: ptf::Action) -> String {
    match a.display() {
        "" => "None".into(),
        t => t.into(),
    }
}

/// One PTF's detail.
fn ptf_detail(system: &mut System, ptf: &str) -> Result<Sys2Data, String> {
    let db = ptfcmd::load_db(system)?;
    let r = db
        .ptfs
        .get(ptf.trim())
        .ok_or_else(|| format!("CPF3606 PTF {ptf} is not loaded"))?;
    let refs = |v: &[ptf::PtfRef]| {
        if v.is_empty() {
            "*NONE".to_string()
        } else {
            v.iter()
                .map(|x| x.ptf.clone())
                .collect::<Vec<_>>()
                .join(" ")
        }
    };
    let none = |s: &str| {
        if s.is_empty() {
            "*NONE".to_string()
        } else {
            s.to_string()
        }
    };
    let vars = pairs(&[
        ("PTFID", r.ptf.clone()),
        ("PRODID", r.product.clone()),
        ("PTFOPT", r.option.clone()),
        ("PTFRLS", r.release.clone()),
        ("PTFSTS", r.status.display().to_string()),
        ("PTFACT", none(r.action.display())),
        ("DELAYED", if r.delayed { "*YES" } else { "*NO" }.into()),
        ("LICPTF", if r.lic { "*YES" } else { "*NO" }.into()),
        ("SAVF", none(&r.savf)),
        ("SUPERSEDE", none(&r.superseded_by)),
        ("PREREQ", refs(&r.prereqs)),
        ("COREQ", refs(&r.coreqs)),
    ]);
    Ok((vars, Vec::new()))
}

/// `WRKPTFGRP PTFGRP()`.
fn groups(system: &mut System, pattern: &str) -> Result<Sys2Data, String> {
    let db = ptfcmd::load_db(system)?;
    let all = ptfcmd::load_groups(system)?;
    let p = pattern.trim().to_ascii_uppercase();
    let rows: Rows = all
        .rows(&db, &crate::cpp::trusted_keys())
        .into_iter()
        .filter(|g| match p.strip_suffix('*') {
            _ if p.is_empty() || p == "*ALL" => true,
            Some(prefix) => g.name.starts_with(prefix),
            None => g.name == p,
        })
        .map(|g| {
            pairs(&[
                ("OPT", String::new()),
                ("GRPNAME", g.name.clone()),
                ("GRPLVL", g.level.to_string()),
                ("GRPSTS", g.status.display().to_string()),
                ("GRPTEXT", g.text.clone()),
            ])
        })
        .collect();
    Ok((Vec::new(), vec![("GRPS".into(), rows)]))
}

/// `DSPIPLA`: each attribute by its keyword.
fn ipla(system: &mut System) -> Sys2Data {
    let a = iplcmd::current(system);
    let vars = licpart::ipla::ATTRS
        .iter()
        .map(|at| {
            (
                at.kwd.to_string(),
                a.get(at.kwd).unwrap_or(at.default).to_string(),
            )
        })
        .collect();
    (vars, Vec::new())
}

/// A comma-separated argument as a list, `*ALL` and blanks dropped.
fn split(s: &str) -> Vec<String> {
    s.split(',')
        .map(|v| v.trim().to_ascii_uppercase())
        .filter(|v| !v.is_empty() && v != "*ALL")
        .collect()
}

/// `DSPPTFCVR LICPGM() SELECT() CVRATR() STATUS()`: one row per cover
/// letter, its text in `LETTER`, one line per `|`-free line.
fn covers(system: &mut System, args: &[String]) -> Result<Sys2Data, String> {
    let arg = |i: usize| args.get(i).cloned().unwrap_or_default();
    // A group named in the selection (a cumulative package) is its PTFs.
    let groups = ptfcmd::load_groups(system)?;
    let keys = crate::cpp::trusted_keys();
    let select: Vec<String> = split(&arg(1))
        .into_iter()
        .flat_map(|s| {
            groups
                .groups
                .get(&s)
                .and_then(|k| k.group(&keys))
                .map_or_else(|| vec![s.clone()], |(g, _)| g.ptfs)
        })
        .collect();
    let all = ptfcmd::cover_letters(
        system,
        &arg(0),
        &select,
        &split(&arg(2)),
        &arg(3).trim().to_ascii_uppercase(),
    )?;
    let rows: Rows = all
        .iter()
        .map(|c| {
            pairs(&[
                ("OPT", String::new()),
                ("PTFID", c.ptf.clone()),
                ("PRODID", c.product.clone()),
                ("PTFRLS", c.release.clone()),
                ("PTFSTS", c.status.to_string()),
                ("CVRATR", c.attr.to_string()),
                ("LETTER", c.text.clone()),
            ])
        })
        .collect();
    Ok((Vec::new(), vec![("CVRS".into(), rows)]))
}

/// `DSPLICKEY PRDID() LICTRM() FEATURE() SERIAL()`: one row per key.
fn lickeys(system: &mut System, args: &[String]) -> Result<Sys2Data, String> {
    let arg = |i: usize| args.get(i).cloned().unwrap_or_default();
    let keys = lpcmd::lickeys(system, &arg(0), &arg(1), &arg(2), &arg(3))?;
    let rows: Rows = keys
        .iter()
        .map(|k| {
            let r = &k.row;
            let line = |i: usize| r.key.get(i).cloned().unwrap_or_default();
            pairs(&[
                ("PRDID", r.product.clone()),
                ("LICTRM", r.term.clone()),
                ("FEATURE", r.feature.clone()),
                ("PRDTEXT", k.text.clone()),
                // The serial ends where IBM's eight-character field does.
                ("SERIAL", format!("{:>8}", r.serial)),
                ("PRCGRP", r.processor_group.clone()),
                ("LICKEY", line(0)),
                ("LICKEY2", line(1)),
                ("LICKEY3", line(2)),
                ("USGLMT", r.usage_limit.clone()),
                ("EXPDATE", r.expiration.clone()),
                (
                    "VNDDTA",
                    if r.vendor_data.trim().is_empty() {
                        "*NONE".into()
                    } else {
                        r.vendor_data.clone()
                    },
                ),
            ])
        })
        .collect();
    Ok((Vec::new(), vec![("KEYS".into(), rows)]))
}

/// `DSPBCKSTS`: `<item>D` and `<item>T` for each item, the history in
/// `HIST`.
fn backup_status(system: &mut System) -> Result<Sys2Data, String> {
    let s = opacmd::backup_status(system)?;
    let mut vars = Vars::new();
    for (it, d, t) in s.items {
        vars.push((format!("{it}D"), d));
        vars.push((format!("{it}T"), t));
    }
    let rows: Rows = s
        .runs
        .into_iter()
        .map(|r| {
            pairs(&[
                ("OPT", String::new()),
                ("HDATE", r.date),
                ("HTIME", r.time),
                ("HOPT", r.option),
                ("HDEV", r.dev),
                ("HSET", r.tapset),
                ("HCHG", if r.changes { "Yes" } else { "No" }.into()),
                (
                    "HSTS",
                    if r.ok {
                        "Completed"
                    } else {
                        "Ended with errors"
                    }
                    .into(),
                ),
                ("HSTEPS", r.steps.join(";")),
            ])
        })
        .collect();
    Ok((vars, vec![("HIST".into(), rows)]))
}
