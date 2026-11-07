// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The security attributes and function usage commands: `CHGSECA`,
//! `DSPSECA`, `CHGFCNUSG`, `DSPFCNUSG` (and `WRKFCNUSG`'s text form
//! outside a display), and `STRPASTHR`, which finds no remote location
//! on a system without APPC. The data the job's panels show is
//! [`seca_rows`] and [`functions`].

use std::fmt::Write as _;

use cmd::block::Parms;
use sec::fcnusg::{Change, Current};
use sec::profile::SpecialAuth;
use stg::proto::CmdReply;

use super::{done, escape};
use crate::ipl::system::System;
use crate::svc::security::in_store;

/// Run `name`, or `None` when it is not one of these.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    Some(match name {
        "CHGSECA" => chgseca(system, parms),
        "DSPSECA" => {
            let text = seca_text(system);
            text_or_print(system, parms, "QPSECA", text)
        }
        "CHGFCNUSG" => chgfcnusg(system, parms),
        "DSPFCNUSG" | "WRKFCNUSG" => {
            let pattern = fcnid(parms);
            match functions(system, &pattern) {
                Ok(list) if list.is_empty() => escape("CPI223E", name, ""),
                Ok(list) => {
                    let text = fcn_text(system, &list);
                    text_or_print(system, parms, "QPSECFCN", Ok(text))
                }
                Err(e) => failed(name, &e),
            }
        }
        // APPC is not part of QS/400: no route to any remote location.
        "STRPASTHR" => escape("CPF8933", name, ""),
        _ => return None,
    })
}

fn failed(name: &str, e: &str) -> CmdReply {
    let id = super::leading_msgid(e).unwrap_or("CPF9898").to_string();
    let detail = e.get(8..).unwrap_or(e).trim();
    escape(&id, name, detail)
}

fn fcnid(parms: &Parms) -> String {
    let p = parms.name("FCNID");
    if p.is_empty() { "*ALL".into() } else { p }
}

fn text_or_print(
    system: &mut System,
    parms: &Parms,
    file: &str,
    text: Result<String, String>,
) -> CmdReply {
    let text = match text {
        Ok(t) => t,
        Err(e) => return failed(file, &e),
    };
    if parms.name("OUTPUT") == "*PRINT" {
        let lines: Vec<String> = text.lines().map(str::to_string).collect();
        return match crate::svc::security::print(system, file, &lines) {
            Ok(t) => CmdReply {
                text: t,
                ..CmdReply::default()
            },
            Err(e) => failed(file, &e),
        };
    }
    CmdReply {
        text,
        ..CmdReply::default()
    }
}

fn yn(b: bool) -> &'static str {
    if b { "*YES" } else { "*NO" }
}

/// `DSPSECA`'s values: label and value, as the display shows them.
pub fn seca_rows(system: &mut System) -> Result<Vec<(String, String)>, String> {
    let flags = system
        .lic
        .as_ref()
        .and_then(|(_, r)| r.tlv(svctools::dst::security::TAG_SECURITY))
        .and_then(|t| t.first().copied())
        // Shipped: security system values and certificates may change.
        .map_or(
            svctools::env::SecurityFlags {
                allow_sysval_security_changes: true,
                allow_add_certificates: true,
                ..svctools::env::SecurityFlags::default()
            },
            svctools::dst::security::decode,
        );
    let vals = crate::svc::security::sysvals(system);
    let get = |n: &str| vals.get(n).cloned().unwrap_or_default().trim().to_string();
    let (uid, gid) = in_store(system, |txn, env| {
        Ok((
            sec::profile::groups::next_id(txn, env.qsys, false)?,
            sec::profile::groups::next_id(txn, env.qsys, true)?,
        ))
    })?;
    Ok(vec![
        ("UID".into(), uid.to_string()),
        ("GID".into(), gid.to_string()),
        ("QSECURITY".into(), get("QSECURITY")),
        ("QPWDLVL".into(), get("QPWDLVL")),
        (
            "SYSVAL".into(),
            yn(flags.allow_sysval_security_changes).into(),
        ),
        ("CERT".into(), yn(flags.allow_add_certificates).into()),
        (
            "STPWD".into(),
            yn(flags.allow_default_password_change).into(),
        ),
    ])
}

/// A label and its value as the text forms line them up.
fn line(system: &mut System, id: &str, value: &str, width: usize) -> String {
    let label = crate::stgsvc::msgs::text(system, id, &[]);
    format!("{label:<width$}:   {value}\n")
}

