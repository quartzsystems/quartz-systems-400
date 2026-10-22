// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CRTUSRPRF`, `CHGUSRPRF`, `CHGPWD`, `CHKPWD`, `RTVUSRPRF`,
//! `DSPUSRPRF TYPE(*BASIC)`, `DSPAUTUSR` and the list `WRKUSRPRF` shows.

use std::fmt::Write as _;

use store::obj::header::Authority;
use store::txn::Txn;

use crate::cmd::{Env, Keywords, bad_value, civil, number, one_of, parse_date, qualified};
use crate::password::policy::{self, By, Policy, Validator};
use crate::password::{form_for, verify};
use crate::profile::payload::{LmtCpb, NOMAX, SYSVAL, Tri, UsrPrf};
use crate::profile::spcaut::{SpecialAuth, UserClass};
use crate::profile::{groups, objects};
use crate::{Result, SecError};

/// The password policy from the environment's system values.
#[must_use]
pub fn policy(env: &Env<'_>) -> Policy {
    Policy::from_sysvals(env.sysval)
}

fn yes_no(kw: &str, v: &str) -> Result<bool> {
    Ok(one_of(kw, v, &["*YES", "*NO"])? == "*YES")
}

fn tri(kw: &str, v: &str) -> Result<Tri> {
    Tri::by_name(v.trim()).ok_or_else(|| bad_value(kw, v))
}

fn interval(kw: &str, v: &str, none: &str, hi: i64) -> Result<u32> {
    match v.trim().to_ascii_uppercase().as_str() {
        "*SYSVAL" => Ok(SYSVAL),
        x if x == none => Ok(NOMAX),
        x => Ok(u32::try_from(number(kw, x, 1, hi)?).expect("in range")),
    }
}

fn storage(kw: &str, v: &str) -> Result<i64> {
    if v.trim().eq_ignore_ascii_case("*NOMAX") {
        Ok(-1)
    } else {
        number(kw, v, 1, i64::MAX)
    }
}

fn name_or(kw: &str, v: &str, specials: &[&str]) -> Result<String> {
    let u = v.trim().to_ascii_uppercase();
    if specials.contains(&u.as_str()) || qsname::Name::parse(&u).is_ok() {
        Ok(u)
    } else {
        Err(bad_value(kw, v))
    }
}

fn qual_or(kw: &str, v: &str, specials: &[&str]) -> Result<(String, String)> {
    let u = v.trim().to_ascii_uppercase();
    if specials.contains(&u.as_str()) {
        return Ok((u, String::new()));
    }
    let (n, l) = qualified(&u);
    if qsname::Name::parse(&n).is_err() {
        return Err(bad_value(kw, v));
    }
    Ok((n, l))
}

