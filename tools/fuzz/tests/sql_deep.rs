// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Deeply nested inputs to the SQL targets end in an error (the parser's
//! depth limit, SQL0101), not a stack overflow.

use fuzz::sql::{parse, run};

fn nest(open: &str, mid: &str, close: &str, n: usize) -> Vec<u8> {
    format!("{}{mid}{}", open.repeat(n), close.repeat(n)).into_bytes()
}

#[test]
fn deep_nesting_is_refused() {
    let n = 2000;
    let cases: Vec<Vec<u8>> = vec![
        nest("VALUES (", "1", ")", n),
        nest("(", "1", ")", n),
        nest("[", "1", "]", n),
        nest("{\"a\":", "1", "}", n),
        nest("BEGIN ", "SET X = 1;", " END", n),
        format!("VALUES {}1", "NOT ".repeat(n)).into_bytes(),
        format!("VALUES {}1", "- ".repeat(n)).into_bytes(),
        nest("VALUES CASE WHEN 1 = 1 THEN ", "1", " END", n),
        nest("SELECT * FROM (", "VALUES 1", ") X", n),
        format!("$.a{}", "[0]".repeat(n)).into_bytes(),
    ];
    // A release build (the fuzz profile) within the 1 MiB main thread of
    // Windows; a debug build, its frames far larger, within Linux's 8 MiB.
    let stack = if cfg!(debug_assertions) { 8 << 20 } else { 1 << 20 };
    for data in cases {
        std::thread::Builder::new()
            .stack_size(stack)
            .spawn(move || {
                parse(&data);
                run(&data);
            })
            .expect("thread")
            .join()
            .expect("no panic");
    }
}
