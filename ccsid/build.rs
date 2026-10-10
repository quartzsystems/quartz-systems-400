// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Turn `cp037.csv` into the two lookup tables. The CSV is the source of
//! truth: one row per byte, `byte,codepoint,name`.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=cp037.csv");
    let csv = fs::read_to_string("cp037.csv").expect("cp037.csv");
    let mut to_unicode = vec![None; 256];
    for (n, line) in csv.lines().enumerate().skip(1) {
        let mut cells = line.split(',');
        let byte = cells.next().expect("byte column");
        let cp = cells.next().expect("codepoint column");
        let byte = u8::from_str_radix(byte.trim_start_matches("0x"), 16)
            .unwrap_or_else(|_| panic!("line {}: bad byte {byte}", n + 1));
        let cp = u32::from_str_radix(cp.trim_start_matches("U+"), 16)
            .unwrap_or_else(|_| panic!("line {}: bad code point {cp}", n + 1));
        assert!(
            to_unicode[usize::from(byte)].is_none(),
            "byte {byte:#04x} listed twice"
        );
        to_unicode[usize::from(byte)] = Some(cp);
    }
    let table: Vec<u32> = to_unicode
        .iter()
        .enumerate()
        .map(|(b, cp)| cp.unwrap_or_else(|| panic!("byte {b:#04x} missing")))
        .collect();
    // Every code point must be unique so the reverse map is a function.
    let mut seen = std::collections::HashSet::new();
    for &cp in &table {
        assert!(seen.insert(cp), "code point U+{cp:04X} mapped twice");
    }
    let mut out = String::from(
        "/// CP037 byte to Unicode scalar value.\npub const TO_UNICODE: [u32; 256] = [\n",
    );
    for row in table.chunks(8) {
        out.push_str("    ");
        for cp in row {
            write!(out, "0x{cp:04X}, ").expect("string");
        }
        out.push('\n');
    }
    out.push_str("];\n");
    let dest = Path::new(&env::var("OUT_DIR").expect("OUT_DIR")).join("cp037.rs");
    fs::write(dest, out).expect("write table");
}