/// Apply every keyword but `USRPRF`, `PASSWORD` and `AUT` to `p`.
#[allow(clippy::too_many_lines)]
fn apply(p: &mut UsrPrf, k: &Keywords, env: &Env<'_>) -> Result<()> {
    let qsecurity = env.num("QSECURITY", 40);
    for (kw, _) in &k.0 {
        let kw = kw.as_str();
        let v = k.get(kw).unwrap_or_default();
        match kw {
            "USRPRF" | "PASSWORD" | "AUT" => {}
            "PWDEXP" => p.pwdexp = yes_no(kw, &v)?,
            "STATUS" => p.disabled = one_of(kw, &v, &["*ENABLED", "*DISABLED"])? == "*DISABLED",
            "USRCLS" => {
                p.usrcls = UserClass::by_name(v.trim()).ok_or_else(|| bad_value(kw, &v))?;
                if !k.has("SPCAUT") {
                    p.spcaut = p.usrcls.defaults(qsecurity);
                }
            }
            "SPCAUT" => {
                let names = k.list(kw).unwrap_or_default();
                p.spcaut = if names == ["*USRCLS"] {
                    let class = match k.upper("USRCLS") {
                        Some(c) => UserClass::by_name(&c).ok_or_else(|| bad_value("USRCLS", &c))?,
                        None => p.usrcls,
                    };
                    class.defaults(qsecurity)
                } else {
                    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
                    SpecialAuth::parse(&refs).map_err(|_| bad_value(kw, &v))?
                };
            }
            "ASTLVL" => {
                p.astlvl = one_of(kw, &v, &["*SYSVAL", "*BASIC", "*INTERMED", "*ADVANCED"])?;
            }
            "CURLIB" => p.curlib = name_or(kw, &v, &["*CRTDFT"])?,
            "INLPGM" => p.inlpgm = qual_or(kw, &v, &["*NONE"])?,
            "INLMNU" => p.inlmnu = qual_or(kw, &v, &["*SIGNOFF"])?,
            "LMTCPB" => p.lmtcpb = LmtCpb::by_name(v.trim()).ok_or_else(|| bad_value(kw, &v))?,
            "TEXT" => {
                p.text = if v.eq_ignore_ascii_case("*BLANK") {
                    String::new()
                } else {
                    v.chars().take(50).collect()
                }
            }
            "SPCENV" => p.spcenv = one_of(kw, &v, &["*SYSVAL", "*NONE", "*S36"])?,
            "DSPSGNINF" => p.dspsgninf = tri(kw, &v)?,
            "PWDEXPITV" => p.pwdexpitv = interval(kw, &v, "*NOMAX", 366)?,
            "PWDCHGBLK" => p.pwdchgblk = interval(kw, &v, "*NONE", 99)?,
            "LCLPWDMGT" => p.lclpwdmgt = yes_no(kw, &v)?,
            "LMTDEVSSN" => p.lmtdevssn = tri(kw, &v)?,
            "KBDBUF" => p.kbdbuf = one_of(kw, &v, &["*SYSVAL", "*NO", "*TYPEAHEAD", "*YES"])?,
            "MAXSTG" => p.maxstg = storage(kw, &v)?,
            "MAXSTGLRG" => p.maxstglrg = storage(kw, &v)?,
            "PTYLMT" => p.ptylmt = u8::try_from(number(kw, &v, 0, 9)?).expect("0-9"),
            "JOBD" => p.jobd = qual_or(kw, &v, &[])?,
            "GRPPRF" => p.grpprf = name_or(kw, &v, &["*NONE"])?,
            "OWNER" => p.owner_grpprf = one_of(kw, &v, &["*USRPRF", "*GRPPRF"])? == "*GRPPRF",
            "GRPAUT" => {
                p.grpaut = one_of(kw, &v, &["*NONE", "*ALL", "*CHANGE", "*USE", "*EXCLUDE"])?;
            }
            "GRPAUTTYP" => p.grpauttyp_pgp = one_of(kw, &v, &["*PRIVATE", "*PGP"])? == "*PGP",
            "SUPGRPPRF" => {
                let l = k.list(kw).unwrap_or_default();
                p.supgrpprf = if l == ["*NONE"] {
                    Vec::new()
                } else {
                    l.iter()
                        .map(|g| name_or(kw, g, &[]))
                        .collect::<Result<_>>()?
                };
            }
            "ACGCDE" => {
                p.acgcde = if v.eq_ignore_ascii_case("*BLANK") {
                    String::new()
                } else {
                    v.chars().take(15).collect()
                }
            }
            "DOCPWD" => p.docpwd = v.trim().to_ascii_uppercase(),
            "MSGQ" => p.msgq = qual_or(kw, &v, &["*USRPRF"])?,
            "DLVRY" => p.dlvry = one_of(kw, &v, &["*NOTIFY", "*BREAK", "*HOLD", "*DFT"])?,
            "SEV" => p.sev = u8::try_from(number(kw, &v, 0, 99)?).expect("0-99"),
            "PRTDEV" => p.prtdev = name_or(kw, &v, &["*WRKSTN", "*SYSVAL"])?,
            "OUTQ" => p.outq = qual_or(kw, &v, &["*WRKSTN", "*DEV"])?,
            "ATNPGM" => p.atnpgm = qual_or(kw, &v, &["*SYSVAL", "*NONE", "*ASSIST"])?,
            "SRTSEQ" => {
                p.srtseq = qual_or(kw, &v, &["*SYSVAL", "*HEX", "*LANGIDUNQ", "*LANGIDSHR"])?;
            }
            "LANGID" => p.langid = v.trim().to_ascii_uppercase(),
            "CNTRYID" => p.cntryid = v.trim().to_ascii_uppercase(),
            "CCSID" => {
                p.ccsid = if v.trim().eq_ignore_ascii_case("*SYSVAL") {
                    0
                } else {
                    u32::try_from(number(kw, &v, 1, 65535)?).expect("range")
                };
            }
            "CHRIDCTL" => p.chridctl = one_of(kw, &v, &["*SYSVAL", "*DEVD", "*JOBCCSID"])?,
            "SETJOBATR" => p.setjobatr = k.list(kw).unwrap_or_default(),
            "LOCALE" => p.locale = v.trim().to_string(),
            "USROPT" => p.usropt = k.list(kw).unwrap_or_default(),
            "UID" => {
                p.uid = if v.trim().eq_ignore_ascii_case("*GEN") {
                    0
                } else {
                    u32::try_from(number(kw, &v, 1, 4_294_967_294)?).expect("range")
                };
            }
            "GID" => {
                p.gid = match v.trim().to_ascii_uppercase().as_str() {
                    "*NONE" => 0,
                    "*GEN" => u32::MAX,
                    x => u32::try_from(number(kw, x, 1, 4_294_967_294)?).expect("range"),
                };
            }
            "HOMEDIR" => p.homedir = v.trim().to_string(),
            "EIMASSOC" => {
                p.eimassoc = v
                    .split_whitespace()
                    .next()
                    .unwrap_or("*NOCHG")
                    .to_ascii_uppercase();
            }
            "USREXPITV" => p.usrexpitv = u32::try_from(number(kw, &v, 1, 366)?).expect("range"),
            "USREXPDATE" => {
                p.usrexpdate = match v.trim().to_ascii_uppercase().as_str() {
                    "*NONE" => 0,
                    "*USREXPITV" => env.now / 86_400 * 86_400 + u64::from(p.usrexpitv) * 86_400,
                    d => parse_date(kw, d)?,
                };
            }
            "AUTHMTH" => p.authmth = v.trim().to_ascii_uppercase(),
            _ => {
                return Err(SecError::msg(
                    "CPD0043",
                    format!("Keyword {kw} is not valid for this command."),
                ));
            }
        }
    }
    Ok(())
}

