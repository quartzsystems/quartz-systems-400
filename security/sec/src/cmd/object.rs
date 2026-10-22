// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The authority commands: `GRTOBJAUT`, `RVKOBJAUT`, `DSPOBJAUT`,
//! `CHGOBJOWN`, `CHGOBJPGP`, `CHKOBJ`, `GRTUSRAUT`, `WRKOBJOWN`, the
//! authorization list commands and `DSPUSRPRF TYPE(*OBJOWN|*OBJAUT|
//! *OBJPGP)`. The caller resolves `OBJ(LIB/NAME) OBJTYPE(..)` to an id.

use std::fmt::Write as _;

use store::obj::guard;
use store::obj::header::{Authority as A, ObjectHeader, text_of};
use store::obj::{ObjectId, auth, ops, types};
use store::txn::Txn;

use super::{Env, Keywords, bad_value, one_of};
use crate::auth::bits::Auth;
use crate::auth::required::Need;
use crate::auth::{autl, grant, search};
use crate::profile::index::{self, Kind};
use crate::profile::objects;
use crate::profile::spcaut::SpecialAuth;
use crate::{Result, SecError};

fn type_name(id: ObjectId) -> &'static str {
    types::type_by_code(id.type_code).map_or("*UNKNOWN", |t| t.name)
}

/// An object's library name; `QSYS` for libraries and `QSYS` itself.
pub fn library_of(txn: &mut Txn<'_>, h: &ObjectHeader) -> String {
    if h.id.type_code == types::LIB || h.id == h.library {
        "QSYS".into()
    } else {
        ops::read_header(txn, h.library).map_or_else(|_| String::new(), |(_, l)| l.name.to_string())
    }
}

fn label(txn: &mut Txn<'_>, h: &ObjectHeader) -> String {
    format!("{}/{} {}", library_of(txn, h), h.name, type_name(h.id))
}

fn none_if_empty(s: String) -> String {
    if s.is_empty() { "*NONE".into() } else { s }
}

/// The authorities the actor may give on `h`: `None` for any (owner
/// or `*ALLOBJ`), otherwise what it has itself.
fn grantable(txn: &mut Txn<'_>, env: &Env<'_>, h: &ObjectHeader) -> Result<Option<Auth>> {
    if text_of(&h.owner) == env.actor.user {
        return Ok(None);
    }
    let f = search::find(txn, env.qsys, env.actor, h)?;
    Ok(if f.all { None } else { Some(f.have) })
}

fn auth_words(k: &Keywords, kw: &str, default: &str) -> Vec<String> {
    k.list(kw).unwrap_or_else(|| vec![default.into()])
}

fn parse_auth(words: &[String], autl: bool) -> Result<Auth> {
    let refs: Vec<&str> = words.iter().map(String::as_str).collect();
    Auth::parse(&refs, autl).map_err(|e| SecError::msg("CPF2209", format!("{e}.")))
}

fn users(k: &Keywords) -> Result<Vec<String>> {
    k.list("USER")
        .filter(|v| !v.is_empty())
        .ok_or_else(|| SecError::msg("CPD0071", "USER is required."))
}

/// `GRTOBJAUT OBJ() OBJTYPE() USER() AUT() [AUTL()] [REPLACE()]`.
/// `AUT(*AUTL)` for `*PUBLIC` takes it from the list.
pub fn grtobjaut(txn: &mut Txn<'_>, env: &Env<'_>, id: ObjectId, k: &Keywords) -> Result<String> {
    let h = grant::header(txn, id)?;
    let what = label(txn, &h);
    if let Some(list) = k.upper("AUTL") {
        if text_of(&h.owner) != env.actor.user && !env.actor.has(SpecialAuth::ALLOBJ) {
            return Err(SecError::msg(
                "CPF2253",
                format!("Only the owner or a user with *ALLOBJ can secure {what} by a list."),
            ));
        }
        let list = (list != "*NONE").then_some(list);
        grant::set_autl(txn, env.qsys, id, list.as_deref())?;
        return Ok(format!(
            "Authorization list {} now secures {what}.",
            list.as_deref().unwrap_or("*NONE")
        ));
    }
    guard::check(txn, "obj.grant", &h)?;
    let words = auth_words(k, "AUT", "*CHANGE");
    let replace = one_of(
        "REPLACE",
        &k.upper("REPLACE").unwrap_or_else(|| "*NO".into()),
        &["*NO", "*YES"],
    )? == "*YES";
    let from_list = words == ["*AUTL"];
    let bits = if from_list {
        Auth::EXCLUDE
    } else {
        parse_auth(&words, id.type_code == types::AUTL)?
    };
    if let Some(have) = grantable(txn, env, &h)? {
        let missing = have.missing(bits);
        if missing != Auth::EXCLUDE {
            return Err(SecError::msg(
                "CPF2227",
                format!(
                    "You cannot grant authorities you do not have: {}.",
                    missing.names().join(" ")
                ),
            ));
        }
    }
    for u in users(k)? {
        if u == "*PUBLIC" {
            if from_list {
                grant::set_public(txn, id, None)?;
            } else {
                let cur = Auth::of(grant::header(txn, id)?.public_authority);
                let new = if replace { bits } else { cur.union(bits) };
                grant::set_public(txn, id, Some(new))?;
            }
            continue;
        }
        if from_list {
            return Err(bad_value("AUT", "*AUTL"));
        }
        let h = grant::header(txn, id)?;
        let new = match grant::private(txn, &h, &u)? {
            Some(cur) if !replace => cur.union(bits),
            _ => bits,
        };
        grant::set_private(txn, env.qsys, id, &u, Some(new))?;
    }
    Ok(format!("Authority given for {what}."))
}

