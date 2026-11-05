// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `GRANT` and `REVOKE` on tables and views, as object authorities, the
//! way the primary oracle maps them (`tools/conformance/oracle/sql-ddl`):
//! SELECT is `*OBJOPR *READ`, INSERT `*OBJOPR *ADD`, UPDATE `*OBJOPR
//! *UPD`, DELETE `*OBJOPR *DLT`, ALTER and INDEX `*OBJALTER`, REFERENCES
//! `*OBJREF`; WITH GRANT OPTION adds `*OBJMGT`.

use sec::auth::bits::Auth;
use sec::auth::grant;
use store::obj::header::Authority as A;
use store::obj::{ObjectId, guard};
use store::txn::Txn;

use super::{SqlContext, view};
use crate::ast::{Grant, GrantObject, Grantee};
use crate::error::{Result, SqlError};
use crate::sqlca::Warning;

/// The data authorities: one of them keeps `*OBJOPR`.
const DATA: u32 = A::READ | A::ADD | A::UPD | A::DLT;

/// The authorities of a table privilege.
fn bits_of(p: &str) -> Option<u32> {
    Some(match p {
        "SELECT" => A::OBJOPR | A::READ,
        "INSERT" => A::OBJOPR | A::ADD,
        "UPDATE" => A::OBJOPR | A::UPD,
        "DELETE" => A::OBJOPR | A::DLT,
        "ALTER" | "INDEX" => A::OBJALTER,
        "REFERENCES" => A::OBJREF,
        _ => return None,
    })
}

/// The privileges `ALL` means for a table.
const ALL: &[&str] = &["SELECT", "INSERT", "UPDATE", "DELETE", "ALTER", "INDEX", "REFERENCES"];

/// What a privilege names that it must hold for `REVOKE` to take it
/// (the data authority, or the object authority).
fn held_bit(p: &str) -> u32 {
    bits_of(p).map_or(0, |b| if b & DATA != 0 { b & DATA } else { b })
}

/// `GRANT` (`revoke` false) or `REVOKE`.
pub fn run(txn: &mut Txn<'_>, ctx: &mut SqlContext, g: &Grant, revoke: bool) -> Result<()> {
    let names_ = match &g.object {
        GrantObject::Tables(n) => n,
        GrantObject::Other { kind, names } if matches!(kind.as_str(), "TYPE" | "VARIABLE") => {
            return super::grant_obj::run(txn, ctx, g, kind, names, revoke);
        }
        GrantObject::Other { kind, .. } => {
            // BLOCKED(sql): routines, sequences, schemas and packages as
            // objects come with SQL-2 and Section 3; the statement is
            // parsed for them.
            return Err(SqlError::Unsupported { what: format!("GRANT and REVOKE on a {kind}") });
        }
    };
    if let Some(p) = g.privileges.iter().find(|p| !p.columns.is_empty()) {
        // BLOCKED(sql): the store keeps no column authorities.
        return Err(SqlError::Unsupported { what: format!("the column privilege {}: authorities by column", p.name) });
    }
    if let Some(p) = g.privileges.iter().find(|p| bits_of(&p.name).is_none()) {
        return Err(SqlError::Syntax { line: 1, col: 1, found: p.name.clone(), expected: "a table privilege".into() });
    }
    for name in names_ {
        let id = ctx.resolve_file(txn, name)?;
        let is_view = view::is_view(txn, id);
        let shown = name.name.value.clone();
        let mut privs: Vec<&str> = if g.all { ALL.to_vec() } else { g.privileges.iter().map(|p| p.name.as_str()).collect() };
        if is_view {
            if !g.all && privs.contains(&"INDEX") {
                return Err(SqlError::PrivilegeNotValid { name: shown });
            }
            privs.retain(|p| *p != "INDEX");
            let changes = privs.iter().any(|p| matches!(*p, "INSERT" | "UPDATE" | "DELETE"));
            let read_only = view::definition(txn, id).is_some_and(|(v, _)| v.read_only);
            if !revoke && changes && (read_only || g.all || privs.contains(&"INSERT") && !insertable(txn, id)) {
                return Err(SqlError::CannotGrant { name: shown });
            }
        }
        let mut bits = privs.iter().filter_map(|p| bits_of(p)).fold(0, |a, b| a | b);
        if g.grant_option {
            bits |= A::OBJMGT;
        }
        let h = grant::header(txn, id).map_err(sec_err)?;
        guard::check(txn, "obj.grant", &h)?;
        for who in &g.grantees {
            if revoke {
                revoke_one(txn, ctx, id, who, &privs, g.all, &shown)?;
            } else {
                grant_one(txn, ctx, id, who, bits)?;
            }
        }
    }
    Ok(())
}

