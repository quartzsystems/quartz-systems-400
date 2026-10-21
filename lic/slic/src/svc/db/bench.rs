// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `dbbench N`: insert, read and update throughput on a five-field
//! physical file with two logical files, in library QDBBENCH.

use std::fmt::Write as _;
use std::time::Instant;

use db::file::{self, CreateFileOpts, Size};
use db::odp::{KeyArg, Odp, OpenOpts, Usage};
use db::{Member, Value as DbValue};
use dds::{CompileOpts, MapRefs, NoRefs, Source, compile_lf, compile_pf};
use qdecimal::Decimal;
use qsname::{MemberSpec, Name};
use sql::SqlContext;
use sql::ast::ObjectName;
use store::obj::{CreateParams, ObjectId, ops, types};
use store::txn::Txn;

use super::{JOB, db, db_err, now, with_txn};
use crate::ipl::system::System;

/// Open for the bench, under the job's commit definition when there is one.
fn open(txn: &mut Txn<'_>, ctx: &SqlContext, id: ObjectId, usage: Usage) -> Result<Odp, String> {
    let mut o = Odp::open(
        txn,
        ctx.locks.clone(),
        id,
        OpenOpts {
            usage,
            keyed: usage != Usage::OUTPUT,
            commit: ctx.session.commit_def().is_some(),
            ..OpenOpts::input(JOB)
        },
    )
    .map_err(|e| db_err(&e))?;
    if let Some(c) = ctx.session.commit_def() {
        o.set_commit(c.clone());
    }
    Ok(o)
}

/// Commit the batch when under commitment control.
fn commit_batch(txn: &mut Txn<'_>, ctx: &mut SqlContext) -> Result<(), String> {
    if ctx.session.commit_def().is_some() {
        ctx.session.commit(txn, None).map_err(|e| db_err(&e))?;
    }
    Ok(())
}

const LIB: &str = "QDBBENCH";
/// Rows per transaction.
const BATCH: u64 = 1000;

// One line per DDS record: a string continuation would strip the
// leading blanks of the first line.
const PF: &[&str] = &[
    "     A                                      UNIQUE",
    "     A          R BENCHR",
    "     A            ID             9P 0",
    "     A            CODE          10A",
    "     A            AMT           11P 2",
    "     A            WHEN            L         ALWNULL",
    "     A            NOTE          50A",
    "     A          K ID",
];
const LF_CODE: &[&str] = &[
    "     A          R BENCHR                    PFILE(QDBBENCH/BENCH)",
    "     A          K CODE",
];
const LF_AMT: &[&str] = &[
    "     A          R BENCHR                    PFILE(QDBBENCH/BENCH)",
    "     A          K AMT",
];

/// The bench library and files, created on first use, the data cleared.
fn prepare(txn: &mut Txn<'_>, ctx: &SqlContext) -> Result<ObjectId, String> {
    let lib_name = Name::parse(LIB).expect("valid");
    let lib = match ops::find_library(txn, ctx.qsys, lib_name) {
        Ok(f) => f.id,
        Err(_) => ops::create(
            txn,
            &CreateParams {
                library: ctx.qsys,
                name: lib_name,
                type_code: types::LIB,
                attribute: String::new(),
                text: "Database bench".into(),
                owner: "QSECOFR".into(),
                system: "QS400".into(),
                now: now(),
                by_system: false,
            },
        )
        .map_err(|e| e.to_string())?,
    };
    let bench = ObjectName::qualified(LIB, "BENCH");
    let opts = CreateFileOpts {
        now: now(),
        size: Size::NOMAX,
        ..CreateFileOpts::default()
    };
    let id = if let Ok(id) = ctx.resolve_file(txn, &bench) {
        db::util::clrpfm(txn, JOB, id, MemberSpec::First).map_err(|e| db_err(&e))?;
        id
    } else {
        let (src, _) = Source::from_text(&PF.join("\n"));
        let comp = compile_pf(&src, &NoRefs, &CompileOpts::default());
        let def = comp
            .definition
            .ok_or_else(|| format!("bench PF:\n{}", comp.listing))?;
        let id = file::create_pf(txn, lib, Name::parse("BENCH").expect("valid"), &def, &opts)
            .map_err(|e| db_err(&e))?;
        let mut refs = MapRefs::default();
        refs.insert_pf(LIB, "BENCH", &def);
        for (name, text) in [("BENCHCODE", LF_CODE), ("BENCHAMT", LF_AMT)] {
            let (src, _) = Source::from_text(&text.join("\n"));
            let comp = compile_lf(&src, &refs, &CompileOpts::default());
            let lf = comp
                .definition
                .ok_or_else(|| format!("bench LF {name}:\n{}", comp.listing))?;
            file::create_lf(
                txn,
                lib,
                Name::parse(name).expect("valid"),
                &lf,
                &mut |_| Ok(id),
                &opts,
            )
            .map_err(|e| db_err(&e))?;
        }
        id
    };
    Ok(id)
}

