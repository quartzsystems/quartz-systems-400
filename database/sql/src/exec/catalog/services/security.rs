// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Object authorities as views: `OBJECT_PRIVILEGES`, `OBJECT_OWNERSHIP`,
//! `AUTHORIZATION_LIST_INFO`, `AUTHORIZATION_LIST_USER_INFO` and
//! `USER_STORAGE`, from the object headers and the profiles the
//! security layer keeps.

use sec::profile;
use store::obj::header::{Authority as A, ObjectHeader};
use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use super::super::super::SqlContext;
use super::Out;
use crate::error::Result;

/// IBM's name of an authority set.
fn authority_name(bits: u32) -> &'static str {
    let b = bits & 0x03FF;
    if bits == 0 {
        "*EXCLUDE"
    } else if b == A::ALL.0 {
        "*ALL"
    } else if b == A::CHANGE.0 {
        "*CHANGE"
    } else if b == A::USE.0 {
        "*USE"
    } else {
        "USER DEFINED"
    }
}

/// The authority bits as the views' YES/NO columns.
fn bits(out: &mut Out, b: u32, autl: bool) {
    out.text("OBJECT_AUTHORITY", authority_name(b));
    if autl {
        out.yes("AUTHORIZATION_LIST_MANAGEMENT", b & A::AUTLMGT != 0);
    } else {
        out.yes("AUTHORIZATION_LIST_MANAGEMENT", false);
    }
    for (col, bit) in [
        ("OBJECT_OPERATIONAL", A::OBJOPR),
        ("OBJECT_MANAGEMENT", A::OBJMGT),
        ("OBJECT_EXISTENCE", A::OBJEXIST),
        ("OBJECT_ALTER", A::OBJALTER),
        ("OBJECT_REFERENCE", A::OBJREF),
        ("DATA_READ", A::READ),
        ("DATA_ADD", A::ADD),
        ("DATA_UPDATE", A::UPD),
        ("DATA_DELETE", A::DLT),
        ("DATA_EXECUTE", A::EXECUTE),
    ] {
        out.yes(col, b & bit != 0);
    }
}

/// Every object of every library the user can list, with its header:
/// library, type name, object, header.
fn objects(txn: &mut Txn<'_>, ctx: &SqlContext, only: Option<u16>) -> Result<Vec<(String, &'static str, ObjectId, ObjectHeader)>> {
    let mut libs: Vec<ObjectId> = ops::list(txn, ctx.qsys, Some(types::LIB))?.into_iter().map(|(_, id)| id).collect();
    libs.insert(0, ctx.qsys);
    let mut out = Vec::new();
    for lib in libs {
        let Ok((_, lh)) = ops::read_header(txn, lib) else { continue };
        let Ok(list) = ops::list(txn, lib, only) else { continue };
        for (k, id) in list {
            let Some(ti) = types::type_by_code(k.type_code) else { continue };
            if matches!(ti.name, "*MBR" | "*SPLF" | "*SQLINF") {
                continue;
            }
            if let Ok((_, h)) = ops::read_header(txn, id) {
                out.push((lh.name.to_string(), ti.name, id, h));
            }
        }
    }
    Ok(out)
}

/// Each holder of an authority to `h`: the owner, the private ones and
/// `*PUBLIC`.
fn holders(txn: &mut Txn<'_>, h: &ObjectHeader) -> Vec<(String, u32, bool)> {
    let owner = ccsid::decode_trimmed(&h.owner);
    let mut out = vec![(owner.clone(), h.owner_authority.0, true)];
    if let Ok(list) = store::obj::auth::private_list(txn, h) {
        for p in list {
            let u = ccsid::decode_trimmed(&p.profile);
            if u != owner {
                out.push((u, p.bits.0, false));
            }
        }
    }
    out.push(("*PUBLIC".into(), h.public_authority.0, false));
    out
}

/// `OBJECT_PRIVILEGES`.
pub fn privileges(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    for (lib, ty, id, h) in objects(txn, ctx, None)? {
        let schema = crate::exec::names::schema_name(txn, h.library);
        let name = if ty == "*FILE" { crate::exec::names::sql_name(txn, id) } else { h.name.to_string() };
        let owner = ccsid::decode_trimmed(&h.owner);
        let autl = ccsid::decode_trimmed(&h.autl);
        let group = ccsid::decode_trimmed(&h.primary_group);
        let text = ccsid::decode_trimmed(&h.text);
        for (who, b, _) in holders(txn, &h) {
            out.row();
            out.text("OBJECT_SCHEMA", schema.as_str());
            out.text("OBJECT_NAME", name.as_str());
            out.text("SYSTEM_OBJECT_SCHEMA", lib.as_str());
            out.text("SYSTEM_OBJECT_NAME", h.name.to_string());
            out.text("OBJECT_TYPE", ty);
            out.text("AUTHORIZATION_NAME", who);
            out.text("OWNER", owner.as_str());
            out.text("OBJECT_OWNER", owner.as_str());
            out.text_or_null("AUTHORIZATION_LIST", &autl);
            out.text_or_null("PRIMARY_GROUP", &group);
            out.text_or_null("TEXT_DESCRIPTION", &text);
            bits(out, b, false);
        }
    }
    Ok(())
}