/// Whether a view takes inserts: a logical file holding every field of
/// its table. IBM's other rules wait on views that change their tables.
fn insertable(txn: &mut Txn<'_>, id: ObjectId) -> bool {
    let Ok(d) = db::file::describe(txn, id) else { return false };
    let Some(fmt) = d.formats.first() else { return false };
    let Some(log) = &fmt.logical else { return false };
    let Some((base, _)) = log.based_on.first() else { return false };
    db::file::describe(txn, *base).is_ok_and(|b| b.formats.first().is_some_and(|bf| bf.format.fields.len() == fmt.format.fields.len()))
}

pub(super) fn grant_one(txn: &mut Txn<'_>, ctx: &SqlContext, id: ObjectId, who: &Grantee, bits: u32) -> Result<()> {
    match who {
        Grantee::Public => {
            let cur = Auth::of(grant::header(txn, id).map_err(sec_err)?.public_authority);
            grant::set_public(txn, id, Some(cur.union(Auth(bits)))).map_err(sec_err)
        }
        Grantee::User(u) => {
            let user = u.value.to_uppercase();
            let h = grant::header(txn, id).map_err(sec_err)?;
            let cur = grant::private(txn, &h, &user).map_err(sec_err)?.unwrap_or(Auth::EXCLUDE);
            grant::set_private(txn, ctx.qsys, id, &user, Some(cur.union(Auth(bits)))).map_err(sec_err)
        }
    }
}

fn revoke_one(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    id: ObjectId,
    who: &Grantee,
    privs: &[&str],
    all: bool,
    shown: &str,
) -> Result<()> {
    let h = grant::header(txn, id).map_err(sec_err)?;
    let cur = match who {
        Grantee::Public => Some(Auth::of(h.public_authority)),
        Grantee::User(u) => grant::private(txn, &h, &u.value.to_uppercase()).map_err(sec_err)?,
    };
    let have = cur.unwrap_or(Auth::EXCLUDE).0;
    let mut left = have;
    let mut missing = false;
    if all {
        left = 0;
    } else {
        for p in privs {
            let need = held_bit(p);
            if have & need != need {
                missing = true;
                continue;
            }
            left &= !need;
        }
        // `*OBJOPR` goes with the last data authority, `*OBJMGT` with
        // the last privilege.
        if left & DATA == 0 {
            left &= !A::OBJOPR;
        }
        if left & !A::OBJMGT == 0 {
            left = 0;
        }
    }
    if missing || cur.is_none() && !all {
        ctx.warning = Some(Warning::not_all_revoked(shown));
    }
    match who {
        Grantee::Public => grant::set_public(txn, id, Some(Auth(left))).map_err(sec_err),
        Grantee::User(u) if cur.is_some() => {
            let keep = (left != 0).then_some(Auth(left));
            grant::set_private(txn, ctx.qsys, id, &u.value.to_uppercase(), keep).map_err(sec_err)
        }
        Grantee::User(_) => Ok(()),
    }
}

/// A security failure as SQL sees it: an unknown profile is `SQL0204`.
pub(super) fn sec_err(e: sec::SecError) -> SqlError {
    match e {
        sec::SecError::Store(s) => s.into(),
        sec::SecError::Msg { msgid: "CPF2204", text } => SqlError::ObjectNotFound { name: text, kind: "*USRPRF" },
        e => SqlError::Database { what: e.to_string(), msgid: None },
    }
}

/// The privileges an authority set holds on a table, as `SYSTABAUTH`
/// lists them, with whether each is grantable.
#[must_use]
pub fn privileges(bits: u32, is_view: bool) -> Vec<(&'static str, bool)> {
    let grantable = bits & A::OBJMGT != 0;
    let mut out = Vec::new();
    for p in ALL {
        if is_view && *p == "INDEX" {
            continue;
        }
        let b = bits_of(p).expect("a table privilege");
        if bits & b == b {
            out.push((*p, grantable));
        }
    }
    out
}

/// Every grantee of an object with its authority bits: the owner with
/// all of them, the private authorities, `PUBLIC` when it has any.
pub fn grantees(txn: &mut Txn<'_>, id: ObjectId) -> Vec<(String, u32)> {
    let Ok(h) = grant::header(txn, id) else { return Vec::new() };
    let owner = ccsid::decode_trimmed(&h.owner);
    let mut out = vec![(owner.clone(), Auth::ALL.0)];
    if let Ok(list) = store::obj::auth::private_list(txn, &h) {
        for p in list {
            let u = ccsid::decode_trimmed(&p.profile);
            if u != owner {
                out.push((u, p.bits.0));
            }
        }
    }
    let public = Auth::of(h.public_authority).0;
    if public != 0 {
        out.push(("PUBLIC".into(), public));
    }
    out
}
