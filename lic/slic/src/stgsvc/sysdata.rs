// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Panel data for the system and security displays (requests 180 to
//! 199): system values, network attributes, user profiles, the users
//! `DSPAUTUSR` lists. Each answers what the job's panel driver shows,
//! from the library functions the text commands use.

use sec::profile::objects;
use sec::profile::payload::{NOMAX, SYSVAL, UsrPrf};
use store::obj::header::text_of;

use crate::ipl::system::{System, now_unix};
use crate::svc::{neta, security};

/// `MM/DD/YY` of Unix seconds, blank for 0.
fn mdy(t: u64) -> String {
    if t == 0 {
        return String::new();
    }
    let c = sec::cmd::civil(t);
    // `YYYY-MM-DD`.
    format!("{}/{}/{}", &c[5..7], &c[8..10], &c[2..4])
}

/// `MM/DD/YY  HH:MM:SS` of Unix seconds, blank for 0.
fn mdy_hms(t: u64) -> String {
    if t == 0 {
        return String::new();
    }
    let s = t % 86_400;
    format!(
        "{}  {:02}:{:02}:{:02}",
        mdy(t),
        s / 3600,
        s / 60 % 60,
        s % 60
    )
}

/// A system value: its text, its type (`*SEC` ...) and the value's
/// elements as shown.
pub fn sysval(system: &System, name: &str) -> Option<(String, String, Vec<String>)> {
    let vals = system.sysvals.as_ref()?;
    let name = name.trim().to_ascii_uppercase();
    let e = vals.catalog().get(&name)?;
    let v = vals.get(&name).ok()?;
    let elems = match v {
        sysval::Value::List(items) => items,
        other => vec![other.text()],
    };
    Some((e.text.clone(), group_name(e.group), elems))
}

/// Every system value: name, type and text, by name.
pub fn sysvals(system: &System) -> Vec<(String, String, String)> {
    let Some(vals) = system.sysvals.as_ref() else {
        return Vec::new();
    };
    let mut out: Vec<(String, String, String)> = vals
        .catalog()
        .entries()
        .map(|e| (e.name.clone(), group_name(e.group), e.text.clone()))
        .collect();
    out.sort();
    out
}

fn group_name(g: sysval::Group) -> String {
    format!("*{g:?}").to_ascii_uppercase()
}

/// The network attributes, by keyword.
pub fn neta(system: &mut System) -> Result<Vec<(String, String)>, String> {
    let sysname = system.system_name();
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    let map = crate::svc::db::with_txn(&mut s.store, |txn| neta::read(txn, qsys, &sysname))?;
    Ok(map.into_iter().collect())
}

fn interval(v: u32, none: &str) -> String {
    match v {
        SYSVAL => "*SYSVAL".into(),
        NOMAX => none.into(),
        n => n.to_string(),
    }
}

fn qual(n: &str, l: &str) -> (String, String) {
    if n.starts_with('*') {
        (n.to_string(), String::new())
    } else {
        (n.to_string(), l.to_string())
    }
}

fn yes(b: bool) -> &'static str {
    if b { "*YES" } else { "*NO" }
}

fn or_none(list: &[String]) -> Vec<String> {
    if list.is_empty() {
        vec!["*NONE".into()]
    } else {
        list.to_vec()
    }
}

/// `DSPUSRPRF TYPE(*BASIC)`: the panel's variables (`QUIUSRPRF`).
pub fn usrprf(system: &mut System, name: &str) -> Result<Vec<(String, String)>, String> {
    let now = now_unix();
    security::in_store(system, |txn, env| {
        let p = sec::cmd::usrprf::rtvusrprf(txn, env, name)?;
        let (id, _) = objects::read(txn, env.qsys, &p.name)?;
        let h = sec::auth::grant::header(txn, id)?;
        let pol = sec::cmd::usrprf::policy(env);
        Ok(usrprf_vars(&p, &h, &pol, now))
    })
}

