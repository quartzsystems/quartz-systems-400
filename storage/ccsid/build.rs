// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Turn the code page CSVs into lookup tables. Each CSV is the source of
//! truth for its code page: one row per byte, `byte,codepoint,name`.
//! `cp037.csv` must map every byte; the tables under `cp/` may leave a
//! byte out (unmapped). `cp/` was written once from Java's charsets
//! (Temurin 25), with EBCDIC 0x15 as U+0085 the way IBM's tables and
//! `cp037.csv` have it, and the five undefined CP1252 bytes as the C1
//! controls of the same value, as IBM's CCSID 1252 maps them.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

/// Marks an unmapped byte in a generated table.
const NONE: u32 = u32::MAX;

fn parse(path: &Path, full: bool) -> Vec<u32> {
    let csv = fs::read_to_string(path).unwrap_or_else(|_| panic!("{}", path.display()));
    let mut to_unicode = vec![NONE; 256];
    for (n, line) in csv.lines().enumerate().skip(1) {
        let mut cells = line.split(',');
        let byte = cells.next().expect("byte column");
        let cp = cells.next().expect("codepoint column");
        let byte = u8::from_str_radix(byte.trim_start_matches("0x"), 16)
            .unwrap_or_else(|_| panic!("{} line {}: bad byte {byte}", path.display(), n + 1));
        let cp = u32::from_str_radix(cp.trim_start_matches("U+"), 16)
            .unwrap_or_else(|_| panic!("{} line {}: bad code point {cp}", path.display(), n + 1));
        assert!(
            to_unicode[usize::from(byte)] == NONE,
            "{}: byte {byte:#04x} listed twice",
            path.display()
        );
        to_unicode[usize::from(byte)] = cp;
    }
    if full {
        for (b, cp) in to_unicode.iter().enumerate() {
            assert!(*cp != NONE, "{}: byte {b:#04x} missing", path.display());
        }
    }
    // Every code point must be unique so the reverse map is a function.
    let mut seen = std::collections::HashSet::new();
    for &cp in to_unicode.iter().filter(|&&c| c != NONE) {
        assert!(
            seen.insert(cp),
            "{}: U+{cp:04X} mapped twice",
            path.display()
        );
    }
    to_unicode
}

fn emit(out: &mut String, doc: &str, name: &str, table: &[u32]) {
    writeln!(out, "/// {doc}\npub static {name}: [u32; 256] = [").expect("string");
    for row in table.chunks(8) {
        out.push_str("    ");
        for cp in row {
            if *cp == NONE {
                out.push_str("u32::MAX, ");
            } else {
                write!(out, "0x{cp:04X}, ").expect("string");
            }
        }
        out.push('\n');
    }
    out.push_str("];\n");
}

fn main() {
    println!("cargo:rerun-if-changed=cp037.csv");
    println!("cargo:rerun-if-changed=cp");
    let dest = Path::new(&env::var("OUT_DIR").expect("OUT_DIR")).to_path_buf();

    let cp037 = parse(Path::new("cp037.csv"), true);
    let mut out = String::new();
    emit(
        &mut out,
        "CP037 byte to Unicode scalar value.",
        "TO_UNICODE",
        &cp037,
    );
    fs::write(
        dest.join("cp037.rs"),
        out.replace("pub static", "pub const"),
    )
    .expect("write table");

    let mut files: Vec<_> = fs::read_dir("cp")
        .expect("cp directory")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "csv"))
        .collect();
    files.sort();
    let mut out = String::new();
    let mut index = String::from(
        "/// Every generated table by CCSID.\npub static TABLES: &[(u16, &[u32; 256])] = &[\n",
    );
    for f in &files {
        let stem = f.file_stem().expect("stem").to_string_lossy();
        let n: u16 = stem.trim_start_matches("cp").parse().expect("cpNNNN.csv");
        let table = parse(f, false);
        let name = format!("CP{n:04}");
        emit(
            &mut out,
            &format!("CCSID {n} byte to Unicode; `u32::MAX` is unmapped."),
            &name,
            &table,
        );
        writeln!(index, "    ({n}, &{name}),").expect("string");
    }
    index.push_str("];\n");
    out.push_str(&index);
    fs::write(dest.join("tables.rs"), out).expect("write tables");
}
