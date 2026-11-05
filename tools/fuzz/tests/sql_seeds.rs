// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SQL targets' seeds are what they claim: every statement seed
//! parses, and each runs against a store in memory without a panic.

use fuzz::sql::{SEEDS, parse, run};

#[test]
fn statement_seeds_parse_and_run() {
    for s in SEEDS {
        let t = String::from_utf8_lossy(s);
        // The JSON path and document seeds are not statements.
        if t.starts_with("lax ") || t.starts_with('{') {
            continue;
        }
        if let Err(e) = sql::parse_all(&t, sql::Naming::Sql) {
            panic!("seed does not parse: {t}\n{e}");
        }
        parse(s);
        run(s);
    }
}