#[allow(clippy::too_many_lines)] // one line per variable
fn usrprf_vars(
    p: &UsrPrf,
    h: &store::obj::header::ObjectHeader,
    pol: &sec::password::policy::Policy,
    now: u64,
) -> Vec<(String, String)> {
    let mut vars: Vec<(String, String)> = Vec::new();
    let mut set = |k: &str, val: &str| vars.push((k.to_string(), val.to_string()));
    set("UPNAME", &p.name);
    set("UPPRVSGN", &mdy_hms(p.prev_signon));
    set("UPSGNINV", &p.signon_invalid.to_string());
    set("UPSTATUS", if p.disabled { "*DISABLED" } else { "*ENABLED" });
    set("UPPWDCHG", &mdy_hms(p.pwd_changed));
    set("UPPWDNONE", yes(p.pwd_none));
    set("UPPWDITV", &interval(p.pwdexpitv, "*NOMAX"));
    let expires = pol.days_left(p, now).map_or_else(
        || "*NONE".to_string(),
        |d| {
            let at = i64::try_from(now).unwrap_or(0) + d * 86_400;
            mdy(u64::try_from(at.max(1)).unwrap_or(1))
        },
    );
    set("UPPWDEXPD", &expires);
    set("UPPWDEXP", yes(p.pwdexp));
    set("UPPWDBLK", &interval(p.pwdchgblk, "*NONE"));
    set("UPLCLPWD", yes(p.lclpwdmgt));
    set("UPMAXSGN", "*SYSVAL");
    set("UPUSRCLS", p.usrcls.name());
    set("UPCRTDT", &mdy_hms(h.created));
    set("UPCRTBY", &text_of(&h.creator));
    set("UPCHGDT", &mdy_hms(h.changed.max(p.changed)));
    set("UPLASTUSE", &mdy(h.last_used.max(p.last_signon)));
    set("UPRSTDT", &mdy_hms(h.last_restored));
    set(
        "UPEXPDT",
        &if p.usrexpdate == 0 {
            "*NONE".into()
        } else {
            mdy(p.usrexpdate)
        },
    );
    set(
        "UPEXPITV",
        &if p.usrexpitv == 0 {
            "*NONE".into()
        } else {
            p.usrexpitv.to_string()
        },
    );
    set("UPEXPACN", "*NONE");
    let spc = or_none(&p.spcaut.names().iter().map(|s| (*s).to_string()).collect::<Vec<_>>());
    for (i, s) in spc.iter().take(8).enumerate() {
        let k = if i == 0 { "UPSPCAUT".to_string() } else { format!("UPSPC{}", i + 1) };
        set(&k, s);
    }
    set("UPGRPPRF", &p.grpprf);
    set("UPOWNER", if p.owner_grpprf { "*GRPPRF" } else { "*USRPRF" });
    set("UPGRPAUT", &p.grpaut);
    set("UPGRPTYP", if p.grpauttyp_pgp { "*PGP" } else { "*PRIVATE" });
    for (i, g) in or_none(&p.supgrpprf).iter().take(15).enumerate() {
        let k = if i == 0 { "UPSUPGRP".to_string() } else { format!("UPSUP{}", i + 1) };
        set(&k, g);
    }
    set("UPASTLVL", &p.astlvl);
    set("UPCURLIB", &p.curlib);
    for (var, lib_var, (obj, lib)) in [
        ("UPINLPGM", "UPINLPGML", &p.inlpgm),
        ("UPINLMNU", "UPINLMNUL", &p.inlmnu),
        ("UPJOBD", "UPJOBDL", &p.jobd),
        ("UPMSGQ", "UPMSGQL", &p.msgq),
        ("UPOUTQ", "UPOUTQL", &p.outq),
        ("UPATNPGM", "UPATNPGML", &p.atnpgm),
        ("UPSRTSEQ", "UPSRTSEQL", &p.srtseq),
    ] {
        let (obj, lib) = qual(obj, lib);
        set(var, &obj);
        set(lib_var, &lib);
    }
    set("UPLMTCPB", p.lmtcpb.name());
    let text: Vec<char> = p.text.trim_end().chars().collect();
    let (first, rest) = text.split_at(text.len().min(31));
    set("UPTEXT", &first.iter().collect::<String>());
    set("UPTEXT2", &rest.iter().collect::<String>());
    set("UPSPCENV", &p.spcenv);
    set("UPDSPSGN", p.dspsgninf.name());
    set("UPLMTDEV", p.lmtdevssn.name());
    set("UPKBDBUF", &p.kbdbuf);
    set(
        "UPMAXSTG",
        &if p.maxstg < 0 {
            "*NOMAX".into()
        } else {
            p.maxstg.to_string()
        },
    );
    set("UPSTGUSD", &p.stg_used.to_string());
    set("UPPTYLMT", &p.ptylmt.to_string());
    set("UPACGCDE", &p.acgcde);
    set("UPDLVRY", &p.dlvry);
    set("UPSEV", &p.sev.to_string());
    set("UPPRTDEV", &p.prtdev);
    set("UPLANGID", &p.langid);
    set("UPCNTRY", &p.cntryid);
    set(
        "UPCCSID",
        &if p.ccsid == 0 {
            "*SYSVAL".into()
        } else {
            p.ccsid.to_string()
        },
    );
    set("UPCHRID", &p.chridctl);
    set("UPJOBATR", &or_none(&p.setjobatr).join(" "));
    set("UPLOCALE", &p.locale);
    set("UPUSROPT", &or_none(&p.usropt).join(" "));
    set("UPUID", &p.uid.to_string());
    set(
        "UPGID",
        &if p.gid == 0 {
            "*NONE".into()
        } else {
            p.gid.to_string()
        },
    );
    set("UPHOMDIR", &p.homedir);
    set("UPOBJAUD", &p.objaud);
    set("UPAUDLVL", &or_none(&p.audlvl).join(" "));
    vars
}

