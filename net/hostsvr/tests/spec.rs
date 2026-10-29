// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `docs/HOSTSVR.md` against the code: every table after a
//! `<!-- consts: KEY -->` marker lists exactly the constants of KEY, and
//! every table after `<!-- layout: KEY -->` the fields of that template
//! at the same offsets and sizes.

use std::collections::BTreeMap;
use std::path::Path;

use hostsvr::database::{
    data::DataHeader,
    format::{FieldRecord, FormatHeader},
    param::ServerAttributes,
    sqlca::Sqlca,
};
use hostsvr::template::{Slot, Template};
use hostsvr::{central, common, cp, database, dtaq, file, netprt, rmtcmd, signon};

fn consts() -> BTreeMap<&'static str, Vec<(String, u64)>> {
    let w = |t: &[(&str, u16)]| {
        t.iter()
            .map(|(n, v)| ((*n).to_string(), u64::from(*v)))
            .collect::<Vec<_>>()
    };
    let w32 = |t: &[(&str, u32)]| {
        t.iter()
            .map(|(n, v)| ((*n).to_string(), u64::from(*v)))
            .collect::<Vec<_>>()
    };
    BTreeMap::from([
        ("cp::signon", w(cp::signon::ALL)),
        ("cp::rmtcmd", w(cp::rmtcmd::ALL)),
        ("cp::central", w(cp::central::ALL)),
        ("cp::db_request", w(cp::db_request::ALL)),
        ("cp::db_reply", w(cp::db_reply::ALL)),
        ("cp::dtaq", w(cp::dtaq::ALL)),
        ("cp::file", w(cp::file::ALL)),
        ("common::id", w(common::id::ALL)),
        ("signon::id", w(signon::id::ALL)),
        ("central::id", w(central::id::ALL)),
        ("rmtcmd::id", w(rmtcmd::id::ALL)),
        ("database::id", w(database::id::ALL)),
        ("database::ors", w32(database::ors::ALL)),
        ("dtaq::id", w(dtaq::id::ALL)),
        ("file::id", w(file::id::ALL)),
        ("netprt::id", w(netprt::id::ALL)),
        ("netprt::action", w(netprt::action::ALL)),
    ])
}

fn layouts() -> BTreeMap<String, Vec<Slot>> {
    let mut m = BTreeMap::new();
    let mut add = |module: &str, list: Vec<(hostsvr::Dir, u16, &'static str, Vec<Slot>)>| {
        for (_, _, name, slots) in list.into_iter().filter(|l| !l.3.is_empty()) {
            m.insert(format!("{module}::{name}"), slots);
        }
    };
    add("common", common::Template::layouts());
    add("signon", signon::Template::layouts());
    add("central", central::Template::layouts());
    add("rmtcmd", rmtcmd::Template::layouts());
    add("database", database::Template::layouts());
    m.insert("netprt::NpTemplate".into(), netprt::NpTemplate::layout());
    m.insert(
        "database::ServerAttributes".into(),
        ServerAttributes::layout(),
    );
    m.insert("database::Sqlca".into(), Sqlca::layout());
    m.insert("database::FormatHeader".into(), FormatHeader::layout());
    m.insert("database::FieldRecord".into(), FieldRecord::layout());
    m.insert("database::DataHeader".into(), DataHeader::layout());
    m
}

fn doc() -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/HOSTSVR.md"))
        .expect("docs/HOSTSVR.md")
}

/// The table rows (cells, trimmed, backticks removed) after each marker.
fn tables(doc: &str, kind: &str) -> BTreeMap<String, Vec<Vec<String>>> {
    let mut out = BTreeMap::new();
    let lines: Vec<&str> = doc.lines().collect();
    let prefix = format!("<!-- {kind}: ");
    for (i, l) in lines.iter().enumerate() {
        let Some(key) = l
            .trim()
            .strip_prefix(&prefix)
            .and_then(|r| r.strip_suffix(" -->"))
        else {
            continue;
        };
        let rows: Vec<Vec<String>> = lines[i + 1..]
            .iter()
            .skip_while(|l| l.trim().is_empty())
            .take_while(|l| l.trim_start().starts_with('|'))
            .skip(2)
            .map(|l| {
                l.trim()
                    .trim_matches('|')
                    .split('|')
                    .map(|c| c.trim().replace('`', ""))
                    .collect()
            })
            .collect();
        assert!(
            out.insert(key.to_string(), rows).is_none(),
            "{kind} {key} appears twice"
        );
    }
    out
}

fn hex(s: &str) -> u64 {
    u64::from_str_radix(s.trim_start_matches("0x").replace('_', "").as_str(), 16)
        .unwrap_or_else(|_| panic!("not hex: {s}"))
}

#[test]
fn code_points_and_ids_match_the_doc() {
    let doc = tables(&doc(), "consts");
    let code = consts();
    for (key, list) in &code {
        let rows = doc
            .get(*key)
            .unwrap_or_else(|| panic!("docs/HOSTSVR.md has no consts table for {key}"));
        let mut d: Vec<(String, u64)> = rows.iter().map(|r| (r[1].clone(), hex(&r[0]))).collect();
        let mut c = list.clone();
        d.sort();
        c.sort();
        assert_eq!(d, c, "consts table {key} differs from the code");
    }
    for key in doc.keys() {
        assert!(
            code.contains_key(key.as_str()),
            "docs/HOSTSVR.md lists unknown consts {key}"
        );
    }
}

#[test]
fn template_layouts_match_the_doc() {
    let doc = tables(&doc(), "layout");
    let code = layouts();
    for (key, slots) in &code {
        let rows = doc
            .get(key)
            .unwrap_or_else(|| panic!("docs/HOSTSVR.md has no layout table for {key}"));
        let d: Vec<(usize, usize, String)> = rows
            .iter()
            .map(|r| (r[0].parse().unwrap(), r[1].parse().unwrap(), r[2].clone()))
            .collect();
        let c: Vec<(usize, usize, String)> = slots
            .iter()
            .map(|s| (s.offset, s.size, s.name.to_string()))
            .collect();
        assert_eq!(d, c, "layout {key} differs from the code");
    }
    for key in doc.keys() {
        assert!(
            code.contains_key(key),
            "docs/HOSTSVR.md lists unknown layout {key}"
        );
    }
}

/// Print the tables the doc needs (`cargo test -p hostsvr --test spec -- --ignored --nocapture`).
#[test]
#[ignore = "prints the doc tables"]
fn print_tables() {
    for (key, list) in consts() {
        println!("<!-- consts: {key} -->\n\n| Value | Name |\n|---|---|");
        let width = if key == "database::ors" { 8 } else { 4 };
        for (n, v) in list {
            println!("| `0x{v:0width$X}` | `{n}` |");
        }
        println!();
    }
    for (key, slots) in layouts() {
        println!("<!-- layout: {key} -->\n\n| Offset | Size | Field |\n|---|---|---|");
        for s in slots {
            println!("| {} | {} | `{}` |", s.offset, s.size, s.name);
        }
        println!();
    }
}
