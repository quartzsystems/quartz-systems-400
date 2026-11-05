// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The statements CRTSQLRPGI keeps print as pub400's `PRTSQLINF`
//! prints them (`tools/conformance/oracle/sql-admin/prtsqlinf-pgm.txt`).

use rpgc::{Command, Options};
use rpgfront::source::{Member, Record};

const GOLDEN: &str = include_str!("../../../tools/conformance/oracle/sql-admin/prtsqlinf-pgm.txt");
const SOURCE: &str = include_str!("../../../tools/conformance/oracle/sql-admin/prtsqlinf-pgm.sqlrpgle");

fn compiled() -> rpgsqlpre::Precompiled {
    let member = Member {
        lib: "QGPL".into(),
        file: "QRPGLESRC".into(),
        name: "QS15SQL".into(),
        ccsid: 37,
        records: SOURCE
            .lines()
            .enumerate()
            .map(|(i, l)| Record { seq: format!("{:06}", (i + 1) * 100), date: "000000".into(), text: l.to_string() })
            .collect(),
    };
    let opts = Options {
        command: Command::CrtSqlRpgi,
        name: "QS15SQL".into(),
        lib: "QGPL".into(),
        sql: Some(rpgsqlpre::Options { commit: "*NONE".into(), ..rpgsqlpre::Options::default() }),
        ..Options::default()
    };
    let c = rpgc::compile(member, &mut rpgfront::NoIncludes, &mut rpgc::NoExtFiles, &opts);
    c.precompiled.expect("precompiled")
}

#[test]
fn statements_print_as_pub400() {
    let pre = compiled();
    let stmts = sql::prtsqlinf::entries(&pre.texts);
    let h = sql::prtsqlinf::Heading {
        release: "V7R5M0".into(),
        object: "{user}1/QS15SQL".into(),
        objtype: "*PGM".into(),
        date: "26-11-05".into(),
        time: "19:52:10".into(),
        command: "CRTSQLRPGI".into(),
        ccsid: 273,
    };
    let ours = sql::prtsqlinf::render(&h, &[], &stmts);
    // The heading, but for IBM's build stamp. The capture's account name
    // is masked, so the object's padding is compared by column: the
    // date at 102, the page at 121.
    let gold = GOLDEN.lines().next().unwrap().replace("220415", "      ");
    let at = gold.find("{user}").unwrap();
    assert_eq!(ours[0][..at], gold[..at]);
    assert_eq!(ours[0][102..], gold[gold.find("26-11-05").unwrap()..]);
    assert_eq!(ours[0].find("Page"), Some(121));
    // The statements and their access plan lines, in pub400's order.
    let pick = |l: &&str| {
        let first = l.get(1..).unwrap_or("").split_whitespace().next().unwrap_or("");
        matches!(first, "DECLARE" | "SELECT" | "OPEN" | "FETCH" | "CLOSE" | "PREPARE" | "VALUES" | "TABLE_SCHEMA" | "SQL5065")
    };
    let norm = |l: &str| {
        let b = l.get(1..).unwrap_or("").trim_end().to_string();
        if b.trim_start().starts_with("SQL5065") { "SQL5065".to_string() } else { b }
    };
    let want: Vec<String> = GOLDEN.lines().filter(pick).map(norm).collect();
    let got: Vec<String> = ours.iter().map(String::as_str).filter(pick).map(norm).collect();
    assert_eq!(got, want);
    // The object and the option block open the report as there.
    assert_eq!(ours[1], "-Object name...............{user}1/QS15SQL");
    assert_eq!(ours[2], " Object type...............*PGM");
    assert_eq!(ours[3], "-");
    assert_eq!(ours[4], "   CRTSQLRPGI");
    assert!(ours.last().unwrap().contains("E N D  O F  L I S T I N G"));
}

#[test]
fn options_are_the_command_and_set_option() {
    let pre = compiled();
    let p = sql::prtsqlinf::Precompile {
        object: "QGPL/QS15SQL".into(),
        commit: pre.options.commit.clone(),
        naming: pre.options.naming.clone(),
        dates: [pre.options.datfmt.clone(), pre.options.datsep.clone(), pre.options.timfmt.clone(), pre.options.timsep.clone()],
        closqlcsr: pre.options.closqlcsr.clone(),
        other: pre.options.other.clone(),
        release: "V1R1M0".into(),
        ..sql::prtsqlinf::Precompile::default()
    };
    let o = sql::prtsqlinf::options(&p);
    for (k, v) in [("COMMIT", "*NONE"), ("DATFMT", "*ISO"), ("OPTION", "*SYS *NOEXTIND *PERIOD"), ("CLOSQLCSR", "*ENDACTGRP")] {
        assert!(o.contains(&(k.into(), v.into())), "{k}({v}) not in {o:?}");
    }
    // Rendered one to a line under the command, as pub400 lays them out.
    let h = sql::prtsqlinf::Heading {
        release: "V1R1M0".into(),
        object: "QGPL/QS15SQL".into(),
        objtype: "*PGM".into(),
        date: "26-11-05".into(),
        time: "19:52:10".into(),
        command: "CRTSQLRPGI".into(),
        ccsid: 37,
    };
    let lines = sql::prtsqlinf::render(&h, &o, &[]);
    assert_eq!(lines[5], "       OBJ(QGPL/QS15SQL)");
    assert!(lines.iter().any(|l| l == "       COMMIT(*NONE)"));
    assert!(lines.iter().any(|l| l == "       STATEMENT TEXT CCSID(37)"));
}