/// `RVKOBJAUT OBJ() OBJTYPE() USER() AUT() [AUTL()]`. A user left with
/// no authority loses its private authority.
pub fn rvkobjaut(txn: &mut Txn<'_>, env: &Env<'_>, id: ObjectId, k: &Keywords) -> Result<String> {
    let h = grant::header(txn, id)?;
    let what = label(txn, &h);
    if let Some(list) = k.upper("AUTL") {
        if text_of(&h.owner) != env.actor.user && !env.actor.has(SpecialAuth::ALLOBJ) {
            return Err(SecError::msg(
                "CPF2253",
                format!(
                    "Only the owner or a user with *ALLOBJ can remove the list securing {what}."
                ),
            ));
        }
        if text_of(&h.autl) != list {
            return Err(SecError::msg(
                "CPF2254",
                format!("{what} is not secured by {list}."),
            ));
        }
        grant::set_autl(txn, env.qsys, id, None)?;
        return Ok(format!(
            "Authorization list {list} no longer secures {what}."
        ));
    }
    guard::check(txn, "obj.grant", &h)?;
    let words = auth_words(k, "AUT", "*CHANGE");
    let bits = parse_auth(&words, id.type_code == types::AUTL)?;
    let all = words == ["*ALL"];
    for u in users(k)? {
        let h = grant::header(txn, id)?;
        if u == "*PUBLIC" {
            let cur = Auth::of(h.public_authority);
            grant::set_public(txn, id, Some(Auth(cur.0 & !bits.0)))?;
            continue;
        }
        let Some(cur) = grant::private(txn, &h, &u)? else {
            continue;
        };
        let left = Auth(cur.0 & !bits.0);
        let keep = (!all && left != Auth::EXCLUDE).then_some(left);
        grant::set_private(txn, env.qsys, id, &u, keep)?;
    }
    Ok(format!("Authority removed for {what}."))
}

/// `DSPOBJAUT`: owner, primary group, list, and each user's authority.
/// A user without `*OBJMGT` (and not the owner) sees only its own.
pub fn dspobjaut(txn: &mut Txn<'_>, env: &Env<'_>, id: ObjectId) -> Result<String> {
    let h = grant::header(txn, id)?;
    guard::check(txn, "obj.describe", &h)?;
    let autl = id.type_code == types::AUTL;
    let everyone = text_of(&h.owner) == env.actor.user
        || search::check(txn, env.qsys, env.actor, &h, Need::Bits(Auth(A::OBJMGT)))?.allowed;
    let lib = library_of(txn, &h);
    let pgp = none_if_empty(text_of(&h.primary_group));
    let list = none_if_empty(text_of(&h.autl));
    let mut o = String::new();
    let _ = writeln!(
        o,
        " Object . . . . . . . :   {:<10}   Owner  . . . . . . . :   {}",
        h.name,
        text_of(&h.owner)
    );
    let _ = writeln!(
        o,
        "   Library  . . . . . :   {lib:<10}   Primary group  . . . :   {pgp}"
    );
    let _ = writeln!(
        o,
        " Object type  . . . . :   {:<10}   Authorization list . :   {list}",
        type_name(id)
    );
    let _ = writeln!(o, "                          Object");
    let _ = writeln!(o, " User        Group       Authority");
    let row = |o: &mut String, user: &str, group: &str, bits: Auth| {
        let _ = writeln!(o, " {user:<10}  {group:<10}  {}", bits.display(autl));
    };
    let owner = text_of(&h.owner);
    let shown = |u: &str| everyone || u == env.actor.user;
    if shown(&owner) {
        row(&mut o, &owner, "", Auth::of(h.owner_authority));
    }
    if pgp != "*NONE" && shown(&pgp) {
        row(&mut o, &pgp, "", Auth::of(h.group_authority));
    }
    for pa in auth::private_list(txn, &h)? {
        let u = text_of(&pa.profile);
        if shown(&u) {
            row(&mut o, &u, "", Auth::of(pa.bits));
        }
    }
    if everyone {
        let public = if h.public_authority.0 & A::FROM_AUTL != 0 {
            "*AUTL"
        } else {
            Auth::of(h.public_authority).display(autl)
        };
        let _ = writeln!(o, " {:<10}  {:<10}  {public}", "*PUBLIC", "");
    }
    Ok(o)
}

