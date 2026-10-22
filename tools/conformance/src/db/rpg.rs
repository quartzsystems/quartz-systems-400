// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The oracle programs generated from the kit: `RLATEST.RPGLE` runs the
//! RLA script on the machine and logs every line's status, flags and the
//! fields the script asks about into `RLALOG`; `QRESULTS.SQL` materializes
//! the fixed queries as `QRESnn` tables for `CPYTOIMPF`.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use super::kit::{self, Kit};

/// What the script does with a file, across every `OPEN` of it.
#[derive(Default)]
#[allow(clippy::struct_excessive_bools)]
struct Usage {
    input: bool,
    update: bool,
    output: bool,
    keyed: bool,
}

struct FileDecl {
    usage: Usage,
    format: String,
    rpg_format: String,
    fields: Vec<kit::DdsField>,
}

fn is_num(w: &str) -> bool {
    w.parse::<f64>().is_ok()
}

/// An RPG literal for a script value.
fn literal(v: &str) -> String {
    if v.starts_with('\'') || is_num(v) {
        v.to_string()
    } else {
        format!("'{v}'")
    }
}

/// `%char(F)` trimmed, or the field itself when character.
fn as_text(f: &kit::DdsField) -> String {
    if f.char == Some(true) {
        format!("%trimr({})", f.name)
    } else {
        format!("%char({})", f.name)
    }
}