/// The special authorities `p` gets that the actor lacks.
fn beyond(env: &Env<'_>, p: &UsrPrf, before: SpecialAuth) -> SpecialAuth {
    let added = p.spcaut.without(before);
    added.without(env.actor.special_adopted())
}

/// Resolve `GID(*GEN)` and `UID(*GEN)`.
fn numbers(txn: &mut Txn<'_>, env: &Env<'_>, p: &mut UsrPrf) -> Result<()> {
    if p.uid == 0 {
        p.uid = groups::next_id(txn, env.qsys, false)?;
    }
    if p.gid == u32::MAX {
        p.gid = groups::next_id(txn, env.qsys, true)?;
    }
    Ok(())
}

/// The `PASSWORD` keyword applied, administrator style.
fn password(p: &mut UsrPrf, k: &Keywords, env: &Env<'_>) -> Result<()> {
    let Some(v) = k.get("PASSWORD") else {
        return Ok(());
    };
    let pol = policy(env);
    let mut no_program = |_: &UsrPrf, _: &str, _: Option<&str>| Ok(());
    match v.trim().to_ascii_uppercase().as_str() {
        "*NONE" => {
            policy::set_none(p);
            Ok(())
        }
        "*SAME" => Ok(()),
        "*USRPRF" if pol.pwdlvl <= 1 => {
            let name = p.name.clone();
            policy::set(p, &name, By::Admin, &pol, env.now, &mut no_program)
        }
        _ => policy::set(p, &v, By::Admin, &pol, env.now, &mut no_program),
    }
}

/// `AUT`: the public authority to the profile object.
fn public(k: &Keywords) -> Result<Authority> {
    match k.upper("AUT").as_deref().unwrap_or("*EXCLUDE") {
        "*EXCLUDE" => Ok(Authority::EXCLUDE),
        "*USE" => Ok(Authority::USE),
        "*CHANGE" => Ok(Authority::CHANGE),
        "*ALL" => Ok(Authority::ALL),
        v => Err(bad_value("AUT", v)),
    }
}