/// The object and data authorities of each user, as `DSPOBJAUT
/// AUTFMT(*DETAIL)` lists them.
pub fn dspobjaut_detail(txn: &mut Txn<'_>, id: ObjectId) -> Result<String> {
    let h = grant::header(txn, id)?;
    guard::check(txn, "obj.describe", &h)?;
    let mut rows = vec![(text_of(&h.owner), Auth::of(h.owner_authority))];
    let pgp = text_of(&h.primary_group);
    if !pgp.is_empty() {
        rows.push((pgp, Auth::of(h.group_authority)));
    }
    for pa in auth::private_list(txn, &h)? {
        rows.push((text_of(&pa.profile), Auth::of(pa.bits)));
    }
    rows.push(("*PUBLIC".into(), Auth::of(h.public_authority)));
    let mut o = String::from(" User        Opr Mgt Exist Alter Ref  Read Add Upd Dlt Exe\n");
    for (u, b) in rows {
        let x = |m: u32| if b.0 & m != 0 { "X" } else { " " };
        let _ = writeln!(
            o,
            " {u:<10}   {}   {}    {}     {}    {}    {}   {}   {}   {}   {}",
            x(A::OBJOPR),
            x(A::OBJMGT),
            x(A::OBJEXIST),
            x(A::OBJALTER),
            x(A::OBJREF),
            x(A::READ),
            x(A::ADD),
            x(A::UPD),
            x(A::DLT),
            x(A::EXECUTE)
        );
    }
    Ok(o)
}

/// `CHGOBJOWN OBJ() OBJTYPE() NEWOWN() [CUROWNAUT(*REVOKE|*SAME)]`.
pub fn chgobjown(txn: &mut Txn<'_>, env: &Env<'_>, id: ObjectId, k: &Keywords) -> Result<String> {
    let h = grant::header(txn, id)?;
    let what = label(txn, &h);
    guard::check(txn, "obj.change-owner", &h)?;
    let new = k
        .upper("NEWOWN")
        .ok_or_else(|| SecError::msg("CPD0071", "NEWOWN is required."))?;
    let (pid, _) = objects::read(txn, env.qsys, &new)?;
    guard::check_id(txn, "usrprf.add-owned", pid)?;
    let cur = one_of(
        "CUROWNAUT",
        &k.upper("CUROWNAUT").unwrap_or_else(|| "*REVOKE".into()),
        &["*REVOKE", "*SAME"],
    )?;
    grant::set_owner(txn, env.qsys, id, &new, cur == "*REVOKE")?;
    Ok(format!("Owner of {what} changed to {new}."))
}

/// `CHGOBJPGP OBJ() OBJTYPE() NEWPGP(name|*NONE) [RVKOLDAUT(*YES|*NO)]`.
pub fn chgobjpgp(txn: &mut Txn<'_>, env: &Env<'_>, id: ObjectId, k: &Keywords) -> Result<String> {
    let h = grant::header(txn, id)?;
    let what = label(txn, &h);
    guard::check(txn, "obj.change-pgp", &h)?;
    let new = k
        .upper("NEWPGP")
        .ok_or_else(|| SecError::msg("CPD0071", "NEWPGP is required."))?;
    let revoke = one_of(
        "RVKOLDAUT",
        &k.upper("RVKOLDAUT").unwrap_or_else(|| "*YES".into()),
        &["*YES", "*NO"],
    )? == "*YES";
    let group = (new != "*NONE").then_some(new.as_str());
    grant::set_group(txn, env.qsys, id, group, revoke)?;
    Ok(format!("Primary group of {what} changed to {new}."))
}

