// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `ALTER PROCEDURE` and `ALTER FUNCTION`: the routine made again from
//! its stored `CREATE` text, the options given put before its body
//! (`ALTER`), or from a new definition (`REPLACE`); its specific name
//! stays, as on the primary.

use store::txn::Txn;

use super::create;
use crate::ast::{AlterRoutine, Ident, Naming, ObjectName, RoutineBody, RoutineKind, Statement};
use crate::error::{Result, SqlError};
use crate::exec::SqlContext;

/// A statement in either naming (the stored text keeps its own).
fn parse(text: &str, naming: Naming) -> Result<Statement> {
    crate::parse::parse(text, naming).or_else(|e| {
        let other = if naming == Naming::Sql {
            Naming::Sys
        } else {
            Naming::Sql
        };
        crate::parse::parse(text, other).map_err(|_| e)
    })
}

/// Run `ALTER PROCEDURE|FUNCTION`.
pub fn alter(txn: &mut Txn<'_>, ctx: &mut SqlContext, a: &AlterRoutine) -> Result<()> {
    let old = create::the_one(txn, ctx, &a.target)?;
    let def = &old.routine.def;
    let kw = if a.target.kind == RoutineKind::Procedure {
        "PROCEDURE"
    } else {
        "FUNCTION"
    };
    let text = if a.replace {
        let name = ObjectName::qualified(&old.schema, &def.name);
        format!("CREATE {kw} {name} {}", a.tail)
    } else {
        // The body as the stored statement writes it.
        let t = def.text.as_str();
        let body = match parse(t, ctx.naming)? {
            Statement::CreateRoutine(r) if matches!(r.body, RoutineBody::Sql(_)) => r.body_text,
            _ => String::new(),
        };
        let cut = if body.is_empty() {
            t.len()
        } else {
            t.rfind(body.as_str()).unwrap_or(t.len())
        };
        format!("{} {} {}", t[..cut].trim_end(), a.tail, &t[cut..])
    };
    let parsed = parse(&text, ctx.naming)?;
    let Statement::CreateRoutine(mut r) = parsed else {
        return Err(SqlError::Unsupported {
            what: "an ALTER of a routine whose definition is not a CREATE".into(),
        });
    };
    if r.kind != a.target.kind {
        return Err(SqlError::Coded {
            code: -104,
            state: "42601",
            what: format!("a {kw} cannot be replaced by another kind of routine"),
        });
    }
    r.replace = false;
    r.options.specific = Some(ObjectName {
        schema: r.name.schema.clone(),
        name: Ident::new(&def.specific),
    });
    create::remove(txn, ctx, &old)?;
    create::create(txn, ctx, &r)
}
