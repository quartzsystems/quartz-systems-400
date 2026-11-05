// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Where the records go: the source member named, and a result set for
//! the caller (`SQL0466`) with the member's columns.

use qdecimal::Decimal;
use qsname::{MemberSpec, Name};
use store::obj::{ops, types};
use store::txn::Txn;

use super::super::super::super::{ColumnDesc, SqlContext};
use super::super::Args;
use crate::error::{Result, SqlError};
use crate::value::Value;

/// The cursor the result set is left open on.
pub const CURSOR: &str = "QSQGENSQL";

/// The source file's data width: `SRCDTA CHAR(80)`.
const WIDTH: u32 = 80;

fn name(args: &Args, arg: &str) -> Result<Name> {
    let w = args.word(arg).unwrap_or_default();
    Name::parse(&w).map_err(|_| SqlError::Coded { code: -443, state: "22023", what: format!("{arg} {w} is not a name") })
}

/// Write the records to the source member the arguments name, making
/// the file and the member when they are missing. QTEMP is out of a
/// statement's reach: the records go to the result set alone.
pub fn keep(txn: &mut Txn<'_>, ctx: &SqlContext, args: &Args, records: &[String]) -> Result<()> {
    let lib = name(args, "DATABASE_SOURCE_FILE_LIBRARY_NAME")?;
    if lib.to_string() == "QTEMP" {
        // BLOCKED(sql): QTEMP is in the temporary ASP and a statement
        // runs in one ASP's transaction; the member is not written.
        return Ok(());
    }
    let file = name(args, "DATABASE_SOURCE_FILE_NAME")?;
    let member = name(args, "DATABASE_SOURCE_FILE_MEMBER")?;
    let append = args.word("REPLACE_OPTION").as_deref() == Some("0");
    let library = ops::find_library(txn, ctx.qsys, lib).map_err(|_| SqlError::ObjectNotFound { name: lib.to_string(), kind: "schema" })?.id;
    let found = ops::list(txn, library, Some(types::FILE))?.into_iter().find(|(k, _)| k.name == file).map(|(_, id)| id);
    let now = ctx.clock();
    let id = match found {
        Some(id) => id,
        None => db::file::create_srcpf(
            txn,
            library,
            file,
            WIDTH + 12,
            &db::file::CreateFileOpts { now, member: db::file::MemberOpt::None, ..db::file::CreateFileOpts::default() },
        )?,
    };
    let desc = db::file::describe(txn, id)?;
    if !desc.members.iter().any(|m| m.name == member) {
        db::file::add_member(txn, id, member, "", Some(Name::parse("SQL").expect("valid")), now)?;
    }
    let codec = ccsid::conv::Codec::of(desc.attr.ccsid).ok().flatten();
    let mut lines: Vec<Vec<u8>> = if append {
        db::source::records(txn, id, MemberSpec::Name(member))?.into_iter().map(|(_, _, d)| d).collect()
    } else {
        Vec::new()
    };
    for r in records {
        let mut b = Vec::new();
        for c in r.chars() {
            match &codec {
                Some(k) => {
                    k.encode(c, &mut b);
                }
                None => b.push(ccsid::byte_of(c).unwrap_or(ccsid::SPACE)),
            }
        }
        lines.push(b);
    }
    db::source::put_records(txn, ctx.job, id, MemberSpec::Name(member), &lines, now)?;
    Ok(())
}

/// Leave the records open for the caller as `SRCSEQ`, `SRCDAT` and
/// `SRCDTA`, and say so (`SQL0466`).
pub fn answer(ctx: &mut SqlContext, records: &[String]) {
    let d = db::record::datetime::Date::from_unix(ctx.clock());
    let date = Decimal::new(i128::from(d.year % 100) * 10_000 + i128::from(d.month) * 100 + i128::from(d.day), 0).expect("six digits");
    let rows: Vec<Vec<Value>> = records
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let seq = Decimal::new(i128::try_from(i + 1).unwrap_or(0) * 100, 2).expect("small");
            let mut srcdta: String = r.chars().take(WIDTH as usize).collect();
            while srcdta.chars().count() < WIDTH as usize {
                srcdta.push(' ');
            }
            vec![Value::Num(seq), Value::Num(date.clone()), Value::Str(srcdta)]
        })
        .collect();
    let columns = vec![
        ColumnDesc { name: "SRCSEQ".into(), data_type: "NUMERIC(6,2)".into(), nullable: false },
        ColumnDesc { name: "SRCDAT".into(), data_type: "NUMERIC(6,0)".into(), nullable: false },
        ColumnDesc { name: "SRCDTA".into(), data_type: "CHAR(80)".into(), nullable: false },
    ];
    crate::exec::cursor::open_rows(ctx, CURSOR, columns, rows);
    ctx.warning = Some(crate::sqlca::Warning::coded(466, "0100C", "1 result set is available from procedure GENERATE_SQL"));
}