/// `CHKOBJ OBJ() OBJTYPE() [AUT()]`: `CPF9802` when the actor lacks an
/// authority named.
pub fn chkobj(txn: &mut Txn<'_>, env: &Env<'_>, id: ObjectId, k: &Keywords) -> Result<String> {
    let h = grant::header(txn, id)?;
    let what = label(txn, &h);
    let words = auth_words(k, "AUT", "*NONE");
    if words != ["*NONE"] {
        let need = parse_auth(&words, id.type_code == types::AUTL)?;
        if !search::check(txn, env.qsys, env.actor, &h, Need::Bits(need))?.allowed {
            return Err(SecError::msg(
                "CPF9802",
                format!("Not authorized to {what}."),
            ));
        }
    }
    Ok(format!("{what} exists."))
}

/// `GRTUSRAUT USER() REFUSER()`: the reference user's private
/// authorities given to `USER` as well. Objects the actor may not grant
/// on are skipped and counted.
pub fn grtusraut(txn: &mut Txn<'_>, env: &Env<'_>, k: &Keywords) -> Result<String> {
    let user = k
        .upper("USER")
        .ok_or_else(|| SecError::msg("CPD0071", "USER is required."))?;
    let reference = k
        .upper("REFUSER")
        .ok_or_else(|| SecError::msg("CPD0071", "REFUSER is required."))?;
    objects::read(txn, env.qsys, &user)?;
    let (mut given, mut skipped) = (0, 0);
    for (id, bits) in index::list(txn, env.qsys, &reference, Kind::Private)? {
        let h = grant::header(txn, id)?;
        if guard::check(txn, "obj.grant", &h).is_err() {
            skipped += 1;
            continue;
        }
        let bits = Auth(u32::try_from(bits).unwrap_or(0));
        let new = grant::private(txn, &h, &user)?.map_or(bits, |c| c.union(bits));
        grant::set_private(txn, env.qsys, id, &user, Some(new))?;
        given += 1;
    }
    if skipped > 0 {
        return Err(SecError::msg(
            "CPF2227",
            format!("{given} authorities given to {user}; {skipped} objects skipped."),
        ));
    }
    Ok(format!(
        "{given} authorities of {reference} given to {user}."
    ))
}

/// The objects a user owns (`Kind::Owned`), holds a private authority
/// to (`Kind::Private`) or is the primary group of (`Kind::Group`):
/// `WRKOBJOWN` and `DSPUSRPRF TYPE(*OBJOWN|*OBJAUT|*OBJPGP)`.
pub fn objects_of(txn: &mut Txn<'_>, env: &Env<'_>, user: &str, kind: Kind) -> Result<String> {
    let user = if user.eq_ignore_ascii_case("*CURRENT") {
        env.actor.user.clone()
    } else {
        user.to_ascii_uppercase()
    };
    if user != env.actor.user {
        let (pid, _) = objects::read(txn, env.qsys, &user)?;
        guard::check_id(txn, "usrprf.display", pid)?;
    }
    let mut o = format!(" User profile {user}\n Object      Library     Type        Authority\n");
    for (id, bits) in index::list(txn, env.qsys, &user, kind)? {
        let h = grant::header(txn, id)?;
        let bits = match kind {
            Kind::Owned => Auth::of(h.owner_authority),
            _ => Auth(u32::try_from(bits).unwrap_or(0)),
        };
        let lib = library_of(txn, &h);
        let _ = writeln!(
            o,
            " {:<10}  {lib:<10}  {:<10}  {}",
            h.name.to_string(),
            type_name(id),
            bits.display(id.type_code == types::AUTL)
        );
    }
    Ok(o)
}

/// `CRTAUTL AUTL() [AUT()] [TEXT()]`; `*PUBLIC` defaults to `*EXCLUDE`.
pub fn crtautl(txn: &mut Txn<'_>, env: &Env<'_>, k: &Keywords) -> Result<String> {
    let name = k
        .upper("AUTL")
        .ok_or_else(|| SecError::msg("CPD0071", "AUTL is required."))?;
    let public = parse_auth(&auth_words(k, "AUT", "*EXCLUDE"), true)?;
    let text = k.get("TEXT").unwrap_or_default();
    autl::create(
        txn,
        env.qsys,
        &name,
        &text,
        &env.actor.user,
        public.store(),
        env.now,
    )?;
    Ok(format!("Authorization list {name} created."))
}

