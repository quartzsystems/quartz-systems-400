// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The servers' own code points and return codes against
//! `docs/HOSTSVR.md` ("Server code points and return codes"): a table
//! after `<!-- server-consts: KEY -->` lists exactly the integer
//! constants of `hostserver::KEY`, and the source declares no constant
//! the list below misses.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use hostserver::{central, database, dtaq, file, netprt};

/// `(name, value)` of each named constant of a module.
macro_rules! consts {
    ($m:path; $($n:ident),* $(,)?) => {{
        use $m as m;
        vec![$((stringify!($n).to_string(), u64::from(m::$n))),*]
    }};
}

/// A key, where it is declared (file under `src/`, inline module) and
/// its constants.
type Entry = (&'static str, &'static str, Option<&'static str>, Vec<(String, u64)>);

/// Every key.
fn code() -> Vec<Entry> {
    vec![
        ("lib", "lib.rs", None, vec![
            ("SERVER_VRM".into(), u64::from(hostserver::SERVER_VRM)),
            ("SERVER_LEVEL".into(), u64::from(hostserver::SERVER_LEVEL)),
        ]),
        ("central::rc", "central.rs", Some("rc"), consts!(central::rc; OK, CCSID, EXIT)),
        ("database::attr", "database/mod.rs", Some("attr"), consts!(database::attr;
            DATE_FORMAT, NAMING, COMMIT, DEFAULT_LIBRARY, AUTOCOMMIT, INTERFACE_TYPE,
            INTERFACE_NAME, INTERFACE_LEVEL)),
        ("database::types::sqltype", "database/types.rs", Some("sqltype"), consts!(database::types::sqltype;
            DATE, TIME, TIMESTAMP, VARCHAR, CHAR, LONGVARCHAR, VARGRAPHIC, GRAPHIC, LONGVARGRAPHIC,
            FLOAT, DECIMAL, NUMERIC, BIGINT, INTEGER, SMALLINT, VARBINARY, BINARY)),
        ("dtaq::rc", "dtaq.rs", Some("rc"), consts!(dtaq::rc; OK, ERROR, PROTOCOL, NO_ENTRY, EXIT_REJECTED)),
        ("file::cp", "file/mod.rs", Some("cp"), consts!(file::cp;
            NAME1, NAME, SOURCE, TARGET, READ_DATA, CCSIDS, OA2, OA1, WRITE_DATA)),
        ("file::rc", "file/rc.rs", None, consts!(file::rc;
            SUCCESS, FILE_IN_USE, FILE_NOT_FOUND, PATH_NOT_FOUND, DUPLICATE, ACCESS_DENIED,
            INVALID_HANDLE, INVALID_NAME, DIR_NOT_EMPTY, ACCESS_DENIED_TO_REQUEST, NO_MORE_FILES,
            REQUEST_NOT_SUPPORTED, UNKNOWN, SHARING_VIOLATION, LOCK_VIOLATION)),
        ("netprt::cp", "netprt/mod.rs", Some("cp"), consts!(netprt::cp;
            ID, DATA, SELECTION, ATTR_IDS, ATTRS, TARGET_OUTQ)),
        ("netprt::rc", "netprt/mod.rs", Some("rc"), consts!(netprt::rc;
            OK, NOT_AUTHORIZED, CPF_MESSAGE, READ_EOF, EMPTY_LIST, NOT_SUPPORTED, SPLF_NOT_FOUND,
            EXIT_DENIED)),
        ("netprt::attrs::kind", "netprt/attrs.rs", Some("kind"), consts!(netprt::attrs::kind;
            BIN4, STRING, PACKED)),
        ("netprt::attrs::ids", "netprt/attrs.rs", Some("ids"), consts!(netprt::attrs::ids;
            DATE, COPIES, JOBNAME, JOBNUMBER, JOBUSER, LPI, OUTPTY, OUTQUELIB, OUTQUE, PAGELEN,
            PAGEWIDTH, PRTDEVTYPE, PRTFLIB, PRTFILE, SPOOLFILE, SPLFNUM, SPLFSTATUS, TIME, PAGES,
            USERDATA, FORMTYPE, NPSCCSID, NPSLEVEL, NLV_ID, JOBSYSTEM, WRITER, PRINTER)),
    ]
}

fn read(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("{}: {e}", p.display()))
        .replace("\r\n", "\n")
}

/// The integer constants a file declares, at its top level or in
/// `pub mod NAME { ... }`.
fn declared(file: &str, module: Option<&str>) -> BTreeSet<String> {
    let src = read(&format!("src/{file}"));
    let lines: Vec<&str> = match module {
        None => src.lines().filter(|l| l.starts_with("pub const ")).collect(),
        Some(m) => {
            let open = format!("pub mod {m} {{");
            let at = src
                .lines()
                .position(|l| l == open)
                .unwrap_or_else(|| panic!("{file} has no `{open}`"));
            src.lines().skip(at + 1).take_while(|l| *l != "}").collect()
        }
    };
    lines
        .iter()
        .filter_map(|l| l.trim().strip_prefix("pub const "))
        .filter_map(|r| r.split_once(':'))
        .filter(|(_, t)| ["u8 ", "u16 ", "u32 ", "i32 "].iter().any(|ty| t.trim_start().starts_with(ty)))
        .map(|(n, _)| n.trim().to_string())
        .collect()
}

/// Each marked table's `(name, value)` rows by key.
fn tables() -> BTreeMap<String, Vec<(String, u64)>> {
    let md = read("../../docs/HOSTSVR.md");
    let lines: Vec<&str> = md.lines().collect();
    let mut out = BTreeMap::new();
    for (i, l) in lines.iter().enumerate() {
        let Some(key) = l.trim().strip_prefix("<!-- server-consts: ").and_then(|r| r.strip_suffix(" -->")) else {
            continue;
        };
        let rows: Vec<(String, u64)> = lines[i + 1..]
            .iter()
            .skip_while(|l| !l.starts_with('|'))
            .take_while(|l| l.starts_with('|'))
            .skip(2)
            .map(|row| {
                let c: Vec<String> =
                    row.trim().trim_matches('|').split('|').map(|c| c.trim().replace('`', "")).collect();
                (c[1].clone(), number(&c[0]))
            })
            .collect();
        assert!(out.insert(key.to_string(), rows).is_none(), "{key} twice");
    }
    out
}

fn number(s: &str) -> u64 {
    match s.strip_prefix("0x") {
        Some(h) => u64::from_str_radix(&h.replace('_', ""), 16),
        None => s.parse(),
    }
    .unwrap_or_else(|_| panic!("not a number: {s}"))
}

#[test]
fn server_constants_match_the_doc() {
    let doc = tables();
    let code = code();
    for (key, _, _, list) in &code {
        let rows = doc.get(*key).unwrap_or_else(|| panic!("docs/HOSTSVR.md has no table for {key}"));
        let mut d = rows.clone();
        let mut c = list.clone();
        d.sort();
        c.sort();
        assert_eq!(d, c, "{key}: the doc and the code differ");
    }
    let keys: BTreeSet<&str> = code.iter().map(|c| c.0).collect();
    for key in doc.keys() {
        assert!(keys.contains(key.as_str()), "docs/HOSTSVR.md lists unknown {key}");
    }
}

#[test]
fn every_declared_constant_is_listed() {
    for (key, file, module, list) in code() {
        let listed: BTreeSet<String> = list.into_iter().map(|(n, _)| n).collect();
        assert_eq!(declared(file, module), listed, "{key}: a constant the test does not list");
    }
}
