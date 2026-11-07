// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `RENAME TABLE` and `RENAME INDEX`: the SQL name, the system name or
//! both, as the primary oracle settled (a long name keeps a generated
//! system name of its prefix, else gets a new one).

use db::sqlinf::Kind;
use qsname::Name;
use store::obj::ops;
use store::txn::Txn;

use super::{SqlContext, names};
use crate::ast::Rename;
use crate::error::{Result, SqlError};

/// Whether `sys` is the name IBM would generate for `sql`'s prefix: its
/// first five characters and five digits.
fn generated_for(sys: Name, sql: &str) -> bool {
    let s = sys.to_string();
    let five: String = sql.chars().take(5).collect();
    s.len() == 10 && s.starts_with(&five) && s[5..].chars().all(|c| c.is_ascii_digit())
}

/// `RENAME`.
pub fn run(txn: &mut Txn<'_>, ctx: &mut SqlContext, r: &Rename) -> Result<()> {
    let id = ctx.resolve_file(txn, &r.name)?;
    let (_, h) = ops::read_header(txn, id)?;
    let lib = h.library;
    let old_sys = h.name;
    let kind = names::kind_of(txn, id).unwrap_or(if r.index { Kind::Index } else { Kind::Table });
    let old_sql = names::sql_name(txn, id);
    if !super::view::dependents(txn, ctx, id)?.is_empty() {
        // BLOCKED(rel): the views over it keep its name in their query.
        return Err(SqlError::Unsupported { what: "RENAME of a table or view other views read".into() });
    }
    let (new_sys, new_sql): (Name, Option<String>) = match (&r.to, &r.system_name) {
        // `TO SYSTEM NAME x`: the system name only; a table whose SQL name
        // was its system name keeps the old one as its SQL name.
        (None, Some(s)) => {
            let n = names::direct(s).ok_or_else(|| SqlError::SystemName { name: s.value.clone() })?;
            (n, (old_sql != n.to_string()).then_some(old_sql.clone()))
        }
        (Some(to), sys) => {
            names::check_length(to)?;
            names::check_for_system(to, sys.as_ref())?;
            match (names::direct(to), sys) {
                (Some(n), _) => (n, None),
                (None, Some(s)) => (names::direct(s).ok_or_else(|| SqlError::SystemName { name: s.value.clone() })?, Some(to.value.clone())),
                (None, None) if generated_for(old_sys, &to.value) => (old_sys, Some(to.value.clone())),
                (None, None) => (names::generate(txn, lib, store::obj::types::FILE, &to.value, &|_| false)?, Some(to.value.clone())),
            }
        }
        (None, None) => return Err(SqlError::Unsupported { what: "RENAME without TO".into() }),
    };
    if let Some(s) = &new_sql
        && *s != old_sql
        && names::find(txn, lib, kind.space(), s)?.is_some()
    {
        return Err(SqlError::Exists { name: s.clone(), kind: "*FILE" });
    }
    if new_sys != old_sys {
        ops::rename(txn, id, new_sys, ctx.clock()).map_err(|e| match e {
            store::StoreError::AlreadyExists { .. } => SqlError::Exists { name: new_sys.to_string(), kind: "*FILE" },
            e => e.into(),
        })?;
    }
    names::rename(txn, lib, id, kind, new_sql.as_deref(), ctx.clock())?;
    Ok(())
}
