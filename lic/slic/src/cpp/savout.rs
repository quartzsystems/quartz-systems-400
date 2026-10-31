// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `OUTPUT(*OUTFILE)` of the save and restore commands: IBM's model
//! formats `QSRSAV` (`QASAVOBJ`) and `QSRRST` (`QASRRSTO`), field for
//! field as the V7R1M0 oracle's `DSPFFD` shows them
//! (`tools/conformance/oracle/save/primary/03-savlib-outfile.txt`,
//! `17-errors.txt`), created through DDS when the file is missing.

use cmd::block::Parms;
use db::Member;
use db::file::{self, CreateFileOpts};
use dds::{CompileOpts, NoRefs, Source, compile_pf};
use qsname::MemberSpec;
use saverst::{RestoreReport, SaveReport};
use store::obj::{JobHandle, types};

use super::direct::{in_store, lib_or};
use crate::ipl::system::{System, now_unix};

/// A field: name, DDS type (`A`, `S` zoned, `P` packed, `V` varying
/// character), length, decimals.
type Fld = (&'static str, char, u16, u8);

/// `QSRSAV`, 76 fields, 3732 bytes.
const QSRSAV: &[Fld] = &[
    ("SROCMD", 'A', 10, 0), ("SROINF", 'A', 10, 0), ("SROSYS", 'A', 8, 0),
    ("SROSRL", 'A', 6, 0), ("SROLIB", 'A', 10, 0), ("SROASP", 'S', 2, 0),
    ("SROSAV", 'S', 6, 0), ("SROERR", 'S', 6, 0), ("SROSEQ", 'S', 4, 0),
    ("SROLBL", 'A', 17, 0), ("SROVOL", 'A', 60, 0), ("SROSVT", 'A', 13, 0),
    ("SRONAM", 'A', 10, 0), ("SROMNM", 'A', 10, 0), ("SROTYP", 'A', 8, 0),
    ("SROATT", 'A', 10, 0), ("SROSIZ", 'S', 15, 0), ("SROOWN", 'A', 10, 0),
    ("SROSTA", 'A', 1, 0), ("SROMSG", 'A', 7, 0), ("SROSWA", 'A', 13, 0),
    ("SROTXT", 'A', 50, 0), ("SRODEV", 'A', 40, 0), ("SROSVF", 'A', 10, 0),
    ("SROSFL", 'A', 10, 0), ("SROTRL", 'A', 6, 0), ("SROSTF", 'A', 1, 0),
    ("SROACP", 'A', 1, 0), ("SROSFD", 'A', 1, 0), ("SROCMP", 'A', 1, 0),
    ("SROCOM", 'A', 1, 0), ("SRORFD", 'A', 7, 0), ("SRORFT", 'A', 6, 0),
    ("SROEXP", 'A', 7, 0), ("SROXVM", 'V', 390, 0), ("SROPGP", 'A', 10, 0),
    ("SROSQ2", 'S', 10, 0), ("SROMIT", 'A', 1, 0), ("SROFMT", 'A', 1, 0),
    ("SROMFN", 'S', 3, 0), ("SROTMF", 'S', 3, 0), ("SROMDN", 'A', 10, 0),
    ("SROMDL", 'A', 10, 0), ("SROVLC", 'S', 3, 0), ("SROVLL", 'S', 3, 0),
    ("SROVLD", 'V', 2400, 0), ("SROOPT", 'V', 256, 0), ("SROAS1", 'A', 10, 0),
    ("SROAS2", 'S', 5, 0), ("SROTSZ", 'P', 21, 0), ("SROPRT", 'A', 1, 0),
    ("SROJN", 'A', 10, 0), ("SROJL", 'A', 10, 0), ("SROJRN", 'A', 10, 0),
    ("SROJRL", 'A', 10, 0), ("SROJRA", 'A', 10, 0), ("SROPFL", 'A', 10, 0),
    ("SROPFN", 'S', 6, 0), ("SROPJB", 'A', 10, 0), ("SROPUN", 'A', 10, 0),
    ("SROPJN", 'A', 6, 0), ("SROPJS", 'A', 8, 0), ("SROPCD", 'A', 7, 0),
    ("SROPCT", 'A', 6, 0), ("SROPQN", 'A', 10, 0), ("SROPQL", 'A', 10, 0),
    ("SROPUD", 'A', 10, 0), ("SROPFT", 'A', 10, 0), ("SROPPG", 'P', 11, 0),
    ("SROPCP", 'S', 3, 0), ("SROPSZ", 'P', 15, 0), ("SROPXD", 'A', 7, 0),
    ("SROPVA", 'A', 1, 0), ("SROSYN", 'A', 10, 0), ("SROSYO", 'S', 2, 0),
    ("SROPSN", 'A', 32, 0),
];

/// `QSRRST`, 71 fields.
const QSRRST: &[Fld] = &[
    ("RSRECR", 'A', 1, 0), ("RSOSTA", 'A', 1, 0), ("RSOMSG", 'A', 7, 0),
    ("RSOSEC", 'A', 1, 0), ("RSNAME", 'A', 10, 0), ("RSMNAM", 'A', 10, 0),
    ("RSTYPE", 'A', 8, 0), ("RSATTR", 'A', 10, 0), ("RSSIZE", 'S', 15, 0),
    ("RSOOWN", 'A', 10, 0), ("RSOTXT", 'A', 50, 0), ("RSRPLV", 'V', 704, 0),
    ("RSOLIB", 'A', 10, 0), ("RSOASP", 'S', 2, 0), ("RSOSEQ", 'S', 4, 0),
    ("RSLABL", 'A', 17, 0), ("RSSVDT", 'A', 13, 0), ("RSRSDT", 'A', 13, 0),
    ("RSVLID", 'V', 450, 0), ("RSODEV", 'A', 40, 0), ("RSOSVF", 'A', 10, 0),
    ("RSOSFL", 'A', 10, 0), ("RSOCMD", 'A', 10, 0), ("RSSYSN", 'A', 8, 0),
    ("RSORRL", 'A', 6, 0), ("RSOSRL", 'A', 6, 0), ("RSOPGP", 'A', 10, 0),
    ("RSOSQ2", 'S', 10, 0), ("RSONLB", 'A', 10, 0), ("RSOFMT", 'A', 1, 0),
    ("RSOMFN", 'S', 3, 0), ("RSOTMF", 'S', 3, 0), ("RSOMDN", 'A', 10, 0),
    ("RSOMDL", 'A', 10, 0), ("RSOVLC", 'S', 3, 0), ("RSOVLL", 'S', 3, 0),
    ("RSOVLD", 'V', 2400, 0), ("RSOOPT", 'V', 256, 0), ("RSOAS1", 'A', 10, 0),
    ("RSOAS2", 'S', 5, 0), ("RSOPRT", 'A', 1, 0), ("RSOJN", 'A', 10, 0),
    ("RSOJL", 'A', 10, 0), ("RSOJRN", 'A', 10, 0), ("RSOJRL", 'A', 10, 0),
    ("RSOJRA", 'A', 10, 0), ("RSOPFL", 'A', 10, 0), ("RSOPFN", 'S', 6, 0),
    ("RSOPJB", 'A', 10, 0), ("RSOPUN", 'A', 10, 0), ("RSOPJN", 'A', 6, 0),
    ("RSOPJS", 'A', 8, 0), ("RSOPCD", 'A', 7, 0), ("RSOPCT", 'A', 6, 0),
    ("RSOPQN", 'A', 10, 0), ("RSOPQL", 'A', 10, 0), ("RSOPUD", 'A', 10, 0),
    ("RSOPFT", 'A', 10, 0), ("RSOPPG", 'P', 11, 0), ("RSOPCP", 'S', 3, 0),
    ("RSOPSZ", 'P', 15, 0), ("RSOPXD", 'A', 7, 0), ("RSOPDR", 'A', 7, 0),
    ("RSOPTR", 'A', 6, 0), ("RSOPQR", 'A', 10, 0), ("RSOPLR", 'A', 10, 0),
    ("RSOPVA", 'A', 1, 0), ("RSOPVS", 'S', 10, 0), ("RSOPVR", 'S', 10, 0),
    ("RSODFR", 'A', 1, 0), ("RSODFI", 'A', 10, 0),
];

/// One row: field name and value (text, or a number for `S`/`P`).
type Row = Vec<(&'static str, String)>;

/// DDS source of a model format.
fn dds(format: &str, fields: &[Fld]) -> String {
    let at = |parts: &[(usize, &str)]| {
        let mut s = vec![' '; 80];
        for (col, text) in parts {
            for (i, ch) in text.chars().enumerate() {
                s[col - 1 + i] = ch;
            }
        }
        s.into_iter().collect::<String>().trim_end().to_string()
    };
    let mut lines = vec![at(&[(6, "A"), (17, "R"), (19, format)])];
    for (name, ty, len, dec) in fields {
        let l = len.to_string();
        let (t, kw) = match ty {
            'V' => ("A", "VARLEN"),
            'S' => ("S", ""),
            'P' => ("P", ""),
            _ => ("A", ""),
        };
        let d = if matches!(ty, 'S' | 'P') {
            dec.to_string()
        } else {
            String::new()
        };
        lines.push(at(&[
            (6, "A"),
            (19, name),
            (35 - l.len(), &l),
            (35, t),
            (38 - d.len(), &d),
            (45, kw),
        ]));
    }
    lines.join("\n")
}

/// `CYYMMDDHHMMSS` of a Unix time (UTC).
#[allow(clippy::many_single_char_names)]
pub fn stamp(t: u64) -> String {
    let days = i64::try_from(t / 86_400).unwrap_or(0);
    let secs = t % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    let c = i64::from(y >= 2000);
    format!(
        "{c}{:02}{m:02}{d:02}{:02}{:02}{:02}",
        y % 100,
        secs / 3600,
        secs / 60 % 60,
        secs % 60
    )
}

fn type_name(t: u16) -> String {
    store::obj::type_by_code(t).map_or_else(String::new, |i| i.name.to_string())
}

/// Write `rows` to the outfile `OUTFILE`/`OUTMBR` names, creating it in
/// the model format when it is missing.
fn write(
    system: &mut System,
    parms: &Parms,
    format: &str,
    fields: &[Fld],
    rows: &[Row],
) -> Result<String, String> {
    let (name, lib) = parms.qual("OUTFILE");
    if name.is_empty() {
        return Err("CPD0071 OUTFILE is required with OUTPUT(*OUTFILE)".into());
    }
    let lib = lib_or(&lib);
    let replace = !parms
        .elems("OUTMBR")
        .get(1)
        .copied()
        .flatten()
        .is_some_and(|v| v.source().eq_ignore_ascii_case("*ADD"));
    let src = dds(format, fields);
    let job = JobHandle(u64::from(system.current_job));
    in_store(system, |txn, qsys, names| {
        let l = super::direct::library(txn, qsys, names, if lib == "*LIBL" { "*CURLIB" } else { &lib })?;
        let n = qsname::Name::parse(&name).map_err(|e| format!("CPD0074 {e}"))?;
        let existing = store::obj::library::Library::load(txn, l)
            .and_then(|x| x.lookup(txn, types::FILE, n))
            .map_err(|e| e.to_string())?;
        let mut note = String::new();
        let fid = if let Some(id) = existing {
            id
        } else {
            let (source, _) = Source::from_text(&src);
            let c = compile_pf(&source, &NoRefs, &CompileOpts::default());
            let def = c.definition.ok_or_else(|| format!("CPF7302 {}", c.listing))?;
            note = format!("CPF9861 Output file {name} created in library {lib}.\n");
            file::create_pf(
                txn,
                l,
                n,
                &def,
                &CreateFileOpts {
                    now: now_unix(),
                    max_members: 0,
                    ..CreateFileOpts::default()
                },
            )
            .map_err(|e| e.to_string())?
        };
        let mut m = Member::open_by(txn, fid, MemberSpec::First).map_err(|e| e.to_string())?;
        if replace {
            m.clear(txn).map_err(|e| e.to_string())?;
        }
        let fmt = m.format().format.clone();
        for row in rows {
            let mut rec = vec![0u8; usize::try_from(fmt.record_len).unwrap_or(0)];
            for (fname, f) in fmt.fields.iter().map(|f| (f.name.to_string(), f)) {
                let v = row
                    .iter()
                    .find(|(k, _)| *k == fname)
                    .map_or("", |(_, v)| v.as_str());
                let val = if f.data_type.is_numeric() {
                    db::Value::Num(qdecimal::Decimal::parse(if v.is_empty() { "0" } else { v }).map_err(|e| e.to_string())?)
                } else {
                    db::Value::Char(ccsid::to_ebcdic(v).unwrap_or_default())
                };
                db::record::write_field(f, &mut rec, &val).map_err(|e| e.to_string())?;
            }
            m.write_new(txn, &rec, &vec![0u8; m.core.null_map_len()])
                .map_err(|e| e.to_string())?;
        }
        let _ = job;
        Ok(format!("{note}{} records written to {lib}/{name}.\n", rows.len()))
    })
}

/// The `QSRSAV` rows of a save, as `INFTYPE` asks.
pub fn save(
    system: &mut System,
    parms: &Parms,
    command: &str,
    reports: &[SaveReport],
    savf: &(String, String),
    now: u64,
) -> Result<String, String> {
    let inftype = parms.text("INFTYPE");
    let rel = {
        let r = release::RELEASE;
        format!("V{}R{}M{}", r.v, r.r, r.m)
    };
    let sys = system.system_name();
    let mut rows = Vec::new();
    for r in reports {
        let base = |info: &str| -> Row {
            vec![
                ("SROCMD", command.to_string()),
                ("SROINF", info.to_string()),
                ("SROSYS", sys.clone()),
                ("SROSRL", rel.clone()),
                ("SROLIB", r.library.clone()),
                ("SROASP", "1".into()),
                ("SROSAV", r.saved().to_string()),
                ("SROERR", r.not_saved().to_string()),
                ("SROSEQ", "0".into()),
                ("SROSVT", stamp(now)),
                ("SROSVF", savf.0.clone()),
                ("SROSFL", savf.1.clone()),
                ("SROTRL", rel.clone()),
                ("SROSTF", "0".into()),
                ("SROACP", "1".into()),
                ("SROSFD", "1".into()),
                ("SROCMP", "0".into()),
                ("SROCOM", "0".into()),
                ("SROMIT", "1".into()),
                ("SROFMT", "0".into()),
                ("SROMFN", "1".into()),
                ("SROTMF", "1".into()),
                ("SROAS1", "*SYSBAS".into()),
                ("SROAS2", "1".into()),
                ("SROPVA", "0".into()),
            ]
        };
        if inftype == "*LIB" {
            let mut row = base("*LIB");
            row.push(("SROTSZ", r.objects.iter().map(|o| o.size).sum::<u64>().to_string()));
            rows.push(row);
            continue;
        }
        for o in &r.objects {
            if inftype == "*ERR" && o.msgid.is_none() {
                continue;
            }
            let mut row = base("*OBJ");
            row.extend([
                ("SRONAM", o.name.clone()),
                ("SROTYP", type_name(o.type_code)),
                ("SROATT", o.attribute.clone()),
                ("SROSIZ", o.size.to_string()),
                ("SROOWN", o.owner.clone()),
                ("SROSTA", if o.msgid.is_none() { "1" } else { "0" }.into()),
                ("SROMSG", o.msgid.unwrap_or("").into()),
                ("SROTXT", o.text.clone()),
                ("SROPGP", if o.primary_group.is_empty() { "*NONE".into() } else { o.primary_group.clone() }),
                ("SROSQ2", "0".into()),
            ]);
            rows.push(row);
        }
    }
    write(system, parms, "QSRSAV", QSRSAV, &rows)
}

/// The `QSRRST` rows of a restore: one per object.
pub fn restore(
    system: &mut System,
    parms: &Parms,
    command: &str,
    report: &RestoreReport,
    savf: &(String, String),
    now: u64,
) -> Result<String, String> {
    let rel = {
        let r = release::RELEASE;
        format!("V{}R{}M{}", r.v, r.r, r.m)
    };
    let sys = system.system_name();
    let errors_only = parms.text("INFTYPE") == "*ERR";
    let mut rows = Vec::new();
    for o in &report.objects {
        if errors_only && o.msgid.is_none() {
            continue;
        }
        rows.push(vec![
            ("RSRECR", "1".to_string()),
            ("RSOSTA", if o.msgid.is_none() { "1" } else { "0" }.into()),
            ("RSOMSG", o.msgid.unwrap_or("").into()),
            ("RSOSEC", "0".into()),
            ("RSNAME", o.name.clone()),
            ("RSTYPE", type_name(o.type_code)),
            ("RSATTR", o.attribute.clone()),
            ("RSSIZE", o.size.to_string()),
            ("RSOOWN", o.owner.clone()),
            ("RSOTXT", o.text.clone()),
            ("RSOLIB", o.library.clone()),
            ("RSOASP", "1".into()),
            ("RSOSEQ", "0".into()),
            ("RSRSDT", stamp(now)),
            ("RSOSVF", savf.0.clone()),
            ("RSOSFL", savf.1.clone()),
            ("RSOCMD", command.into()),
            ("RSSYSN", sys.clone()),
            ("RSORRL", rel.clone()),
            ("RSOSRL", rel.clone()),
            ("RSOPGP", if o.primary_group.is_empty() { "*NONE".into() } else { o.primary_group.clone() }),
            ("RSOSQ2", "0".into()),
            ("RSONLB", o.library.clone()),
            ("RSOFMT", "0".into()),
            ("RSOMFN", "1".into()),
            ("RSOTMF", "1".into()),
            ("RSOAS1", "*SYSBAS".into()),
            ("RSOAS2", "1".into()),
            ("RSOPRT", "0".into()),
            ("RSOPVA", "0".into()),
            ("RSODFR", "0".into()),
        ]);
    }
    write(system, parms, "QSRRST", QSRRST, &rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_and_lengths() {
        assert_eq!(stamp(1_793_349_915), "1261030072515");
        let len = |f: &[Fld]| -> usize {
            f.iter()
                .map(|(_, t, l, _)| match t {
                    'V' => usize::from(*l) + 2,
                    'P' => usize::from(*l) / 2 + 1,
                    _ => usize::from(*l),
                })
                .sum()
        };
        // The record lengths DSPFFD shows.
        assert_eq!(len(QSRSAV), 3732);
        assert_eq!(QSRSAV.len(), 76);
        assert_eq!(QSRRST.len(), 71);
    }
}