/// The `CP` entry for a profile command.
fn cp_event(cmd: &str, p: &UsrPrf, password: bool) -> crate::audit::Event {
    crate::audit::Event::new(crate::audit::Kind::Cp)
        .set("CMDN", cmd)
        .set("USRP", p.name.as_str())
        .set("PWDC", if password { "Y" } else { "N" })
        .set("PWDN", if p.pwd_none { "Y" } else { "N" })
        .set("PWDX", if p.pwdexp { "Y" } else { "N" })
        .set("STAT", if p.disabled { "*DISABLED" } else { "*ENABLED" })
        .set("CLAS", p.usrcls.name())
        .set(
            "SPCA",
            p.spcaut.names().iter().fold(String::new(), |mut acc, n| {
                let _ = write!(acc, "{n:<10}");
                acc
            }),
        )
        .set("GRPP", p.grpprf.as_str())
        .set("OWNR", if p.owner_grpprf { "*GRPPRF" } else { "*USRPRF" })
        .set("GAUT", p.grpaut.as_str())
}

/// `CRTUSRPRF`.
pub fn crtusrprf(txn: &mut Txn<'_>, env: &Env<'_>, k: &Keywords) -> Result<String> {
    env.actor
        .require(SpecialAuth::SECADM, "create user profiles")?;
    let name = k
        .upper("USRPRF")
        .ok_or_else(|| SecError::msg("CPD0071", "USRPRF is required."))?;
    if qsname::Name::parse(&name).is_err() {
        return Err(SecError::msg(
            "CPF2203",
            format!("{name} is not a valid user profile name."),
        ));
    }
    let qsecurity = env.num("QSECURITY", 40);
    let mut p = UsrPrf::new(&name, qsecurity);
    p.created = env.now;
    p.changed = env.now;
    apply(&mut p, k, env)?;
    let extra = beyond(env, &p, SpecialAuth::NONE);
    if extra != SpecialAuth::NONE {
        return Err(SecError::msg(
            "CPF2222",
            format!(
                "You cannot give special authorities you do not have: {}.",
                extra.names().join(" ")
            ),
        ));
    }
    groups::validate(txn, env.qsys, &p)?;
    numbers(txn, env, &mut p)?;
    password(&mut p, k, env)?;
    objects::create(txn, env.qsys, &p, &env.actor.user, public(k)?, env.now)?;
    groups::ensure_gids(txn, env.qsys, &p)?;
    crate::audit::record(cp_event("CRTUSRPRF", &p, k.has("PASSWORD")));
    Ok(format!("User profile {name} created."))
}

/// `CHGUSRPRF`.
pub fn chgusrprf(txn: &mut Txn<'_>, env: &Env<'_>, k: &Keywords) -> Result<String> {
    env.actor
        .require(SpecialAuth::SECADM, "change user profiles")?;
    let name = k
        .upper("USRPRF")
        .ok_or_else(|| SecError::msg("CPD0071", "USRPRF is required."))?;
    let (id, mut p) = objects::read(txn, env.qsys, &name)?;
    let before = p.spcaut;
    apply(&mut p, k, env)?;
    let extra = beyond(env, &p, before);
    if extra != SpecialAuth::NONE {
        return Err(SecError::msg(
            "CPF2222",
            format!(
                "You cannot give special authorities you do not have: {}.",
                extra.names().join(" ")
            ),
        ));
    }
    groups::validate(txn, env.qsys, &p)?;
    numbers(txn, env, &mut p)?;
    password(&mut p, k, env)?;
    if k.has("STATUS") && !p.disabled {
        p.signon_invalid = 0;
    }
    p.changed = env.now;
    objects::write(txn, id, &p)?;
    groups::ensure_gids(txn, env.qsys, &p)?;
    crate::auth::cache::bump_epoch();
    crate::audit::record(cp_event("CHGUSRPRF", &p, k.has("PASSWORD")));
    Ok(format!("User profile {name} changed."))
}

/// `CHGPWD`: the actor's own password.
pub fn chgpwd(
    txn: &mut Txn<'_>,
    env: &Env<'_>,
    current: &str,
    new: &str,
    validate: &mut Validator<'_>,
) -> Result<String> {
    let (id, mut p) = objects::read(txn, env.qsys, &env.actor.user)?;
    if !p.lclpwdmgt {
        return Err(SecError::msg(
            "CPF22C7",
            "The password of this profile is not managed here (LCLPWDMGT(*NO)).",
        ));
    }
    policy::set(
        &mut p,
        new,
        By::User { current },
        &policy(env),
        env.now,
        validate,
    )?;
    p.changed = env.now;
    objects::write(txn, id, &p)?;
    crate::audit::record(cp_event("CHGPWD", &p, true));
    Ok("Password changed successfully.".into())
}

