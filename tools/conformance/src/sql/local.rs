// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A case run in memory: a fresh store with `QSTEST` as `{lib}`, `*SQL`
//! naming, current schema `QSTEST`, each statement its own transaction
//! (autocommit), the outcome written in the kit's form.

use std::fmt::Write as _;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use qsname::Name;
use sql::{Naming, Outcome, SqlContext, prepare};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, types};
use store::{FormatParams, Store, StoreConfig};

use super::case::{self, Case};
use super::render;

/// The library a case writes in here.
pub const LIBRARY: &str = "QSTEST";
/// The clock: 2026-01-01 00:00:00 UTC.
pub const NOW: u64 = 1_767_225_600;

fn s(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Run `c` on a fresh store; the outcome text.
pub fn run(c: &Case) -> Result<String, String> {
    let dev = Arc::new(MemBlockDev::with_seed(65536, 7));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 512,
        now: NOW,
    };
    let mut store =
        Store::format(vec![Box::new(dev) as Box<dyn BlockDev>], &params, StoreConfig::default()).map_err(s)?;
    let mut txn = store.begin().map_err(s)?;
    let qsys = ops::bootstrap(&mut txn, NOW, "QS400").map_err(s)?;
    let lib = ops::create(
        &mut txn,
        &CreateParams {
            library: qsys,
            name: Name::parse(LIBRARY).expect("valid"),
            type_code: types::LIB,
            attribute: String::new(),
            text: "SQL conformance kit".into(),
            owner: "QSECOFR".into(),
            system: "QS400".into(),
            now: NOW,
            by_system: false,
        },
    )
    .map_err(s)?;
    txn.commit().map_err(s)?;
    let mut ctx = SqlContext::new(qsys, JobHandle(1), NameContext { curlib: Some(lib), libl: vec![lib] });
    ctx.now = Some(NOW);
    ctx.current_schema = Some(Name::parse(LIBRARY).expect("valid"));
    ctx.user = "QSECOFR".into();

    let mut out = String::from("-- captured from qs400\n");
    for st in &c.statements {
        let text = st.replace("{lib}", LIBRARY);
        let _ = writeln!(out, "> {}", case::first(st));
        let prepared = prepare(&text, Naming::Sql);
        let mut txn = store.begin().map_err(s)?;
        let (sqlca, outcome) = match &prepared {
            Ok(p) => p.run(&mut txn, &mut ctx, &[]),
            Err(e) => (sql::Sqlca::from_error(e), None),
        };
        if sqlca.sqlcode < 0 {
            txn.abort();
        } else {
            txn.commit().map_err(s)?;
        }
        let count = match &outcome {
            Some(Outcome::Count(n)) if case::changes(st) => Some(*n),
            _ if case::changes(st) && sqlca.sqlcode >= 0 => Some(u64::try_from(sqlca.sqlerrd[2]).unwrap_or(0)),
            _ => None,
        };
        // JDBC reports warnings apart; the status line has 0 for them. A
        // query with no rows is no warning over JDBC.
        let empty_query = sqlca.sqlcode == 100 && matches!(outcome, Some(Outcome::Rows(_)));
        if empty_query {
            out.push_str(&render::status(0, "00000", "-", count));
            out.push('\n');
        } else if sqlca.sqlcode > 0 {
            out.push_str(&render::status(0, "00000", "-", count));
            out.push('\n');
            let _ = writeln!(out, "warning {} {} {}", sqlca.sqlcode, sqlca.sqlstate, sqlca.msgid());
        } else {
            let msgid = if sqlca.sqlcode == 0 { "-".to_string() } else { sqlca.msgid() };
            out.push_str(&render::status(sqlca.sqlcode, &sqlca.sqlstate, &msgid, count));
            out.push('\n');
        }
        if let Some(Outcome::Rows(rs)) = &outcome {
            out.push_str(&render::rows(rs));
        }
    }
    Ok(out.replace(LIBRARY, "{lib}"))
}