fn seca_text(system: &mut System) -> Result<String, String> {
    let rows = seca_rows(system)?;
    let v = |k: &str| {
        rows.iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    let mut s = String::new();
    for (id, k) in [
        ("TXT0B01", "UID"),
        ("TXT0B02", "GID"),
        ("TXT0B03", "QSECURITY"),
        ("TXT0B04", "QPWDLVL"),
        ("TXT0B05", "SYSVAL"),
        ("TXT0B06", "CERT"),
        ("TXT0B07", "STPWD"),
    ] {
        s.push_str(&line(system, id, &v(k), 56));
    }
    Ok(s)
}

/// `CHGSECA`'s prompt override (IBM's `QSYPOATR`): the numbers the next
/// profiles get, in keyword form.
pub fn override_values(system: &mut System) -> String {
    seca_rows(system).map_or_else(
        |_| String::new(),
        |rows| {
            let v = |k: &str| {
                rows.iter()
                    .find(|(n, _)| n == k)
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default()
            };
            format!("UID({}) GID({})", v("UID"), v("GID"))
        },
    )
}

/// `CHGSECA UID() GID()`: `*SECADM`; `CPC2231` names where generated
/// numbers start now.
fn chgseca(system: &mut System, parms: &Parms) -> CmdReply {
    const NAME: &str = "CHGSECA";
    if let Err(e) = crate::stgsvc::security::require(SpecialAuth::SECADM, "CHGSECA") {
        return failed(NAME, &e);
    }
    let value = |k: &str| -> Option<u32> {
        if parms.name(k) == "*SAME" {
            return None;
        }
        parms
            .int(k)
            .and_then(|n| u32::try_from(n).ok())
            .filter(|n| *n != 0)
    };
    let (uid, gid) = (value("UID"), value("GID"));
    let r = in_store(system, |txn, env| {
        let before = (
            sec::profile::groups::next_id(txn, env.qsys, false)?,
            sec::profile::groups::next_id(txn, env.qsys, true)?,
        );
        sec::seca::change(txn, env.qsys, uid, gid)?;
        let after = (
            sec::profile::groups::next_id(txn, env.qsys, false)?,
            sec::profile::groups::next_id(txn, env.qsys, true)?,
        );
        Ok((before, after))
    });
    match r {
        Ok(((u0, g0), (u1, g1))) => {
            let f = |n: u32| n.to_string();
            done(
                String::new(),
                "CPC2231",
                &[(&f(u0), 10), (&f(u1), 10), (&f(g0), 10), (&f(g1), 10)],
            )
        }
        Err(e) => failed(NAME, &e),
    }
}

/// The functions matching `pattern`, with each profile's usage and
/// whether it is a group; `CPF228C` when a name matches none.
pub fn functions(system: &mut System, pattern: &str) -> Result<Vec<(Current, Vec<bool>)>, String> {
    crate::stgsvc::security::require(SpecialAuth::SECADM, "WRKFCNUSG")?;
    in_store(system, |txn, env| {
        let reg = sec::fcnusg::load(txn, env.qsys)?;
        let list = reg.list(pattern);
        if list.is_empty() && !pattern.ends_with('*') && pattern != "*ALL" {
            return Err(sec::SecError::msg("CPF228C", pattern.to_string()));
        }
        let mut out = Vec::new();
        for c in list {
            let groups = c
                .users
                .iter()
                .map(|(u, _)| sec::profile::groups::is_group(txn, env.qsys, u).unwrap_or(false))
                .collect();
            out.push((c, groups));
        }
        Ok(out)
    })
}

fn usage(a: bool) -> &'static str {
    if a { "*ALLOWED" } else { "*DENIED" }
}

fn fcn_text(system: &mut System, list: &[(Current, Vec<bool>)]) -> String {
    let mut s = String::new();
    let (user, group) = (
        crate::stgsvc::msgs::text(system, "TXT0B18", &[]),
        crate::stgsvc::msgs::text(system, "TXT0B19", &[]),
    );
    for (c, groups) in list {
        let f = c.function;
        for (id, v) in [
            ("TXT0B10", f.id),
            ("TXT0B11", f.name),
            ("TXT0B12", f.desc),
            ("TXT0B13", f.product),
            ("TXT0B14", f.group),
            ("TXT0B15", usage(c.default_allowed)),
            ("TXT0B16", if c.allobj_used { "*USED" } else { "*NOTUSED" }),
        ] {
            s.push_str(&line(system, id, v, 44));
        }
        if c.users.is_empty() {
            let none = crate::stgsvc::msgs::text(system, "TXT0B17", &[]);
            let _ = writeln!(s, "  {none}");
        }
        for ((u, a), g) in c.users.iter().zip(groups) {
            let _ = writeln!(
                s,
                "  {u:<10}  {:<13}  {}",
                if *g { &group } else { &user },
                usage(*a)
            );
        }
        s.push('\n');
    }
    s
}

