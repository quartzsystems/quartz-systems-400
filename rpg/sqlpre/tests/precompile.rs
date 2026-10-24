// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Statements precompiled against a small set of host variables and
//! tables: the engine's text, the host variables, the checks and the
//! generated lines.

use rpgsqlpre::{Env, HostInfo, Op, Options, Precompiled, Rows, Source, precompile};

struct Kit;

impl Env for Kit {
    fn host(&self, _scope: usize, name: &str) -> Option<HostInfo> {
        match name {
            "N" | "NM" | "IND" | "ID" | "REC.ID" => Some(HostInfo::default()),
            "REC" => Some(HostInfo { fields: 3, dim: 0 }),
            "ROWS" => Some(HostInfo { fields: 2, dim: 10 }),
            "INDS" => Some(HostInfo { fields: 0, dim: 3 }),
            _ => None,
        }
    }

    fn table(&mut self, _schema: Option<&str>, name: &str) -> bool {
        name == "T"
    }
}

fn run(stmts: &[(u32, &str)]) -> Precompiled {
    let srcs: Vec<Source> = stmts
        .iter()
        .map(|(line, text)| Source {
            line: *line,
            text: (*text).to_string(),
            scope: 0,
        })
        .collect();
    precompile(&srcs, &mut Kit, &Options::default())
}

#[test]
fn select_into_takes_out_the_into_list() {
    let p = run(&[(5, "select A, B into :nm, :n :ind from T where ID = :id")]);
    assert!(p.ok, "{:?}", p.diags);
    let s = &p.stmts[0];
    assert_eq!(s.desc.op, Op::Query);
    assert_eq!(s.desc.text, "select A, B from T where ID = ?");
    let outs: Vec<(&str, Option<&str>)> = s.outputs.iter().map(|h| (h.name.as_str(), h.ind.as_deref())).collect();
    assert_eq!(outs, vec![("NM", None), ("N", Some("IND"))]);
    assert_eq!(s.inputs[0].name, "ID");
}

#[test]
fn set_and_values_become_a_values_query() {
    let p = run(&[(1, "set :n = :id + 1, :nm = 'x'"), (2, "values 6 * 7 into :n")]);
    assert_eq!(p.stmts[0].desc.text, "VALUES ( ? + 1, 'x')");
    assert_eq!(p.stmts[0].outputs.len(), 2);
    assert_eq!(p.stmts[0].inputs[0].name, "ID");
    assert_eq!(p.stmts[1].desc.text, "values (6 * 7 )");
}

#[test]
fn a_structure_stands_for_its_subfields() {
    let p = run(&[(1, "insert into T values (:rec)")]);
    assert_eq!(p.stmts[0].desc.text, "insert into T values (?, ?, ?)");
    assert_eq!(p.stmts[0].n_in, 3);
}

#[test]
fn cursors_carry_their_declaration_to_open() {
    let p = run(&[
        (1, "declare c1 scroll cursor with hold for select A from T where ID > :id"),
        (2, "open c1"),
        (3, "fetch relative :n from c1 for 2 rows into :rows"),
        (4, "close c1"),
    ]);
    assert!(p.ok, "{:?}", p.diags);
    let open = &p.stmts[0];
    assert_eq!((open.desc.op, open.desc.hold, open.desc.scroll), (Op::Open, true, true));
    assert_eq!(open.desc.text, "select A from T where ID > ?");
    assert_eq!(open.inputs[0].name, "ID");
    let f = &p.stmts[1];
    assert_eq!((f.desc.orient.as_str(), f.desc.rows), ("RELATIVE ?", true));
    assert_eq!(f.rows, Some(Rows::Lit(2)));
    assert_eq!(f.outputs[0].name, "ROWS");
}

#[test]
fn checks_give_ibm_identifiers_and_records() {
    let p = run(&[
        (3, "select A into :nosuch from T"),
        (4, "selec A into :nm from T"),
        (5, "select A into :nm\n from NOTHERE"),
        (6, "delete from T"),
        (7, "fetch c9 into :n"),
    ]);
    let got: Vec<(u32, &str, u8)> = p.diags.iter().map(|d| (d.line, d.msgid, d.sev)).collect();
    assert_eq!(got, expected());
    assert!(!p.ok);
}

fn expected() -> Vec<(u32, &'static str, u8)> {
    let mut v = vec![(3, "SQL0312", 30), (4, "SQL0104", 30), (6, "SQL1103", 10), (6, "SQL0088", 0), (7, "SQL0504", 30)];
    v.sort_unstable();
    v
}

#[test]
fn set_option_and_whenever_act_at_compile_time() {
    let p = run(&[
        (1, "set option commit = *none, naming = *sql"),
        (2, "whenever sqlerror go to fail"),
        (3, "delete from T where ID = 1"),
        (4, "whenever sqlerror continue"),
        (5, "delete from T where ID = 2"),
    ]);
    assert_eq!((p.options.commit.as_str(), p.options.naming.as_str()), ("*NONE", "*SQL"));
    assert_eq!(p.stmts.len(), 2);
    assert_eq!(p.stmts[0].whenever.error.as_deref(), Some("FAIL"));
    assert!(p.stmts[1].whenever.error.is_none());
    assert!(p.stmts[0].desc.sql_naming);
    assert_eq!(p.diags.iter().filter(|d| d.msgid == "SQL5066").count(), 2);
}

#[test]
fn generated_lines_follow_ibm() {
    let p = run(&[(10, "declare c0 cursor for select A from T"), (11, "select A into :nm from T where ID = 1"), (12, "close c0")]);
    let g = rpgsqlpre::lines::generated(&p, &|_| 10, true);
    // The SQLCA and two structures before line 10, the code after each.
    assert_eq!(g, vec![(10, 65 + 9 + 9), (12, 8), (13, 12)]);
}
