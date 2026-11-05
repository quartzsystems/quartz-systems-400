// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSYS2.OBJECT_STATISTICS(OBJECT_SCHEMA, OBJTYPELIST, OBJECT_NAME)`:
//! the objects of a library or a list of them, from the object headers
//! as `DSPOBJD` reads them. Journaling and source attributes are not in
//! the header and are null.

use qsname::Name;
use store::obj::header::Domain;
use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use super::super::super::SqlContext;
use super::{Args, Out};
use crate::error::{Result, SqlError};

/// The libraries `schema` names: one, or a list's (`*LIBL`, `*USRLIBL`,
/// `*CURLIB`, `*ALLUSR`, `*ALL`).
fn libraries(txn: &mut Txn<'_>, ctx: &SqlContext, schema: &str) -> Result<Vec<ObjectId>> {
    let all = |txn: &mut Txn<'_>| -> Result<Vec<ObjectId>> {
        let mut v: Vec<ObjectId> = ops::list(txn, ctx.qsys, Some(types::LIB))?.into_iter().map(|(_, id)| id).collect();
        v.insert(0, ctx.qsys);
        Ok(v)
    };
    Ok(match schema {
        "*LIBL" => ctx.names.libl.clone(),
        "*CURLIB" => ctx.names.curlib.into_iter().collect(),
        "*USRLIBL" => {
            let host = ctx.host.as_deref().and_then(super::host::SystemHost::library_list);
            match host {
                Some(l) => l.usr.iter().filter_map(|n| super::libl::library(txn, ctx, n)).collect(),
                None => ctx.names.libl.clone(),
            }
        }
        "*ALL" => all(txn)?,
        "*ALLUSR" => {
            let mut v = Vec::new();
            for id in all(txn)? {
                let (_, h) = ops::read_header(txn, id)?;
                let n = h.name.to_string();
                if !n.starts_with('Q') || matches!(n.as_str(), "QGPL" | "QTEMP" | "QUSRSYS" | "QS36F") {
                    v.push(id);
                }
            }
            v
        }
        name => super::libl::library(txn, ctx, name).into_iter().collect(),
    })
}

/// Whether `name` matches `filter`: a name, a generic name, `*ALL`.
fn matches(filter: Option<&str>, name: &str) -> bool {
    match filter {
        None | Some("*ALL") => true,
        Some(f) => match f.strip_suffix('*') {
            Some(p) => name.starts_with(p),
            None => f == name,
        },
    }
}

fn blank_or(b: &[u8]) -> String {
    ccsid::decode_trimmed(b)
}

/// The rows.
pub fn rows(txn: &mut Txn<'_>, ctx: &SqlContext, args: &Args, out: &mut Out) -> Result<()> {
    let schema = args.word("OBJECT_SCHEMA").unwrap_or_default();
    let list = args.word("OBJTYPELIST").unwrap_or_else(|| "*ALL".into());
    let wanted: Vec<String> = list.split([' ', ',']).filter(|s| !s.is_empty()).map(str::to_string).collect();
    let all_types = wanted.iter().any(|t| t == "*ALL" || t == "*ALLSIMPLE");
    let mut codes = Vec::new();
    for t in &wanted {
        if t == "*ALL" || t == "*ALLSIMPLE" {
            continue;
        }
        let ti = types::type_by_name(t).ok_or_else(|| SqlError::Coded { code: -443, state: "42616", what: format!("{t} is not an object type") })?;
        codes.push(ti.code);
    }
    if schema.starts_with('*') && codes.contains(&types::LIB) {
        // As pub400: libraries are listed from QSYS only.
        return Err(SqlError::Coded { code: -443, state: "42616", what: "*LIB objects are listed with OBJECT_SCHEMA QSYS".into() });
    }
    let name = args.word("OBJECT_NAME");
    for lib in libraries(txn, ctx, &schema)? {
        let Ok((_, lh)) = ops::read_header(txn, lib) else { continue };
        let objs = match ops::list(txn, lib, None) {
            Ok(v) => v,
            Err(store::StoreError::NotAuthorized { .. }) => continue,
            Err(e) => return Err(e.into()),
        };
        let lib_name = lh.name.to_string();
        let lib_sql = crate::exec::names::schema_name(txn, lib);
        for (key, id) in objs {
            if !all_types && !codes.contains(&key.type_code) {
                continue;
            }
            let Some(ti) = types::type_by_code(key.type_code) else { continue };
            // Members and spooled files are not objects a list shows.
            if matches!(ti.name, "*MBR" | "*SPLF" | "*SQLINF") {
                continue;
            }
            if !matches(name.as_deref(), &key.name.to_string()) {
                continue;
            }
            let Ok((_, h)) = ops::read_header(txn, id) else { continue };
            row(txn, out, id, &h, ti.name, &lib_name, &lib_sql);
        }
    }
    Ok(())
}