/// `OBJECT_OWNERSHIP`: each profile's owned objects.
pub fn ownership(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    let mut all = objects(txn, ctx, None)?;
    all.sort_by(|a, b| (ccsid::decode_trimmed(&a.3.owner), &a.0, a.3.name.to_string()).cmp(&(ccsid::decode_trimmed(&b.3.owner), &b.0, b.3.name.to_string())));
    for (lib, ty, _, h) in all {
        out.row();
        out.text("AUTHORIZATION_NAME", ccsid::decode_trimmed(&h.owner));
        out.text("OBJECT_TYPE", ty);
        out.text("OBJECT_LIBRARY", lib.as_str());
        out.text("OBJECT_NAME", h.name.to_string());
        out.text_or_null("OBJECT_ATTRIBUTE", &ccsid::decode_trimmed(&h.attribute));
        out.text_or_null("TEXT_DESCRIPTION", &ccsid::decode_trimmed(&h.text));
        out.text("IASP_NAME", "*SYSBAS");
        out.yes("AUTHORITY_HOLDER", false);
        bits(out, h.owner_authority.0, false);
    }
    Ok(())
}

/// `AUTHORIZATION_LIST_INFO`: the objects each list secures.
pub fn autl_objects(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    let lists = sec::auth::autl::list(txn, ctx.qsys).unwrap_or_default();
    for l in lists {
        let objs = sec::auth::autl::secured(txn, ctx.qsys, &l).unwrap_or_default();
        for id in objs {
            let Ok((_, h)) = ops::read_header(txn, id) else { continue };
            let Ok((_, lh)) = ops::read_header(txn, h.library) else { continue };
            let ty = ops::list(txn, h.library, None)
                .ok()
                .and_then(|v| v.into_iter().find(|(_, o)| *o == id))
                .and_then(|(k, _)| types::type_by_code(k.type_code))
                .map_or("*N", |t| t.name);
            out.row();
            out.text("AUTHORIZATION_LIST", l.as_str());
            out.text("SYSTEM_OBJECT_SCHEMA", lh.name.to_string());
            out.text("SYSTEM_OBJECT_NAME", h.name.to_string());
            out.text("SYSTEM_OBJECT_TYPE", ty);
            out.text_or_null("OBJECT_ATTRIBUTE", &ccsid::decode_trimmed(&h.attribute));
            out.text("OBJECT_SCHEMA", crate::exec::names::schema_name(txn, h.library));
            out.text("OBJECT_NAME", h.name.to_string());
            out.text("OBJECT_TYPE", ty);
            out.text("OBJECT_OWNER", ccsid::decode_trimmed(&h.owner));
            out.text_or_null("PRIMARY_GROUP", &ccsid::decode_trimmed(&h.primary_group));
            out.text_or_null("TEXT_DESCRIPTION", &ccsid::decode_trimmed(&h.text));
            out.text("ASPGRP", "*SYSBAS");
            out.yes("AUTHORITY_HOLDER", false);
        }
    }
    Ok(())
}

/// `AUTHORIZATION_LIST_USER_INFO`: the users each list authorizes.
pub fn autl_users(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    let lists = sec::auth::autl::list(txn, ctx.qsys).unwrap_or_default();
    for l in lists {
        let Ok(id) = sec::auth::autl::find(txn, ctx.qsys, &l) else { continue };
        let Ok((_, h)) = ops::read_header(txn, id) else { continue };
        let text = ccsid::decode_trimmed(&h.text);
        for (who, b, owner) in holders(txn, &h) {
            out.row();
            out.text("AUTHORIZATION_LIST", l.as_str());
            out.text("AUTHORIZATION_NAME", who);
            out.yes("OWNER", owner);
            out.text_or_null("TEXT_DESCRIPTION", &text);
            bits(out, b, true);
        }
    }
    Ok(())
}

/// `USER_STORAGE`: each profile's storage limit and use, and its
/// counts of owned objects and private authorities.
#[allow(clippy::unnecessary_wraps)] // a view's rows may fail
pub fn storage(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    let names = profile::objects::list(txn, ctx.qsys).unwrap_or_default();
    for n in names {
        let Ok((_, p)) = profile::objects::read(txn, ctx.qsys, &n) else { continue };
        out.row();
        out.text("AUTHORIZATION_NAME", p.name.as_str());
        out.text("ASPGRP", "*SYSBAS");
        if p.maxstg >= 0 {
            out.int("MAXIMUM_STORAGE_ALLOWED", p.maxstg);
        }
        out.int("STORAGE_USED", p.stg_used);
        out.int("OWNER_ENTRIES", p.owned_count);
    }
    Ok(())
}

/// `USER_STORAGE`'s `NOT NULL` columns with no value here: QS/400's
/// profiles have no fixed entry table to count against.
pub const STORAGE_NO_VALUE: &[&str] = &[
    "AVAILABLE_PROFILE_ENTRIES",
    "TOTAL_PROFILE_ENTRIES",
    "PRIVATE_AUTHORITY_ENTRIES",
    "AUTHORIZED_USER_ENTRIES",
    "PRIMARY_GROUP_ENTRIES",
];