/// `CHGFCNUSG FCNID() USER() USAGE() DEFAULT() ALLOBJAUT()`: `*SECADM`.
fn chgfcnusg(system: &mut System, parms: &Parms) -> CmdReply {
    const NAME: &str = "CHGFCNUSG";
    if let Err(e) = crate::stgsvc::security::require(SpecialAuth::SECADM, "CHGFCNUSG") {
        return failed(NAME, &e);
    }
    let id = parms.name("FCNID");
    let user = parms.name("USER");
    let change = Change {
        user: (!user.is_empty()).then(|| {
            let u = match parms.name("USAGE").as_str() {
                "*ALLOWED" => Some(true),
                "*DENIED" => Some(false),
                _ => None,
            };
            (user.clone(), u)
        }),
        default_allowed: match parms.name("DEFAULT").as_str() {
            "*ALLOWED" => Some(true),
            "*DENIED" => Some(false),
            _ => None,
        },
        allobj_used: match parms.name("ALLOBJAUT").as_str() {
            "*USED" => Some(true),
            "*NOTUSED" => Some(false),
            _ => None,
        },
    };
    let r = in_store(system, |txn, env| {
        if let Some((u, Some(_))) = &change.user {
            sec::profile::objects::read(txn, env.qsys, u)?;
        }
        let mut reg = sec::fcnusg::load(txn, env.qsys)?;
        reg.change(&id, &change)?;
        sec::fcnusg::save(txn, env.qsys, &reg)
    });
    match r {
        Ok(()) => CmdReply::default(),
        Err(e) => failed(NAME, &e),
    }
}

/// The job's panels' data (request 270): `SECA` (`DSPSECA`) and
/// `FCNUSG` (`WRKFCNUSG`, `DSPFCNUSG`; the pattern as the argument).
pub fn panel(
    system: &mut System,
    what: &str,
    args: &[String],
) -> Option<Result<stg::proto::Sys2Data, String>> {
    Some(match what {
        "SECA" => seca_rows(system).map(|v| (v, Vec::new())),
        "FCNUSG" => {
            let pattern = args.first().cloned().unwrap_or_else(|| "*ALL".into());
            let user = crate::stgsvc::msgs::text(system, "TXT0B18", &[]);
            let group = crate::stgsvc::msgs::text(system, "TXT0B19", &[]);
            functions(system, &pattern).map(|list| {
                let fns = list
                    .iter()
                    .map(|(c, _)| {
                        let f = c.function;
                        vec![
                            ("FCNID".to_string(), f.id.to_string()),
                            ("FCNNAME".to_string(), f.name.to_string()),
                            ("FCNDESC".to_string(), f.desc.to_string()),
                            ("FCNPROD".to_string(), f.product.to_string()),
                            ("FCNGRP".to_string(), f.group.to_string()),
                            ("FCNDFT".to_string(), usage(c.default_allowed).to_string()),
                            (
                                "FCNALLOBJ".to_string(),
                                if c.allobj_used { "*USED" } else { "*NOTUSED" }.to_string(),
                            ),
                        ]
                    })
                    .collect();
                let users = list
                    .iter()
                    .flat_map(|(c, groups)| {
                        let (user, group) = (&user, &group);
                        c.users.iter().zip(groups).map(move |((u, a), g)| {
                            vec![
                                ("FCNID".to_string(), c.function.id.to_string()),
                                ("USER".to_string(), u.clone()),
                                (
                                    "TYPE".to_string(),
                                    if *g { group.clone() } else { user.clone() },
                                ),
                                ("USAGE".to_string(), usage(*a).to_string()),
                            ]
                        })
                    })
                    .collect();
                (
                    Vec::new(),
                    vec![("FUNCTIONS".to_string(), fns), ("USERS".to_string(), users)],
                )
            })
        }
        _ => return None,
    })
}