/// `ADDAUTLE` (`add`) and `CHGAUTLE`: a user's entry on a list.
pub fn setautle(txn: &mut Txn<'_>, env: &Env<'_>, k: &Keywords, add: bool) -> Result<String> {
    let name = k
        .upper("AUTL")
        .ok_or_else(|| SecError::msg("CPD0071", "AUTL is required."))?;
    let list = autl::find(txn, env.qsys, &name)?;
    let h = grant::header(txn, list)?;
    guard::check(txn, "autl.manage", &h)?;
    let bits = parse_auth(&auth_words(k, "AUT", "*USE"), true)?;
    if let Some(have) = grantable(txn, env, &h)? {
        let missing = have.missing(bits);
        if missing != Auth::EXCLUDE {
            return Err(SecError::msg(
                "CPF2227",
                format!(
                    "You cannot grant authorities you do not have: {}.",
                    missing.names().join(" ")
                ),
            ));
        }
    }
    for u in users(k)? {
        if u == "*PUBLIC" {
            grant::set_public(txn, list, Some(bits))?;
            continue;
        }
        let h = grant::header(txn, list)?;
        let on = grant::private(txn, &h, &u)?.is_some() || text_of(&h.owner) == u;
        if add && on {
            return Err(SecError::msg(
                "CPF2272",
                format!("{u} is already on authorization list {name}."),
            ));
        }
        if !add && !on {
            return Err(SecError::msg(
                "CPF2270",
                format!("{u} is not on authorization list {name}."),
            ));
        }
        grant::set_private(txn, env.qsys, list, &u, Some(bits))?;
    }
    Ok(format!("Authorization list {name} changed."))
}

/// `RMVAUTLE AUTL() USER()`.
pub fn rmvautle(txn: &mut Txn<'_>, env: &Env<'_>, k: &Keywords) -> Result<String> {
    let name = k
        .upper("AUTL")
        .ok_or_else(|| SecError::msg("CPD0071", "AUTL is required."))?;
    let list = autl::find(txn, env.qsys, &name)?;
    let h = grant::header(txn, list)?;
    guard::check(txn, "autl.manage", &h)?;
    for u in users(k)? {
        let h = grant::header(txn, list)?;
        if grant::private(txn, &h, &u)?.is_none() {
            return Err(SecError::msg(
                "CPF2270",
                format!("{u} is not on authorization list {name}."),
            ));
        }
        grant::set_private(txn, env.qsys, list, &u, None)?;
    }
    Ok(format!("Authorization list {name} changed."))
}

/// `DSPAUTL AUTL()`: the list's owner, entries and `*PUBLIC`, with
/// `*AUTLMGT`.
pub fn dspautl(txn: &mut Txn<'_>, env: &Env<'_>, name: &str) -> Result<String> {
    let list = autl::find(txn, env.qsys, name)?;
    let h = grant::header(txn, list)?;
    guard::check(txn, "obj.describe", &h)?;
    let mut o = format!(
        " Authorization list . . . . :   {name}\n Owner  . . . . . . . . . . :   {}\n Text . . . . . . . . . . . :   {}\n User        Authority  List mgt\n",
        text_of(&h.owner),
        text_of(&h.text)
    );
    let row = |o: &mut String, u: &str, b: Auth| {
        let mgt = if b.0 & A::AUTLMGT != 0 { "X" } else { "" };
        let _ = writeln!(
            o,
            " {u:<10}  {:<9}  {mgt}",
            Auth(b.0 & Auth::ALL.0).display(false)
        );
    };
    row(&mut o, &text_of(&h.owner), Auth::of(h.owner_authority));
    for pa in auth::private_list(txn, &h)? {
        row(&mut o, &text_of(&pa.profile), Auth::of(pa.bits));
    }
    row(&mut o, "*PUBLIC", Auth::of(h.public_authority));
    Ok(o)
}

/// `DSPAUTLOBJ AUTL()`: the objects the list secures.
pub fn dspautlobj(txn: &mut Txn<'_>, env: &Env<'_>, name: &str) -> Result<String> {
    let list = autl::find(txn, env.qsys, name)?;
    let h = grant::header(txn, list)?;
    guard::check(txn, "obj.describe", &h)?;
    let mut o = format!(" Authorization list {name}\n Object      Library     Type        Owner\n");
    for id in autl::secured(txn, env.qsys, name)? {
        let oh = grant::header(txn, id)?;
        let lib = library_of(txn, &oh);
        let _ = writeln!(
            o,
            " {:<10}  {lib:<10}  {:<10}  {}",
            oh.name.to_string(),
            type_name(id),
            text_of(&oh.owner)
        );
    }
    Ok(o)
}