/// Build `RLATEST.RPGLE` from the script.
#[allow(clippy::too_many_lines)]
pub fn generate_rpg(kit: &Kit) -> Result<String, String> {
    let lines = kit.rla_lines().map_err(|e| e.to_string())?;
    let members = kit.dds_members().map_err(|e| e.to_string())?;
    let dds: BTreeMap<String, (Option<String>, Vec<kit::DdsField>)> = members
        .iter()
        .map(|(n, _, t)| (n.clone(), kit::dds_fields(t)))
        .collect();
    // Logical files take field types from their physical file.
    let pf_types: BTreeMap<String, bool> = dds
        .values()
        .flat_map(|(_, fs)| {
            fs.iter()
                .filter_map(|f| f.char.map(|c| (f.name.clone(), c)))
        })
        .collect();

    // Pass 1: which files, how used.
    let mut files: BTreeMap<String, FileDecl> = BTreeMap::new();
    let mut formats_seen: BTreeMap<String, u32> = BTreeMap::new();
    for raw in &lines {
        let (op, args) = split(raw);
        if !op.eq_ignore_ascii_case("OPEN") {
            continue;
        }
        let Some(qual) = args.first() else { continue };
        let name = qual.rsplit('/').next().unwrap_or(qual).to_ascii_uppercase();
        let (format, fields) = dds
            .get(&name)
            .cloned()
            .ok_or_else(|| format!("{name}: not a kit file"))?;
        let format = format.ok_or_else(|| format!("{name}: no record format"))?;
        let entry = files.entry(name.clone()).or_insert_with(|| {
            let n = formats_seen.entry(format.clone()).or_insert(0);
            *n += 1;
            let rpg_format = if *n == 1 {
                format.clone()
            } else {
                format!("{}{}", &format[..format.len().min(8)], n)
            };
            FileDecl {
                usage: Usage::default(),
                format: format.clone(),
                rpg_format,
                fields: fields
                    .into_iter()
                    .map(|mut f| {
                        if f.char.is_none() {
                            f.char = pf_types.get(&f.name).copied();
                        }
                        f
                    })
                    .collect(),
            }
        });
        let mut any = false;
        for a in &args[1..] {
            match a.to_ascii_uppercase().as_str() {
                "INPUT" => {
                    entry.usage.input = true;
                    any = true;
                }
                "UPDATE" => {
                    entry.usage.input = true;
                    entry.usage.update = true;
                    any = true;
                }
                "OUTPUT" => {
                    entry.usage.output = true;
                    any = true;
                }
                "ALL" => {
                    entry.usage.input = true;
                    entry.usage.update = true;
                    entry.usage.output = true;
                    any = true;
                }
                "KEYED" => entry.usage.keyed = true,
                _ => {}
            }
        }
        if !any {
            entry.usage.input = true;
        }
    }

    let mut out = String::new();
    out.push_str("**free\n");
    out.push_str("// RLATEST: the QSTEST record-level script on IBM i.\n");
    out.push_str("// Generated from rla/qstest.rla by `cargo test -p conformance`; do not edit.\n");
    out.push_str("// Every script line logs LINE, %status, F/E/O flags and the fields it names\n");
    out.push_str("// into QSTEST/RLALOG; the comparer reads that back as rla.csv.\n\n");
    out.push_str(
        "ctl-opt dftactgrp(*no) actgrp(*new) option(*srcstmt:*nodebugio) alwnull(*usrctl);\n\n",
    );
    for (name, d) in &files {
        let mut usage = Vec::new();
        if d.usage.input {
            usage.push("*input");
        }
        if d.usage.update {
            usage.push("*update");
            usage.push("*delete");
        }
        if d.usage.output {
            usage.push("*output");
        }
        let _ = write!(
            out,
            "dcl-f {name} disk(*ext) usage({}) usropn",
            usage.join(":")
        );
        if d.usage.keyed {
            out.push_str(" keyed");
        }
        if d.rpg_format != d.format {
            let _ = write!(out, " rename({}:{})", d.format, d.rpg_format);
        }
        out.push_str(";\n");
    }
    out.push_str("dcl-f RLALOG disk(*ext) usage(*output) prefix(LG_);\n\n");
    out.push_str("dcl-s detail varchar(200);\n\n");

    // Pass 2: the operations.
    let mut current: Option<String> = None;
    for (i, raw) in lines.iter().enumerate() {
        let n = i + 1;
        let (op, args) = split(raw);
        if op.is_empty() {
            continue;
        }
        let expect = raw.split_once("=>").map_or("", |(_, e)| e.trim());
        let wanted: Vec<String> = sql::rla::words(expect)
            .into_iter()
            .filter_map(|w| w.split_once('=').map(|(f, _)| f.to_ascii_uppercase()))
            .collect();
        let up = op.to_ascii_uppercase();
        let _ = writeln!(out, "// {n}: {}", raw.trim());
        if up == "OPEN" {
            let qual = args.first().map_or("", String::as_str);
            let name = qual.rsplit('/').next().unwrap_or(qual).to_ascii_uppercase();
            if let Some(prev) = current.replace(name.clone()) {
                let _ = writeln!(out, "if %open({prev});\n  close {prev};\nendif;");
            }
            let _ = writeln!(out, "open(e) {name};");
            log(&mut out, n, &name, &[], &files);
            continue;
        }
        let Some(file) = current.clone() else {
            let _ = writeln!(
                out,
                "logline({n} : 1211 : *off : *off : *off : 'no file is open');"
            );
            continue;
        };
        let d = &files[&file];
        let fmt = &d.rpg_format;
        match up.as_str() {
            "CLOSE" => {
                let _ = writeln!(out, "close(e) {file};");
                current = None;
            }
            "SETLL" | "SETGT" => {
                let _ = writeln!(out, "{}(e) {} {file};", up.to_ascii_lowercase(), key(&args));
            }
            "READ" => {
                let _ = writeln!(out, "read(e) {file};");
            }
            "READN" => {
                let _ = writeln!(out, "read(en) {file};");
            }
            "READP" => {
                let _ = writeln!(out, "readp(e) {file};");
            }
            "READE" | "READPE" => {
                let k = if args.is_empty() {
                    "*key".to_string()
                } else {
                    key(&args)
                };
                let _ = writeln!(out, "{}(e) {k} {file};", up.to_ascii_lowercase());
            }
            "CHAIN" => {
                let _ = writeln!(out, "chain(e) {} {file};", key(&args));
            }
            "CHAINN" => {
                let _ = writeln!(out, "chain(en) {} {file};", key(&args));
            }
            "WRITE" | "UPDATE" => {
                if up == "WRITE" {
                    let _ = writeln!(out, "clear {fmt};");
                    for f in d.fields.iter().filter(|f| f.allow_null) {
                        let _ = writeln!(out, "%nullind({}) = *on;", f.name);
                    }
                }
                for a in &args {
                    if let Some((f, v)) = a.split_once('=') {
                        let f = f.to_ascii_uppercase();
                        let nullable = d.fields.iter().any(|x| x.name == f && x.allow_null);
                        if v.eq_ignore_ascii_case("*NULL") {
                            let _ = writeln!(out, "%nullind({f}) = *on;");
                        } else {
                            if nullable {
                                let _ = writeln!(out, "%nullind({f}) = *off;");
                            }
                            let _ = writeln!(out, "{f} = {};", literal(v));
                        }
                    }
                }
                let _ = writeln!(out, "{}(e) {fmt};", up.to_ascii_lowercase());
            }
            "DELETE" => {
                let _ = writeln!(out, "delete(e) {fmt};");
            }
            "UNLOCK" => {
                let _ = writeln!(out, "unlock(e) {file};");
            }
            "FEOD" => {
                let _ = writeln!(out, "feod(e) {file};");
            }
            other => {
                let _ = writeln!(out, "// {other}: not generated");
                continue;
            }
        }
        log(&mut out, n, &file, &wanted, &files);
    }
    if let Some(file) = current {
        let _ = writeln!(out, "if %open({file});\n  close {file};\nendif;");
    }
    out.push_str("\n*inlr = *on;\nreturn;\n\n");
    out.push_str(
        "dcl-proc logline;\n  dcl-pi *n;\n    line packed(4:0) const;\n    status packed(5:0) const;\n\
         \x20   fnd ind const;\n    eq ind const;\n    eof ind const;\n    det varchar(200) const;\n  end-pi;\n\
         \x20 LG_LINE = line;\n  LG_STATUS = %editc(status:'X');\n  LG_FLAGS = '---';\n\
         \x20 if fnd;\n    %subst(LG_FLAGS:1:1) = 'F';\n  endif;\n\
         \x20 if eq;\n    %subst(LG_FLAGS:2:1) = 'E';\n  endif;\n\
         \x20 if eof;\n    %subst(LG_FLAGS:3:1) = 'O';\n  endif;\n\
         \x20 LG_DETAIL = det;\n  write LOGREC;\nend-proc;\n",
    );
    Ok(out)
}

