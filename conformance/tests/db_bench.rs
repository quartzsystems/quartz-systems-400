// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `dbbench` loop against an in-memory store on the host: insert,
//! read by key and update through the open data path, a keyed PF with
//! two logical files kept up. Ignored by default; run with
//! `cargo test --release -p conformance --test db_bench -- --ignored --nocapture`
//! and `QS400_DB_BENCH=N` for the row count (default 20,000).

use std::time::Instant;

use conformance::db::Kit;
use conformance::db::local::{JOB, Local};
use db::file::{self, CreateFileOpts, Size};
use db::odp::{KeyArg, Odp, OpenOpts, Usage};
use db::{Member, Value as DbValue};
use dds::{CompileOpts, MapRefs, NoRefs, Source, compile_lf, compile_pf};
use qdecimal::Decimal;
use qsname::Name;

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
    "     A          R BENCHR                    PFILE(QSTEST/BENCH)",
    "     A          K CODE",
];
const LF_AMT: &[&str] = &[
    "     A          R BENCHR                    PFILE(QSTEST/BENCH)",
    "     A          K AMT",
];
const BATCH: u64 = 1000;

fn record(m: &Member, i: u64, note: &str) -> (Vec<u8>, Vec<u8>) {
    let fmt = m.format();
    let mut rec = vec![0u8; usize::try_from(fmt.format.record_len).unwrap()];
    let nulls = vec![0u8; usize::from(fmt.null_map_len)];
    let f = &fmt.format.fields;
    let values = [
        DbValue::Num(Decimal::new(i128::from(i), 0).unwrap()),
        DbValue::Char(ccsid::to_ebcdic(&format!("C{:04}", i % 997)).unwrap()),
        DbValue::Num(Decimal::new(i128::from(i % 10_000) * 7, 2).unwrap()),
        DbValue::Char(ccsid::to_ebcdic("2026-01-01").unwrap()),
        DbValue::Char(ccsid::to_ebcdic(note).unwrap()),
    ];
    for (i, v) in values.iter().enumerate() {
        db::record::write_field(&f[i], &mut rec, v).unwrap();
    }
    (rec, nulls)
}

#[allow(clippy::cast_precision_loss)]
fn rate(what: &str, n: u64, start: Instant) -> String {
    let secs = start.elapsed().as_secs_f64().max(1e-9);
    format!(
        "{what:<7} {n} rows in {secs:.2} s = {:.0} rows/s",
        n as f64 / secs
    )
}

#[test]
#[ignore = "a bench, not a check"]
#[allow(clippy::too_many_lines)]
fn dbbench_in_memory() {
    let n: u64 = std::env::var("QS400_DB_BENCH")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20_000);
    let kit = Kit::locate();
    let mut local = Local::build(&kit).unwrap();
    let lib = local.lib;
    let opts = CreateFileOpts {
        now: conformance::db::local::NOW,
        size: Size::NOMAX,
        ..CreateFileOpts::default()
    };
    let id = local
        .txn(|txn, _| {
            let (src, _) = Source::from_text(&PF.join("\n"));
            let comp = compile_pf(&src, &NoRefs, &CompileOpts::default());
            let def = comp
                .definition
                .unwrap_or_else(|| panic!("{}", comp.listing));
            let id = file::create_pf(txn, lib, Name::parse("BENCH").unwrap(), &def, &opts)
                .map_err(|e| e.to_string())?;
            let mut refs = MapRefs::default();
            refs.insert_pf("QSTEST", "BENCH", &def);
            for (name, text) in [("BENCHCODE", LF_CODE), ("BENCHAMT", LF_AMT)] {
                let (src, _) = Source::from_text(&text.join("\n"));
                let comp = compile_lf(&src, &refs, &CompileOpts::default());
                let lf = comp
                    .definition
                    .unwrap_or_else(|| panic!("{}", comp.listing));
                file::create_lf(
                    txn,
                    lib,
                    Name::parse(name).unwrap(),
                    &lf,
                    &mut |_| Ok(id),
                    &opts,
                )
                .map_err(|e| e.to_string())?;
            }
            Ok(id)
        })
        .unwrap();
    let open = |txn: &mut store::txn::Txn<'_>, locks, usage| {
        Odp::open(
            txn,
            locks,
            id,
            OpenOpts {
                usage,
                keyed: usage != Usage::OUTPUT,
                ..OpenOpts::input(JOB)
            },
        )
        .unwrap()
    };
    let mut lines = Vec::new();
    let start = Instant::now();
    let mut i = 1;
    while i <= n {
        let end = (i + BATCH - 1).min(n);
        local
            .txn(|txn, ctx| {
                let mut o = open(txn, ctx.locks.clone(), Usage::OUTPUT);
                for k in i..=end {
                    let (rec, nulls) = record(o.member(), k, "inserted");
                    o.write(txn, &rec, &nulls).unwrap();
                }
                o.close(txn);
                Ok(())
            })
            .unwrap();
        i = end + 1;
    }
    lines.push(rate("insert", n, start));
    let start = Instant::now();
    local
        .txn(|txn, ctx| {
            let mut o = open(txn, ctx.locks.clone(), Usage::INPUT);
            for k in 1..=n {
                let key = KeyArg::one(DbValue::Num(Decimal::new(i128::from(k), 0).unwrap()));
                assert!(o.chain(txn, &key, false).unwrap().is_some(), "row {k}");
            }
            o.close(txn);
            Ok(())
        })
        .unwrap();
    lines.push(rate("read", n, start));
    let start = Instant::now();
    let mut i = 1;
    while i <= n {
        let end = (i + BATCH - 1).min(n);
        local
            .txn(|txn, ctx| {
                let mut o = open(txn, ctx.locks.clone(), Usage::UPDATE);
                for k in i..=end {
                    let key = KeyArg::one(DbValue::Num(Decimal::new(i128::from(k), 0).unwrap()));
                    if o.chain(txn, &key, true).unwrap().is_some() {
                        let (rec, nulls) = record(o.member(), k, "updated");
                        o.update(txn, &rec, &nulls).unwrap();
                    }
                }
                o.close(txn);
                Ok(())
            })
            .unwrap();
        i = end + 1;
    }
    lines.push(rate("update", n, start));
    for l in &lines {
        println!("{l}");
    }
}