/// `CHKPWD`: whether `password` is the actor's; a wrong one counts as
/// a sign-on attempt not valid.
pub fn chkpwd(txn: &mut Txn<'_>, env: &Env<'_>, password: &str) -> Result<String> {
    let (id, mut p) = objects::read(txn, env.qsys, &env.actor.user)?;
    let lvl = env.num("QPWDLVL", 3);
    if !p.pwd_none && p.hash(form_for(lvl)).is_some_and(|h| verify(h, password)) {
        return Ok("Password is correct.".into());
    }
    p.signon_invalid += 1;
    objects::write(txn, id, &p)?;
    Err(SecError::msg("CPF2362", "The password is not correct."))
}

/// `RTVUSRPRF`: the profile, for the caller to pick fields from.
pub fn rtvusrprf(txn: &mut Txn<'_>, env: &Env<'_>, name: &str) -> Result<UsrPrf> {
    let name = if name.eq_ignore_ascii_case("*CURRENT") {
        env.actor.user.clone()
    } else {
        name.to_ascii_uppercase()
    };
    let (id, p) = objects::read(txn, env.qsys, &name)?;
    if name != env.actor.user {
        store::obj::guard::check_id(txn, "usrprf.display", id)?;
    }
    Ok(p)
}

fn interval_text(v: u32, none: &str) -> String {
    match v {
        SYSVAL => "*SYSVAL".into(),
        NOMAX => none.into(),
        n => n.to_string(),
    }
}

fn qual_text((n, l): &(String, String)) -> String {
    if l.is_empty() || n.starts_with('*') {
        n.clone()
    } else {
        format!("{l}/{n}")
    }
}

fn when(t: u64) -> String {
    if t == 0 {
        String::new()
    } else {
        let s = t % 86_400;
        format!(
            "{} {:02}:{:02}:{:02}",
            civil(t),
            s / 3600,
            s / 60 % 60,
            s % 60
        )
    }
}