/// Log a line's outcome, with the fields the script asked about.
fn log(
    out: &mut String,
    n: usize,
    file: &str,
    wanted: &[String],
    files: &BTreeMap<String, FileDecl>,
) {
    out.push_str("detail = '';\n");
    if let Some(d) = files.get(file) {
        for w in wanted {
            let Some(f) = d.fields.iter().find(|f| &f.name == w) else {
                let _ = writeln!(out, "detail += '{w}=? ';");
                continue;
            };
            if f.allow_null {
                let _ = writeln!(
                    out,
                    "if %nullind({w});\n  detail += '{w}=*NULL ';\nelse;\n  detail += '{w}=' + {} + ' ';\nendif;",
                    as_text(f)
                );
            } else {
                let _ = writeln!(out, "detail += '{w}=' + {} + ' ';", as_text(f));
            }
        }
    }
    let _ = writeln!(
        out,
        "logline({n} : %status({file}) : %found({file}) : %equal({file}) : %eof({file}) : %trimr(detail));"
    );
}

/// The operation and its arguments of a script line, empty for a
/// comment or blank line.
fn split(raw: &str) -> (String, Vec<String>) {
    let line = raw.trim();
    if line.is_empty() || line.starts_with('#') || line.starts_with("--") {
        return (String::new(), Vec::new());
    }
    let op_text = line.split_once("=>").map_or(line, |(a, _)| a).trim();
    let mut ws = sql::rla::words(op_text);
    if ws.is_empty() {
        return (String::new(), Vec::new());
    }
    let op = ws.remove(0);
    (op, ws)
}

