// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Panel data for the displays that had only a text listing
//! (`docs/CPP_AUDIT.md`): the PTFs of a product and one PTF's detail
//! (`DSPPTF`), the PTF groups (`WRKPTFGRP`) and the IPL attributes
//! (`DSPIPLA`). Answered through request 270 (`sys2_panel`) for the
//! job's drivers in `work/qsjob/src/panels/info/`.

use stg::proto::Sys2Data;

use crate::cpp::{iplcmd, ptfcmd};
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
        _ => return None,
    })
}

fn pairs(p: &[(&str, String)]) -> Vars {
    p.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect()
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
    let sel: Vec<&str> = select.split(',').map(str::trim).filter(|s| !s.is_empty() && *s != "*ALL").collect();
    let prod = if wanted(product, "") { release::PRODUCT_ID.to_string() } else { product.trim().to_string() };
    let rows: Rows = db
        .ptfs
        .values()
        .filter(|r| r.product == prod)
        .filter(|r| sel.is_empty() || sel.contains(&r.ptf.as_str()))
        .map(|r| {
            pairs(&[
                ("OPT", String::new()),
                ("PTFID", r.ptf.clone()),
                ("PTFSTS", r.status.display().to_string()),
                ("PTFACT", r.action.display().to_string()),
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
            v.iter().map(|x| x.ptf.clone()).collect::<Vec<_>>().join(" ")
        }
    };
    let none = |s: &str| if s.is_empty() { "*NONE".to_string() } else { s.to_string() };
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
        .map(|at| (at.kwd.to_string(), a.get(at.kwd).unwrap_or(at.default).to_string()))
        .collect();
    (vars, Vec::new())
}
