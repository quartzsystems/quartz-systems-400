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
//!
//! The tables added for V1R1M0 (870, 875, 1025, 1026, 1142 to 1149 and
//! the bidirectional 420 and 424) were written from the primary
//! oracle's own conversion to UTF-16, byte by byte
//! (`tools/conformance/oracle/ccsid/`); a byte IBM converts to the
//! substitution character is left out. CCSID 420 holds several shapes
//! of one Arabic letter (and Arabic-Indic digits): every such byte
//! decodes to the letter, and the rows of all but the byte the oracle
//! encodes it to end in ` (decode only)`.
//!
//! The Japanese tables were written the same way, from Java's x-IBM300,
//! x-IBM930 and x-IBM939 (Temurin 25): `cp/cp0300.dbcs` the double-byte
//! code page, `cp/mixed/cpNNNN.csv` the single-byte half of a mixed CCSID
//! (0x15 as U+0085 again) and `cp/mixed/cpNNNN.dbcs` where its double-byte
//! half differs from 300. A `.dbcs` line is `BYTES CODEPOINT` (four hex
//! digits each), `BYTES CODEPOINT N` for N further pairs counting up by
//! one on both sides, or `>CODEPOINT BYTES` for a code point that only
//! encodes (a fallback); `#` starts a comment.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

/// Marks an unmapped byte in a generated table.
const NONE: u32 = u32::MAX;

/// A row whose name ends so decodes but is never what a character
/// encodes to (another byte of the same code point is).
const DECODE_ONLY: &str = " (decode only)";

/// The bytes of a table marked decode-only.
fn decode_only(path: &Path) -> Vec<u8> {
    let csv = fs::read_to_string(path).unwrap_or_else(|_| panic!("{}", path.display()));
    csv.lines()
        .skip(1)
        .filter(|l| l.ends_with(DECODE_ONLY))
        .map(|l| u8::from_str_radix(l[2..4].trim_end_matches(','), 16).expect("byte"))
        .collect()
}

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
    // Every code point must be unique among the bytes that encode, so
    // the reverse map is a function.
    let skip = decode_only(path);
    let mut seen = std::collections::HashSet::new();
    for (b, &cp) in to_unicode.iter().enumerate().filter(|&(_, &c)| c != NONE) {
        if skip.contains(&u8::try_from(b).expect("byte")) {
            continue;
        }
        assert!(
            seen.insert(cp),
            "{}: U+{cp:04X} mapped twice",
            path.display()
        );
    }
    to_unicode
}

/// Pairs of two 16-bit values.
type Pairs = Vec<(u16, u16)>;

/// A `.dbcs` file: its pairs (bytes, code point) in byte order and its
/// fallbacks (code point, bytes).
fn parse_dbcs(path: &Path) -> (Pairs, Pairs) {
    let text = fs::read_to_string(path).unwrap_or_else(|_| panic!("{}", path.display()));
    let hex = |s: &str, n: usize| {
        u16::from_str_radix(s, 16)
            .unwrap_or_else(|_| panic!("{} line {n}: bad hex {s}", path.display()))
    };
    let pair_ok = |b: u16| {
        b == 0x4040
            || ((b >> 8) >= 0x41 && (b >> 8) <= 0xFE && (b & 0xFF) >= 0x41 && (b & 0xFF) <= 0xFE)
    };
    let (mut pairs, mut enc) = (Vec::new(), Vec::new());
    for (i, line) in text.lines().enumerate() {
        let n = i + 1;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('>') {
            let mut f = rest.split_whitespace();
            let cp = hex(f.next().expect("code point"), n);
            let b = hex(f.next().expect("bytes"), n);
            assert!(pair_ok(b), "{} line {n}: bad pair", path.display());
            enc.push((cp, b));
            continue;
        }
        let mut f = line.split_whitespace();
        let b = hex(f.next().expect("bytes"), n);
        let cp = hex(f.next().expect("code point"), n);
        let run: u16 = f.next().map_or(0, |r| r.parse().expect("run"));
        for k in 0..=run {
            let (b, cp) = (b + k, cp + k);
            assert!(pair_ok(b), "{} line {n}: bad pair {b:04X}", path.display());
            assert!(
                !(0xD800..0xE000).contains(&cp),
                "{} line {n}: surrogate",
                path.display()
            );
            if let Some(&(last, _)) = pairs.last() {
                assert!(b > last, "{} line {n}: pairs out of order", path.display());
            }
            pairs.push((b, cp));
        }
    }
    let mut seen = std::collections::HashSet::new();
    for (_, cp) in &pairs {
        assert!(
            seen.insert(*cp),
            "{}: U+{cp:04X} mapped twice",
            path.display()
        );
    }
    (pairs, enc)
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

fn emit_dbcs(out: &mut String, doc: &str, name: &str, path: &Path) {
    let (pairs, enc) = parse_dbcs(path);
    let list = |out: &mut String, v: &[(u16, u16)]| {
        for row in v.chunks(6) {
            out.push_str("        ");
            for (a, b) in row {
                write!(out, "(0x{a:04X}, 0x{b:04X}), ").expect("string");
            }
            out.push('\n');
        }
    };
    writeln!(
        out,
        "/// {doc}\npub static {name}: crate::conv::dbcs::Layer = crate::conv::dbcs::Layer {{\n    pairs: &["
    )
    .expect("string");
    list(out, &pairs);
    out.push_str("    ],\n    fallbacks: &[\n");
    list(out, &enc);
    out.push_str("    ],\n};\n");
}

fn main() {
    println!("cargo:rerun-if-changed=cp037.csv");
    println!("cargo:rerun-if-changed=cp");
    println!("cargo:rerun-if-changed=cp/mixed");
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
    let mut one_way = String::from(
        "/// The bytes of a table that decode but never encode.\npub static DECODE_ONLY: &[(u16, &[u8])] = &[\n",
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
        let skip = decode_only(f);
        if !skip.is_empty() {
            writeln!(one_way, "    ({n}, &{skip:?}),").expect("string");
        }
    }
    index.push_str("];\n");
    out.push_str(&index);
    one_way.push_str("];\n");
    out.push_str(&one_way);

    emit_dbcs(
        &mut out,
        "The double-byte code page 300.",
        "DB0300",
        Path::new("cp/cp0300.dbcs"),
    );
    for n in [930u16, 939] {
        let sb = parse(Path::new(&format!("cp/mixed/cp{n:04}.csv")), false);
        for b in [0x0E, 0x0F] {
            assert!(sb[b] == NONE, "cp{n:04}.csv maps a shift code");
        }
        emit(
            &mut out,
            &format!("The single-byte half of CCSID {n}; `u32::MAX` is unmapped."),
            &format!("SB{n:04}"),
            &sb,
        );
        emit_dbcs(
            &mut out,
            &format!("Where CCSID {n}'s double-byte half differs from 300."),
            &format!("DB{n:04}"),
            Path::new(&format!("cp/mixed/cp{n:04}.dbcs")),
        );
    }
    fs::write(dest.join("tables.rs"), out).expect("write tables");
}
