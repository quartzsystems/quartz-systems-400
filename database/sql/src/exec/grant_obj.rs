// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `GRANT` and `REVOKE` on distinct types and global variables, as the
//! primary maps them to object authorities
//! (`tools/conformance/sql/cases/objects2/GRANTUDT`): a type's `USAGE` is
//! `*OBJOPR *EXECUTE`, its `ALTER` `*OBJALTER`; a variable's `READ` is
//! `*OBJOPR *READ *EXECUTE`, `WRITE` `*OBJOPR *UPD *EXECUTE`, `ALTER`
//! `*OBJALTER`; `ALL` is every one of the kind's; `WITH GRANT OPTION`
//! adds `*OBJMGT`.

use sec::auth::bits::Auth;
use sec::auth::grant;
use store::obj::header::Authority as A;
use store::obj::{ObjectId, guard};
use store::txn::Txn;

use super::SqlContext;
use super::grant::{grant_one, sec_err};
use crate::ast::{Grant, Grantee, ObjectName};
use crate::error::{Result, SqlError};
use crate::sqlca::Warning;

/// The privileges of a kind.
fn privileges(kind: &str) -> &'static [&'static str] {
    if kind == "TYPE" {
        &["USAGE", "ALTER"]
    } else {
        &["READ", "WRITE", "ALTER"]
    }
}

/// The authorities of a privilege of a kind.
fn bits_of(kind: &str, p: &str) -> Option<u32> {
    Some(match (kind, p) {
        ("TYPE", "USAGE") => A::OBJOPR | A::EXECUTE,
        ("VARIABLE", "READ") => A::OBJOPR | A::READ | A::EXECUTE,
        ("VARIABLE", "WRITE") => A::OBJOPR | A::UPD | A::EXECUTE,
        (_, "ALTER") => A::OBJALTER,
        _ => return None,
    })
}

/// What a privilege must hold for `REVOKE` to take it.
fn held_bit(kind: &str, p: &str) -> u32 {
    match (kind, p) {
        ("VARIABLE", "READ") => A::READ,
        ("VARIABLE", "WRITE") => A::UPD,
        _ => bits_of(kind, p).unwrap_or(0),
    }
}

/// The object a type's or variable's name means.
fn object(txn: &mut Txn<'_>, ctx: &SqlContext, kind: &str, name: &ObjectName) -> Result<ObjectId> {
    let found = if kind == "TYPE" {
        super::udt::find(txn, ctx, name)?.map(|(id, _)| id)
    } else {
        super::var::find(txn, ctx, name)?.map(|f| f.id)
    };
    found.ok_or_else(|| SqlError::ObjectNotFound {
        name: name.to_string(),
        kind: if kind == "TYPE" { "type" } else { "variable" },
    })
}

/// `GRANT` (`revoke` false) or `REVOKE` on types or variables.
pub fn run(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    g: &Grant,
    kind: &str,
    names: &[ObjectName],
    revoke: bool,
) -> Result<()> {
    // A privilege the kind has not is a syntax error on the primary.
    if let Some(p) = g
        .privileges
        .iter()
        .find(|p| !p.columns.is_empty() || bits_of(kind, &p.name).is_none())
    {
        return Err(SqlError::Syntax {
            line: 1,
            col: 1,
            found: p.name.clone(),
            expected: format!("a {} privilege", kind.to_lowercase()),
        });
    }
    let privs: Vec<&str> = if g.all {
        privileges(kind).to_vec()
    } else {
        g.privileges.iter().map(|p| p.name.as_str()).collect()
    };
    for name in names {
        let id = object(txn, ctx, kind, name)?;
        let h = grant::header(txn, id).map_err(sec_err)?;
        guard::check(txn, "obj.grant", &h)?;
        for who in &g.grantees {
            if revoke {
                revoke_one(txn, ctx, (kind, id), who, &privs, g.all, &name.name.value)?;
            } else {
                let mut bits = privs
                    .iter()
                    .filter_map(|p| bits_of(kind, p))
                    .fold(0, |a, b| a | b);
                if g.grant_option {
                    bits |= A::OBJMGT;
                }
                grant_one(txn, ctx, id, who, bits)?;
            }
        }
    }
    Ok(())
}

fn revoke_one(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    (kind, id): (&str, ObjectId),
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
            let need = held_bit(kind, p);
            if have & need != need {
                missing = true;
                continue;
            }
            left &= !need;
        }
        // `*OBJOPR *EXECUTE` go with the last use of the object.
        let uses = if kind == "TYPE" {
            A::EXECUTE
        } else {
            A::READ | A::UPD
        };
        if left & uses == 0 {
            left &= !(A::OBJOPR | A::EXECUTE);
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
