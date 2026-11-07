// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSYS2.LIBRARY_LIST_INFO`: the job's library list in search order,
//! each part named as the job's attributes keep it. Without a host the
//! list is the statement's own and the part is null.

use qsname::Name;
use store::obj::{ObjectId, ops};
use store::txn::Txn;

use super::super::super::SqlContext;
use super::Out;
use crate::error::Result;

/// The rows.
#[allow(clippy::unnecessary_wraps)] // a view's rows may fail
pub fn rows(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    let parts: Vec<(String, Option<&str>)> = match ctx
        .host
        .as_deref()
        .and_then(super::host::SystemHost::library_list)
    {
        Some(l) => {
            let mut v: Vec<(String, Option<&str>)> =
                l.sys.iter().map(|n| (n.clone(), Some("SYSTEM"))).collect();
            v.extend(
                l.prd
                    .iter()
                    .filter(|n| !n.trim().is_empty())
                    .map(|n| (n.clone(), Some("PRODUCT"))),
            );
            v.extend(l.cur.iter().map(|n| (n.clone(), Some("CURRENT"))));
            v.extend(l.usr.iter().map(|n| (n.clone(), Some("USER"))));
            v
        }
        None => ctx
            .names
            .libl
            .iter()
            .filter_map(|id| {
                ops::read_header(txn, *id)
                    .ok()
                    .map(|(_, h)| (h.name.to_string(), None))
            })
            .collect(),
    };
    for (i, (name, part)) in parts.iter().enumerate() {
        let name = name.trim();
        let id = library(txn, ctx, name);
        out.row();
        out.int("ORDINAL_POSITION", i + 1);
        out.text(
            "SCHEMA_NAME",
            id.map_or_else(
                || name.to_string(),
                |id| crate::exec::names::schema_name(txn, id),
            ),
        );
        out.text("SYSTEM_SCHEMA_NAME", name);
        if let Some(p) = part {
            out.text("TYPE", *p);
        }
        // Libraries are in the system ASP: no independent ASP number.
        out.int("IASP_NUMBER", 0);
        if let Some(id) = id
            && let Ok((_, h)) = ops::read_header(txn, id)
        {
            out.text_or_null("TEXT_DESCRIPTION", &ccsid::decode_trimmed(&h.text));
        }
    }
    Ok(())
}

/// A library by system name.
pub(super) fn library(txn: &mut Txn<'_>, ctx: &SqlContext, name: &str) -> Option<ObjectId> {
    if name == "QSYS" {
        return Some(ctx.qsys);
    }
    let n = Name::parse(name).ok()?;
    ops::find_library(txn, ctx.qsys, n).ok().map(|f| f.id)
}