fn row(txn: &mut Txn<'_>, out: &mut Out, id: ObjectId, h: &store::obj::header::ObjectHeader, ty: &str, lib: &str, lib_sql: &str) {
    out.row();
    let n: Name = h.name;
    out.text("OBJNAME", n.to_string());
    out.text("OBJTYPE", ty);
    out.text("OBJOWNER", blank_or(&h.owner));
    out.text_or_null("OBJDEFINER", &blank_or(&h.creator));
    out.stamp("OBJCREATED", h.created);
    out.int("OBJSIZE", (h.size_pages + 1) * 4096);
    let text = blank_or(&h.text);
    out.text_or_null("OBJTEXT", &text);
    out.text_or_null("TEXT", &text);
    let sql = if ty == "*FILE" { crate::exec::names::sql_name(txn, id) } else { n.to_string() };
    out.text("OBJLONGNAME", sql);
    out.stamp("LAST_USED_TIMESTAMP", h.last_used);
    out.yes("LAST_USED_OBJECT", h.last_used != 0);
    out.int("DAYS_USED_COUNT", h.days_used);
    out.stamp("LAST_RESET_TIMESTAMP", h.reset_date);
    out.int("IASP_NUMBER", 0);
    out.text("IASP_NAME", "*SYSBAS");
    out.text_or_null("OBJATTRIBUTE", &blank_or(&h.attribute));
    out.text("OBJLONGSCHEMA", lib_sql);
    out.text("OBJLIB", lib);
    if ty == "*FILE" {
        let kind = crate::exec::names::kind_of(txn, id).map(|k| match k {
            db::sqlinf::Kind::Table => "TABLE",
            db::sqlinf::Kind::View => "VIEW",
            db::sqlinf::Kind::Index => "INDEX",
            db::sqlinf::Kind::Alias => "ALIAS",
            db::sqlinf::Kind::Mqt => "MQT",
            _ => "",
        });
        if let Some(k) = kind.filter(|k| !k.is_empty()) {
            out.text("SQL_OBJECT_TYPE", k);
        }
    }
    out.stamp("CHANGE_TIMESTAMP", h.changed);
    out.yes("USER_CHANGED", h.changed > h.created);
    out.text_or_null("CREATED_SYSTEM", &blank_or(&h.system));
    out.text("CREATED_SYSTEM_VERSION", h.release.to_string());
    out.text_or_null("PRIMARY_GROUP", &blank_or(&h.primary_group));
    out.yes("STORAGE_FREED", false);
    out.text("OBJECT_DOMAIN", if h.domain == Domain::System { "*SYSTEM" } else { "*USER" });
    let audit = blank_or(&h.audit);
    out.text_or_null("OBJECT_AUDIT", &audit);
    out.yes("OBJECT_SIGNED", h.signature.is_some());
    out.stamp("SAVE_TIMESTAMP", h.last_saved);
    out.stamp("RESTORE_TIMESTAMP", h.last_restored);
    if h.last_saved != 0 {
        out.text_or_null("SAVE_COMMAND", &blank_or(&h.save.command));
        out.text_or_null("SAVE_DEVICE", &blank_or(&h.save.device));
        out.text_or_null("SAVE_FILE_NAME", &blank_or(&h.save.file));
        out.text_or_null("SAVE_FILE_LIBRARY", &blank_or(&h.save.file_lib));
        out.text_or_null("SAVE_VOLUME", &blank_or(&h.save.volume));
        out.text_or_null("SAVE_LABEL", &blank_or(&h.save.label));
        out.int("SAVE_SEQUENCE_NUMBER", h.save.seq);
        out.int("LAST_SAVE_SIZE", h.save.size);
    }
}