/// `DSPUSRPRF TYPE(*BASIC)` as text.
#[allow(clippy::too_many_lines)]
pub fn display_basic(p: &UsrPrf, pol: &Policy, now: u64) -> String {
    let mut o = String::new();
    let mut line = |label: &str, v: &str| {
        let dots: String = format!("{label} ")
            .chars()
            .chain(std::iter::repeat_n('.', 36))
            .take(36)
            .collect();
        let _ = writeln!(o, " {dots}:   {v}");
    };
    line("User profile", &p.name);
    line("Previous sign-on", &when(p.prev_signon));
    line("Sign-on attempts not valid", &p.signon_invalid.to_string());
    line("Status", if p.disabled { "*DISABLED" } else { "*ENABLED" });
    line(
        "Date password last changed",
        &if p.pwd_changed == 0 {
            String::new()
        } else {
            civil(p.pwd_changed)
        },
    );
    line("Password is *NONE", if p.pwd_none { "*YES" } else { "*NO" });
    line(
        "Password expiration interval",
        &interval_text(p.pwdexpitv, "*NOMAX"),
    );
    let left = pol
        .days_left(p, now)
        .map_or_else(|| "*NOMAX".to_string(), |d| d.max(0).to_string());
    line("Days until password expires", &left);
    line(
        "Set password to expired",
        if p.pwdexp { "*YES" } else { "*NO" },
    );
    line(
        "Block password change",
        &interval_text(p.pwdchgblk, "*NONE"),
    );
    line("User class", p.usrcls.name());
    line("Special authority", &p.spcaut.names().join(" "));
    line("Group profile", &p.grpprf);
    line(
        "Supplemental groups",
        &if p.supgrpprf.is_empty() {
            "*NONE".into()
        } else {
            p.supgrpprf.join(" ")
        },
    );
    line("Owner", if p.owner_grpprf { "*GRPPRF" } else { "*USRPRF" });
    line("Group authority", &p.grpaut);
    line(
        "Group authority type",
        if p.grpauttyp_pgp { "*PGP" } else { "*PRIVATE" },
    );
    line("Assistance level", &p.astlvl);
    line("Current library", &p.curlib);
    line("Initial program to call", &qual_text(&p.inlpgm));
    line("Initial menu", &qual_text(&p.inlmnu));
    line("Limit capabilities", p.lmtcpb.name());
    line("Text", &p.text);
    line("Special environment", &p.spcenv);
    line("Display sign-on information", p.dspsgninf.name());
    line(
        "Local password management",
        if p.lclpwdmgt { "*YES" } else { "*NO" },
    );
    line("Limit device sessions", p.lmtdevssn.name());
    line("Keyboard buffering", &p.kbdbuf);
    line(
        "Maximum allowed storage",
        &if p.maxstg < 0 {
            "*NOMAX".into()
        } else {
            p.maxstg.to_string()
        },
    );
    line("Storage used", &p.stg_used.to_string());
    line("Highest scheduling priority", &p.ptylmt.to_string());
    line("Job description", &qual_text(&p.jobd));
    line("Accounting code", &p.acgcde);
    line("Message queue", &qual_text(&p.msgq));
    line("Message queue delivery", &p.dlvry);
    line("Message queue severity", &p.sev.to_string());
    line("Output queue", &qual_text(&p.outq));
    line("Printer device", &p.prtdev);
    line("Attention program", &qual_text(&p.atnpgm));
    line("Sort sequence", &qual_text(&p.srtseq));
    line("Language identifier", &p.langid);
    line("Country or region identifier", &p.cntryid);
    line(
        "Coded character set ID",
        &if p.ccsid == 0 {
            "*SYSVAL".into()
        } else {
            p.ccsid.to_string()
        },
    );
    line("Character identifier control", &p.chridctl);
    line("Job attributes", &p.setjobatr.join(" "));
    line("Locale", &p.locale);
    line("User options", &p.usropt.join(" "));
    line("User ID number", &p.uid.to_string());
    line(
        "Group ID number",
        &if p.gid == 0 {
            "*NONE".into()
        } else {
            p.gid.to_string()
        },
    );
    line("Home directory", &p.homedir);
    line(
        "User expiration date",
        &if p.usrexpdate == 0 {
            "*NONE".into()
        } else {
            civil(p.usrexpdate)
        },
    );
    line("Object auditing value", &p.objaud);
    line("Action auditing values", &p.audlvl.join(" "));
    line("Objects owned", &p.owned_count.to_string());
    o
}

/// `DSPUSRPRF TYPE(*GRPMBR)`.
pub fn display_members(txn: &mut Txn<'_>, env: &Env<'_>, group: &str) -> Result<String> {
    let members = groups::members(txn, env.qsys, group)?;
    let mut o = format!(" Group profile {group}\n User        Text\n");
    for m in members {
        let (_, p) = objects::read(txn, env.qsys, &m)?;
        let _ = writeln!(o, " {m:<10}  {}", p.text);
    }
    Ok(o)
}

/// `DSPAUTUSR SEQ(*USRPRF|*GRPPRF)`: profiles, groups, whether each has
/// a password, when it last changed.
pub fn dspautusr(txn: &mut Txn<'_>, env: &Env<'_>, by_group: bool) -> Result<String> {
    env.actor
        .require(SpecialAuth::SECADM, "list authorized users")?;
    let mut rows = Vec::new();
    for name in objects::list(txn, env.qsys)? {
        let (_, p) = objects::read(txn, env.qsys, &name)?;
        rows.push((p.grpprf.clone(), p));
    }
    if by_group {
        rows.sort_by(|a, b| {
            (a.0.as_str(), a.1.name.as_str()).cmp(&(b.0.as_str(), b.1.name.as_str()))
        });
    }
    let mut o =
        String::from(" User        Group       Password  Last changed  No password  Text\n");
    for (g, p) in rows {
        let _ = writeln!(
            o,
            " {:<10}  {:<10}  {:<8}  {:<12}  {:<11}  {}",
            p.name,
            g,
            if p.pwd_none { "" } else { "*YES" },
            if p.pwd_changed == 0 {
                String::new()
            } else {
                civil(p.pwd_changed)
            },
            if p.pwd_none { "*YES" } else { "" },
            p.text
        );
    }
    Ok(o)
}

/// What `WRKUSRPRF` lists: name, class, text.
pub fn list(txn: &mut Txn<'_>, env: &Env<'_>) -> Result<Vec<(String, UserClass, String)>> {
    let mut out = Vec::new();
    for name in objects::list(txn, env.qsys)? {
        let (_, p) = objects::read(txn, env.qsys, &name)?;
        out.push((p.name, p.usrcls, p.text));
    }
    Ok(out)
}