/// `WRKUSRPRF`: name, class and text of every profile.
pub fn usrprfs(system: &mut System) -> Result<Vec<(String, String, String)>, String> {
    security::in_store(system, |txn, env| {
        Ok(sec::cmd::usrprf::list(txn, env)?
            .into_iter()
            .map(|(n, c, t)| (n, c.name().to_string(), t))
            .collect())
    })
}

/// `DSPSFWRSC`: the Licensed Internal Code and the operating system
/// with its English language feature.
pub fn sfwrsc() -> Vec<Vec<String>> {
    let rls = release::RELEASE.to_string();
    let row = |id: &str, ftr: &str, text: &str, ty: &str, lib: &str| {
        [id, "*BASE", ftr, text, ty, lib, &rls]
            .iter()
            .map(|s| (*s).to_string())
            .collect::<Vec<String>>()
    };
    vec![
        row("5770999", "5050", "Licensed Internal Code", "*CODE", ""),
        row(release::PRODUCT_ID, "5050", release::PRODUCT_TEXT, "*CODE", "QSYS"),
        row(release::PRODUCT_ID, "2924", release::PRODUCT_TEXT, "*LNG", "QSYS"),
    ]
}

/// `DSPAUTUSR`: user, group, date the password changed, `X` for no
/// password, text; by user, or by group with `by_group`.
pub fn autusr(system: &mut System, by_group: bool) -> Result<Vec<Vec<String>>, String> {
    super::security::require(sec::profile::SpecialAuth::SECADM, "list authorized users")?;
    security::in_store(system, |txn, env| {
        let mut rows = Vec::new();
        for name in objects::list(txn, env.qsys)? {
            let (_, p) = objects::read(txn, env.qsys, &name)?;
            let group = if p.grpprf.starts_with('*') {
                String::new()
            } else {
                p.grpprf.clone()
            };
            rows.push(vec![
                p.name.clone(),
                group,
                mdy(p.pwd_changed),
                if p.pwd_none { "X".into() } else { String::new() },
                p.text.clone(),
            ]);
        }
        if by_group {
            rows.sort_by(|a, b| (&a[1], &a[0]).cmp(&(&b[1], &b[0])));
        } else {
            rows.sort();
        }
        Ok(rows)
    })
}