fn record(m: &Member, i: u64, note: &str) -> (Vec<u8>, Vec<u8>) {
    let fmt = m.format();
    let mut rec = vec![0u8; usize::try_from(fmt.format.record_len).expect("fits")];
    let nulls = vec![0u8; usize::from(fmt.null_map_len)];
    let f = &fmt.format.fields;
    let set = |rec: &mut Vec<u8>, i: usize, v: &DbValue| {
        db::record::write_field(&f[i], rec, v).expect("fits");
    };
    set(
        &mut rec,
        0,
        &DbValue::Num(Decimal::new(i128::from(i), 0).expect("fits")),
    );
    set(
        &mut rec,
        1,
        &DbValue::Char(ccsid::to_ebcdic(&format!("C{:04}", i % 997)).expect("ascii")),
    );
    set(
        &mut rec,
        2,
        &DbValue::Num(Decimal::new(i128::from(i % 10_000) * 7, 2).expect("fits")),
    );
    set(
        &mut rec,
        3,
        &DbValue::Char(ccsid::to_ebcdic("2026-01-01").expect("ascii")),
    );
    set(
        &mut rec,
        4,
        &DbValue::Char(ccsid::to_ebcdic(note).expect("ascii")),
    );
    (rec, nulls)
}

#[allow(clippy::cast_precision_loss)]
fn rate(n: u64, start: Instant) -> String {
    let secs = start.elapsed().as_secs_f64().max(1e-9);
    format!("{n} rows in {secs:.2} s = {:.0} rows/s", n as f64 / secs)
}

/// `dbbench N`.
pub fn dbbench(system: &mut System, args: &[&str]) -> Result<String, String> {
    let n: u64 = match args {
        [n] => n.parse().map_err(|_| "Usage: dbbench N")?,
        _ => return Err("Usage: dbbench N".into()),
    };
    let (store, ctx) = db(system)?;
    let id = with_txn(store, |txn| prepare(txn, ctx))?;
    let mut out = String::new();
    // Insert through the open data path, BATCH rows per transaction.
    let start = Instant::now();
    let mut i = 1;
    while i <= n {
        let end = (i + BATCH - 1).min(n);
        with_txn(store, |txn| {
            let mut o = open(txn, ctx, id, Usage::OUTPUT)?;
            for k in i..=end {
                let (rec, nulls) = record(o.member(), k, "inserted");
                o.write(txn, &rec, &nulls).map_err(|e| db_err(&e))?;
            }
            o.close(txn);
            commit_batch(txn, ctx)
        })?;
        i = end + 1;
    }
    let _ = writeln!(out, "insert  {}", rate(n, start));
    // Read every row by key.
    let start = Instant::now();
    with_txn(store, |txn| {
        let mut o = open(txn, ctx, id, Usage::INPUT)?;
        let mut missing = 0u64;
        for k in 1..=n {
            let key = KeyArg::one(DbValue::Num(Decimal::new(i128::from(k), 0).expect("fits")));
            if o.chain(txn, &key, false).map_err(|e| db_err(&e))?.is_none() {
                missing += 1;
            }
        }
        o.close(txn);
        if missing > 0 {
            return Err(format!("{missing} rows not found by key"));
        }
        commit_batch(txn, ctx)
    })?;
    let _ = writeln!(out, "read    {}", rate(n, start));
    // Update every row, BATCH per transaction.
    let start = Instant::now();
    let mut i = 1;
    while i <= n {
        let end = (i + BATCH - 1).min(n);
        with_txn(store, |txn| {
            let mut o = open(txn, ctx, id, Usage::UPDATE)?;
            for k in i..=end {
                let key = KeyArg::one(DbValue::Num(Decimal::new(i128::from(k), 0).expect("fits")));
                if o.chain(txn, &key, true).map_err(|e| db_err(&e))?.is_some() {
                    let (rec, nulls) = record(o.member(), k, "updated");
                    o.update(txn, &rec, &nulls).map_err(|e| db_err(&e))?;
                }
            }
            o.close(txn);
            commit_batch(txn, ctx)
        })?;
        i = end + 1;
    }
    let _ = writeln!(out, "update  {}", rate(n, start));
    // The logical files kept up.
    with_txn(store, |txn| {
        for lf in ["BENCHCODE", "BENCHAMT"] {
            let lid = ctx
                .resolve_file(txn, &ObjectName::qualified(LIB, lf))
                .map_err(|e| e.to_string())?;
            let m = Member::open_by(txn, lid, MemberSpec::First).map_err(|e| db_err(&e))?;
            let _ = writeln!(out, "{lf:<9} {} path entries", m.attr().path_entries);
        }
        Ok(())
    })?;
    Ok(out)
}