/// `PRTUSRPRF TYPE(*AUTINFO|*ENVINFO|*PWDINFO)`: one line per profile
/// for the spooled report; `*ALLOBJ` is needed.
pub fn prtusrprf(txn: &mut Txn<'_>, env: &Env<'_>, kind: &str) -> Result<Vec<String>> {
    env.actor.require(SpecialAuth::ALLOBJ, "print user profiles")?;
    let mut lines = vec![match kind {
        "*ENVINFO" => format!(
            "{:<10} {:<10} {:<21} {:<21} {:<21} {:<21} {:<21} {}",
            "Profile", "Curlib", "Initial program", "Initial menu", "Job description",
            "Message queue", "Output queue", "Printer"
        ),
        "*PWDINFO" => format!(
            "{:<10} {:<8} {:<10} {:>7} {:<10} {}",
            "Profile", "Password", "Changed", "Expitv", "Status", "Last sign-on"
        ),
        _ => format!(
            "{:<10} {:<8} {:<10} {:<8} {:<10} {}",
            "Profile", "Class", "Group", "Owner", "Status", "Special authorities"
        ),
    }];
    let qualified = |(n, l): &(String, String)| {
        if n.starts_with('*') || n.is_empty() {
            n.clone()
        } else {
            format!("{l}/{n}")
        }
    };
    for name in objects::list(txn, env.qsys)? {
        let (_, p) = objects::read(txn, env.qsys, &name)?;
        let status = if p.disabled { "*DISABLED" } else { "*ENABLED" };
        lines.push(match kind {
            "*ENVINFO" => format!(
                "{:<10} {:<10} {:<21} {:<21} {:<21} {:<21} {:<21} {}",
                p.name,
                p.curlib,
                qualified(&p.inlpgm),
                qualified(&p.inlmnu),
                qualified(&p.jobd),
                qualified(&p.msgq),
                qualified(&p.outq),
                p.prtdev
            ),
            "*PWDINFO" => format!(
                "{:<10} {:<8} {:<10} {:>7} {:<10} {}",
                p.name,
                if p.pwd_none { "*NONE" } else { "*YES" },
                if p.pwd_changed == 0 { "*NONE".to_string() } else { civil(p.pwd_changed) },
                if p.pwdexpitv == 0 { "*SYSVAL".to_string() } else { p.pwdexpitv.to_string() },
                status,
                if p.last_signon == 0 { "*NONE".to_string() } else { civil(p.last_signon) }
            ),
            _ => format!(
                "{:<10} {:<8} {:<10} {:<8} {:<10} {}",
                p.name,
                p.usrcls.name(),
                if p.grpprf.is_empty() { "*NONE" } else { &p.grpprf },
                if p.owner_grpprf { "*GRPPRF" } else { "*USRPRF" },
                status,
                p.spcaut.names().join(" ")
            ),
        });
    }
    Ok(lines)
}

/// `ANZPRFACT DAYS(n)`: disable every profile that has not signed on
/// for `days` (never signed on: since it was created), except the
/// system's `Q*` profiles and the caller; `*SECADM` is needed.
pub fn anzprfact(txn: &mut Txn<'_>, env: &Env<'_>, days: u32) -> Result<String> {
    env.actor.require(SpecialAuth::SECADM, "analyze profile activity")?;
    let limit = u64::from(days) * 86_400;
    let mut disabled = Vec::new();
    for name in objects::list(txn, env.qsys)? {
        if name.starts_with('Q') || name == env.actor.user {
            continue;
        }
        let (id, mut p) = objects::read(txn, env.qsys, &name)?;
        let last = if p.last_signon == 0 { p.created } else { p.last_signon };
        if p.disabled || env.now < last.saturating_add(limit) {
            continue;
        }
        p.disabled = true;
        p.changed = env.now;
        objects::write(txn, id, &p)?;
        disabled.push(name);
    }
    Ok(if disabled.is_empty() {
        format!("No profile inactive for {days} days.")
    } else {
        format!(
            "{} profile(s) inactive for {days} days disabled: {}.",
            disabled.len(),
            disabled.join(" ")
        )
    })
}