/// A key argument in RPG: `*START`/`*LOVAL`, `*END`/`*HIVAL`, `#n` as a
/// record number, else the values as a key list.
fn key(args: &[String]) -> String {
    match args.first().map(|w| w.to_ascii_uppercase()).as_deref() {
        None | Some("*START" | "*LOVAL") => "*loval".into(),
        Some("*END" | "*HIVAL") => "*hival".into(),
        Some(w) if w.starts_with('#') => w[1..].to_string(),
        _ => {
            let vals: Vec<String> = args.iter().map(|a| literal(a)).collect();
            if vals.len() == 1 {
                vals[0].clone()
            } else {
                format!("({})", vals.join(":"))
            }
        }
    }
}

/// How many items a `SELECT` list has: commas at depth 0 before `FROM`.
/// `CREATE TABLE AS` needs a column list where an item has no name.
fn select_items(query: &str) -> usize {
    let mut depth = 0usize;
    let mut items = 1;
    let bytes = query.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => items += 1,
            b' ' if depth == 0 && query[i..].to_ascii_uppercase().starts_with(" FROM ") => break,
            _ => {}
        }
        i += 1;
    }
    items
}

/// `QRESULTS.SQL`: each fixed query materialized as `QSTEST/QRESnn`.
pub fn generate_sql(kit: &Kit) -> Result<String, String> {
    let mut out = String::new();
    out.push_str("-- QRESULTS: the fixed queries as tables, for CPYTOIMPF into sql/Qnn.csv.\n");
    out.push_str(
        "-- Generated from sql/queries.sql by `cargo test -p conformance`; do not edit.\n",
    );
    out.push_str("-- RUNSQLSTM NAMING(*SYS) ERRLVL(30): the DROPs may fail the first time.\n");
    for (i, q) in kit.queries().map_err(|e| e.to_string())?.iter().enumerate() {
        let table = format!("QSTEST/QRES{:02}", i + 1);
        let upper = q.to_ascii_uppercase();
        let body = upper
            .rfind(" ORDER BY ")
            .map_or(q.as_str(), |p| q[..p].trim());
        let cols: Vec<String> = (1..=select_items(body)).map(|n| format!("C{n}")).collect();
        let cols = cols.join(", ");
        let _ = writeln!(out, "DROP TABLE {table};");
        let _ = writeln!(out, "CREATE TABLE {table} ({cols}) AS ({body}) WITH NO DATA;");
        let _ = writeln!(out, "INSERT INTO {table} {q};");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_literals() {
        assert_eq!(key(&["3".into()]), "3");
        assert_eq!(key(&["'Carol'".into()]), "'Carol'");
        assert_eq!(key(&["#3".into()]), "3");
        assert_eq!(key(&["*END".into()]), "*hival");
        assert_eq!(key(&["1".into(), "'a'".into()]), "(1:'a')");
        assert_eq!(literal("A"), "'A'");
        assert_eq!(literal("6.5"), "6.5");
    }

    #[test]
    fn program_declares_each_file_once() {
        let kit = Kit::locate();
        let rpg = generate_rpg(&kit).unwrap();
        assert_eq!(rpg.matches("dcl-f CUSTOMER ").count(), 1);
        assert!(rpg.contains("rename(CUSREC:"));
        assert!(rpg.contains("%nullind(SINCE)"));
        assert!(rpg.contains("logline("));
        let sqls = generate_sql(&kit).unwrap();
        assert!(sqls.contains("CREATE TABLE QSTEST/QRES01 (C1, C2, C3, C4, C5, C6, C7) AS ("));
        assert_eq!(select_items("SELECT A, (SELECT B, C FROM T), SUM(D) FROM U"), 3);
    }
}
