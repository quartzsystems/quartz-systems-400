// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `OUTPUT(*OUTFILE)` of `DSPPTF`, `DSPSFWRSC` and `DSPOBJAUT`: IBM's
//! model formats `QSCPTF` (`QADSPPTF`), `QARZLCGD` (`QARZLCOF`) and
//! `QSYDSAUT` (`QAOBJAUT`), field for field as the V7R1M0 oracle's
//! `DSPFFD` shows them and filled the way its records are
//! (`tools/conformance/probes/audit71/outfiles-*.tn`).

use cmd::block::Parms;
use licpgm::ProductStore as _;
use ptf::{Action, PtfRecord, Status};
use sec::cmd::object::ObjectAuthorities;
use store::obj::header::Authority as A;

use super::savout::{Fld, Row, stamp, write, write_to};
use crate::ipl::system::{System, now_unix};

/// `QSCPTF`, 48 fields, 323 bytes.
const QSCPTF: &[Fld] = &[
    ("SCCENTRY", 'A', 1, 0),
    ("SCDATE", 'A', 6, 0),
    ("SCTIME", 'A', 6, 0),
    ("SCSRLNBR", 'A', 8, 0),
    ("SCXPFID", 'A', 7, 0),
    ("SCXPFREL", 'A', 3, 0),
    ("SCXPFMOD", 'A', 3, 0),
    ("SCXPFDAT", 'A', 6, 0),
    ("SCLIB", 'A', 10, 0),
    ("SCPPID", 'A', 7, 0),
    ("SCPTFID", 'A', 7, 0),
    ("SCPPREL", 'A', 6, 0),
    ("SCSTATUS", 'A', 37, 0),
    ("SCSTDATE", 'A', 8, 0),
    ("SCSTTIME", 'A', 8, 0),
    ("SCTYPE", 'A', 18, 0),
    ("SCIPLACT", 'A', 34, 0),
    ("SCLANG", 'A', 8, 0),
    ("SCOBJLIB", 'A', 10, 0),
    ("SCISRC", 'A', 8, 0),
    ("SCSYSNM", 'A', 8, 0),
    ("SCONOR", 'A', 1, 0),
    ("SCSAVF", 'A', 1, 0),
    ("SCOPTP", 'A', 4, 0),
    ("SCSPTF", 'A', 7, 0),
    ("SCENST", 'A', 1, 0),
    ("SCENDT", 'A', 6, 0),
    ("SCENTP", 'A', 1, 0),
    ("SCENIA", 'A', 1, 0),
    ("SCENLG", 'A', 4, 0),
    ("SCXPFV", 'A', 6, 0),
    ("SCPTFV", 'A', 6, 0),
    ("SCMCGG", 'A', 7, 0),
    ("SCACTP", 'A', 8, 0),
    ("SCEACP", 'A', 1, 0),
    ("SCAACR", 'A', 15, 0),
    ("SCEACR", 'A', 1, 0),
    ("SCTGTV", 'A', 6, 0),
    ("SCXPFCEN", 'A', 1, 0),
    ("SCSTCEN", 'A', 1, 0),
    ("SCXPFLVL", 'A', 2, 0),
    ("SCMNLV", 'A', 2, 0),
    ("SCMXLV", 'A', 2, 0),
    ("SCSSBY", 'A', 7, 0),
    ("SCSVIPLS", 'A', 8, 0),
    ("SCSVIPLR", 'A', 1, 0),
    ("SCCRTDAT", 'A', 8, 0),
    ("SCCRTTIM", 'A', 6, 0),
];

/// `QARZLCGD`, 43 fields, 311 bytes.
const QARZLCGD: &[Fld] = &[
    ("LCCEN", 'A', 1, 0),
    ("LCDDAT", 'A', 6, 0),
    ("LCSYSN", 'A', 8, 0),
    ("LCDBRT", 'A', 1, 0),
    ("LCRSV1", 'A', 7, 0),
    ("LCLCGN", 'A', 10, 0),
    ("LCRSV2", 'A', 6, 0),
    ("LCLCGL", 'A', 10, 0),
    ("LCRSV3", 'A', 6, 0),
    ("LCPRDI", 'A', 7, 0),
    ("LCRSV4", 'A', 2, 0),
    ("LCLVLI", 'A', 2, 0),
    ("LCRSV8", 'A', 1, 0),
    ("LCPFGI", 'A', 4, 0),
    ("LCSFGI", 'A', 4, 0),
    ("LCVRSL", 'A', 4, 0),
    ("LCRLSL", 'A', 2, 0),
    ("LCMODL", 'A', 2, 0),
    ("LCPCID", 'A', 2, 0),
    ("LCCLEN", 'S', 2, 0),
    ("LCCPYR", 'A', 80, 0),
    ("LCRPLU", 'S', 4, 0),
    ("LCRUNM", 'A', 10, 0),
    ("LCRSV5", 'A', 1, 0),
    ("LCRUTY", 'A', 3, 0),
    ("LCOBJT", 'A', 2, 0),
    ("LCRULB", 'A', 10, 0),
    ("LCRSV6", 'A', 6, 0),
    ("LCRUDI", 'A', 8, 0),
    ("LCRUDC", 'A', 8, 0),
    ("LCRUTI", 'A', 8, 0),
    ("LCRUTC", 'A', 8, 0),
    ("LCRUCT", 'A', 4, 0),
    ("LCRUDV", 'A', 4, 0),
    ("LCPCLV", 'A', 4, 0),
    ("LCRUSZ", 'S', 10, 0),
    ("LCRSV7", 'A', 10, 0),
    ("LCDTMI", 'A', 7, 0),
    ("LCDTMF", 'A', 10, 0),
    ("LCDTML", 'A', 10, 0),
    ("LCFTYP", 'A', 1, 0),
    ("LCNMUT", 'S', 10, 0),
    ("LCUTSZ", 'S', 6, 0),
];

/// `QSYDSAUT`, 27 fields, 148 bytes.
const QSYDSAUT: &[Fld] = &[
    ("OADCEN", 'A', 1, 0),
    ("OADDAT", 'A', 6, 0),
    ("OADTIM", 'A', 6, 0),
    ("OAUSR", 'A', 10, 0),
    ("OAOBJA", 'A', 8, 0),
    ("OAOPR", 'A', 1, 0),
    ("OAOMGT", 'A', 1, 0),
    ("OAEXS", 'A', 1, 0),
    ("OAREAD", 'A', 1, 0),
    ("OAADD", 'A', 1, 0),
    ("OAUPD", 'A', 1, 0),
    ("OADLT", 'A', 1, 0),
    ("OAAMGT", 'A', 1, 0),
    ("OAANAM", 'A', 10, 0),
    ("OANAME", 'A', 10, 0),
    ("OALIB", 'A', 10, 0),
    ("OATYPE", 'A', 8, 0),
    ("OAOWN", 'A', 10, 0),
    ("OASYST", 'A', 8, 0),
    ("OAPGRP", 'A', 10, 0),
    ("OAGRPN", 'A', 10, 0),
    ("OAEXEC", 'A', 1, 0),
    ("OAALT", 'A', 1, 0),
    ("OAREF", 'A', 1, 0),
    ("OARES1", 'A', 10, 0),
    ("OAASPL", 'A', 10, 0),
    ("OAASPO", 'A', 10, 0),
];

/// The run's stamp split: century, `YYMMDD`, `HHMMSS`.
fn now_parts() -> (String, String, String) {
    let st = stamp(now_unix());
    (
        st[..1].to_string(),
        st[1..7].to_string(),
        st[7..13].to_string(),
    )
}

/// `VxRyMz` as `(00vv, rr, mm)`, the product levels of `QARZLCGD`.
fn vrm(release: &str) -> (String, String, String) {
    let num = |c: char| {
        release
            .split(c)
            .nth(1)
            .map(|s| {
                s.chars()
                    .take_while(char::is_ascii_digit)
                    .collect::<String>()
            })
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0)
    };
    (
        format!("{:04}", num('V')),
        format!("{:02}", num('R')),
        format!("{:02}", num('M')),
    )
}

/// What every `QSCPTF` record of one run shares.
struct PtfRun {
    cen: String,
    date: String,
    time: String,
    serial: String,
    sysname: String,
    isrc: String,
    level: String,
    /// The operating system's release date, `YYMMDD`.
    rel_date: String,
    /// The product asked for, for a record without one.
    product: String,
}

/// `SCENST`: the status as one letter. 7.1 showed `P` and `S`; the
/// rest follow the same initials.
fn status_code(s: Status) -> &'static str {
    match s {
        Status::PermApplied => "P",
        Status::Superseded => "S",
        Status::TempApplied => "T",
        Status::PermRemoved => "R",
        Status::Damaged => "D",
        Status::Loaded | Status::TempRemoved => "N",
        _ => "",
    }
}

/// One `QSCPTF` record, filled as 7.1 fills them: the unused release
/// fields `9`s, a superseded PTF without status date, type or levels.
fn ptf_row(run: &PtfRun, p: &PtfRecord) -> Row {
    let superseded = p.status == Status::Superseded;
    let dated = p.changed != 0 && !superseded;
    let st = stamp(p.changed);
    let blank_if = |c: bool, v: String| if c { String::new() } else { v };
    let mdy = format!("{}/{}/{}", &st[3..5], &st[5..7], &st[1..3]);
    let hms = format!("{}:{}:{}", &st[7..9], &st[9..11], &st[11..13]);
    let action = match p.action.display() {
        "" => "None",
        a => a,
    };
    let ia = match p.action {
        Action::None => "",
        Action::ApplyAtIpl => "A",
        Action::RemoveAtIpl => "R",
    };
    let opt = p.option.trim().trim_start_matches('*');
    let opt = opt
        .parse::<u32>()
        .map_or_else(|_| "0000".into(), |n| format!("{n:04}"));
    let product = if p.product.is_empty() {
        run.product.clone()
    } else {
        p.product.clone()
    };
    let lib = if release::product_id(&product) == release::PRODUCT_ID {
        "QSYS"
    } else {
        "QGPL"
    };
    let ty = if p.delayed {
        ("Delayed", "D")
    } else {
        ("Immediate", "I")
    };
    let serial: String = run
        .serial
        .trim()
        .chars()
        .rev()
        .take(8)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    vec![
        ("SCCENTRY", run.cen.clone()),
        ("SCDATE", run.date.clone()),
        ("SCTIME", run.time.clone()),
        ("SCSRLNBR", format!("{serial:>8}")),
        ("SCXPFID", release::PRODUCT_ID.into()),
        ("SCXPFREL", "999".into()),
        ("SCXPFMOD", "999".into()),
        ("SCXPFDAT", run.rel_date.clone()),
        ("SCLIB", lib.into()),
        ("SCPPID", product),
        ("SCPTFID", p.ptf.clone()),
        ("SCPPREL", "999999".into()),
        ("SCSTATUS", p.status.display().into()),
        ("SCSTDATE", blank_if(!dated, mdy)),
        ("SCSTTIME", blank_if(!dated, hms)),
        ("SCTYPE", blank_if(superseded, ty.0.into())),
        ("SCIPLACT", action.into()),
        ("SCLANG", "None".into()),
        ("SCOBJLIB", lib.into()),
        ("SCISRC", run.isrc.clone()),
        ("SCSYSNM", run.sysname.clone()),
        ("SCONOR", "N".into()),
        ("SCSAVF", if p.savf.is_empty() { "N" } else { "Y" }.into()),
        ("SCOPTP", opt),
        ("SCSPTF", p.superseded_by.clone()),
        ("SCENST", status_code(p.status).into()),
        ("SCENDT", blank_if(!dated, st[1..7].to_string())),
        ("SCENTP", blank_if(superseded, ty.1.into())),
        ("SCENIA", ia.into()),
        ("SCENLG", "CODE".into()),
        ("SCXPFV", run.level.clone()),
        ("SCPTFV", run.level.clone()),
        ("SCACTP", "No".into()),
        ("SCEACP", "0".into()),
        ("SCAACR", "No".into()),
        ("SCEACR", "0".into()),
        ("SCTGTV", blank_if(superseded, p.release.clone())),
        ("SCXPFCEN", run.cen.clone()),
        ("SCSTCEN", blank_if(!dated, st[..1].to_string())),
        ("SCXPFLVL", "00".into()),
        ("SCMNLV", blank_if(superseded, "00".into())),
        ("SCMXLV", blank_if(superseded, "00".into())),
        ("SCSSBY", p.superseded_by.clone()),
        ("SCSVIPLS", "##SERV#P".into()),
        ("SCSVIPLR", "N".into()),
    ]
}

/// The operating system's release date as `YYMMDD`, from its product
/// definition; blank when it has none.
fn os_release_date(system: &mut System) -> String {
    crate::stgsvc::licpgm::with(system, |p, _, _| {
        Ok(p.definitions()
            .into_iter()
            .map(|(_, d)| d)
            .find(|d| {
                d.product.as_str() == release::PRODUCT_ID
                    || d.ibm_alias
                        .as_ref()
                        .is_some_and(|a| a.as_str() == release::PRODUCT_ID)
            })
            .and_then(|d| d.release_date)
            .map(|d| format!("{:02}{:02}{:02}", d.year % 100, d.month, d.day))
            .unwrap_or_default())
    })
    .unwrap_or_default()
}

/// `DSPPTF OUTPUT(*OUTFILE)`: one `QSCPTF` record per PTF listed.
pub fn ptfs(
    system: &mut System,
    parms: &Parms,
    product: &str,
    rows: &[&PtfRecord],
) -> Result<String, String> {
    let (cen, date, time) = now_parts();
    let (v, r, m) = vrm(&release::RELEASE.to_string());
    let run = PtfRun {
        cen,
        date,
        time,
        serial: crate::stgsvc::security::sysval_text(system, "QSRLNBR"),
        sysname: system.system_name(),
        isrc: format!("##MACH#{:?}", system.boot.slot),
        level: format!("{}{r}{m}", &v[2..]),
        rel_date: os_release_date(system),
        product: product.to_string(),
    };
    let out: Vec<Row> = rows.iter().map(|p| ptf_row(&run, p)).collect();
    write(system, parms, "QSCPTF", QSCPTF, &out)
}

/// One `QARZLCGD` record (type 2, a product load) of a display row:
/// product, option, feature, text, load type, library, release, load
/// object, message ID, message file and library, copyright years. As
/// 7.1: the copyright line starts with the product (`5770-SS1`), the
/// option level only on `*BASE`, units of 512 bytes.
fn sfwrsc_row(cen: &str, date: &str, sysname: &str, r: &[String]) -> Row {
    let get = |i: usize| r.get(i).cloned().unwrap_or_default();
    let product = get(0);
    let opt = get(1);
    let base = opt == "*BASE";
    let opt = opt
        .parse::<u32>()
        .map_or_else(|_| "0000".into(), |n| format!("{n:04}"));
    let (v, rl, m) = vrm(&get(6));
    let pid = if product.len() == 7 {
        format!("{}-{}", &product[..4], &product[4..])
    } else {
        product.clone()
    };
    let years = match get(11) {
        y if y.is_empty() => String::new(),
        y => format!(" {y}"),
    };
    let copyright = format!("{pid} (C) COPYRIGHT QUARTZ SYSTEMS{years}");
    vec![
        ("LCCEN", cen.into()),
        ("LCDDAT", date.into()),
        ("LCSYSN", sysname.into()),
        ("LCDBRT", "2".into()),
        ("LCPRDI", product),
        ("LCLVLI", if base { "00" } else { "" }.into()),
        ("LCPFGI", get(2)),
        ("LCSFGI", opt),
        ("LCVRSL", v),
        ("LCRLSL", rl),
        ("LCMODL", m),
        ("LCCLEN", copyright.len().to_string()),
        ("LCCPYR", copyright),
        ("LCRUNM", get(7)),
        ("LCRUTY", "LOD".into()),
        ("LCRULB", get(5)),
        ("LCDTMI", get(8)),
        ("LCDTMF", get(9)),
        ("LCDTML", get(10)),
        ("LCFTYP", if get(4) == "*LNG" { "3" } else { "1" }.into()),
        ("LCUTSZ", "512".into()),
    ]
}

/// `DSPSFWRSC OUTPUT(*OUTFILE)`: one `QARZLCGD` record per row of
/// [`super::lpcmd::sfwrsc_full`].
pub fn sfwrsc(system: &mut System, parms: &Parms, rows: &[Vec<String>]) -> Result<String, String> {
    let (cen, date, _) = now_parts();
    let sysname = system.system_name();
    let out: Vec<Row> = rows
        .iter()
        .map(|r| sfwrsc_row(&cen, &date, &sysname, r))
        .collect();
    write(system, parms, "QARZLCGD", QARZLCGD, &out)
}

/// The `QSYDSAUT` records of `a`, stamped `CYYMMDDHHMMSS` on `sysname`.
/// As 7.1: the date `MMDDYY`, the reserved field's first byte null.
fn objaut_rows(st: &str, sysname: &str, a: &ObjectAuthorities) -> Vec<Row> {
    let mdy = format!("{}{}{}", &st[3..5], &st[5..7], &st[1..3]);
    a.rows
        .iter()
        .map(|r| {
            let x = |m: u32| if r.bits.0 & m != 0 { "X" } else { "" }.to_string();
            vec![
                ("OADCEN", st[..1].to_string()),
                ("OADDAT", mdy.clone()),
                ("OADTIM", st[7..13].to_string()),
                ("OAUSR", r.user.clone()),
                ("OAOBJA", r.shown.clone()),
                ("OAOPR", x(A::OBJOPR)),
                ("OAOMGT", x(A::OBJMGT)),
                ("OAEXS", x(A::OBJEXIST)),
                ("OAREAD", x(A::READ)),
                ("OAADD", x(A::ADD)),
                ("OAUPD", x(A::UPD)),
                ("OADLT", x(A::DLT)),
                ("OAAMGT", x(A::AUTLMGT)),
                ("OAANAM", a.autl.clone()),
                ("OANAME", a.name.clone()),
                ("OALIB", a.lib.clone()),
                ("OATYPE", a.ty.clone()),
                ("OAOWN", a.owner.clone()),
                ("OASYST", sysname.into()),
                ("OAPGRP", a.pgp.clone()),
                ("OAGRPN", r.group.clone()),
                ("OAEXEC", x(A::EXECUTE)),
                ("OAALT", x(A::OBJALTER)),
                ("OAREF", x(A::OBJREF)),
                ("OARES1", "\u{0}".into()),
                ("OAASPL", "*SYSBAS".into()),
                ("OAASPO", "*SYSBAS".into()),
            ]
        })
        .collect()
}

/// `DSPOBJAUT OUTPUT(*OUTFILE)`: one `QSYDSAUT` record per user shown,
/// to file `(name, lib, replace)`.
pub fn objaut(
    system: &mut System,
    file: (&str, &str, bool),
    a: &ObjectAuthorities,
) -> Result<String, String> {
    let out = objaut_rows(&stamp(now_unix()), &system.system_name(), a);
    write_to(system, file, "QSYDSAUT", QSYDSAUT, &out)
}

#[cfg(test)]
mod tests {
    use super::super::savout;
    use super::*;

    fn len(f: &[Fld]) -> usize {
        f.iter()
            .map(|(_, t, l, _)| {
                if *t == 'P' {
                    usize::from(*l) / 2 + 1
                } else {
                    usize::from(*l)
                }
            })
            .sum()
    }

    #[test]
    fn the_formats_are_the_oracles() {
        assert_eq!((QSCPTF.len(), len(QSCPTF)), (48, 323));
        assert_eq!((QARZLCGD.len(), len(QARZLCGD)), (43, 311));
        assert_eq!((QSYDSAUT.len(), len(QSYDSAUT)), (27, 148));
    }

    #[test]
    fn product_levels() {
        assert_eq!(vrm("V7R1M0"), ("0007".into(), "01".into(), "00".into()));
        assert_eq!(vrm("V1R0M0"), ("0001".into(), "00".into(), "00".into()));
    }

    macro_rules! snaps {
        ($dir:literal, $($f:literal),+) => {
            &[$(include_str!(concat!(
                "../../../../tools/conformance/golden/verified/oracle71/audit/", $dir, "/", $f, ".txt"
            ))),+]
        };
    }

    /// Record `row` of a 7.1 `DSPPFM` capture, its screens shifted with
    /// F20 joined by column, cut to `len`.
    fn oracle(snaps: &[&str], row: usize, len: usize) -> String {
        let mut at = std::collections::BTreeMap::new();
        for s in snaps {
            let lines: Vec<&str> = s.lines().collect();
            let col: usize = lines[3].rsplit(':').next().unwrap().trim().parse().unwrap();
            let data: Vec<char> = lines[6 + row].chars().skip(1).take(78).collect();
            for (i, ch) in data.into_iter().enumerate() {
                at.entry(col + i).or_insert(ch);
            }
        }
        (1..=len)
            .map(|c| at.get(&c).copied().unwrap_or(' '))
            .collect()
    }

    /// Our record of `row` as `DSPPFM` shows it (a null as a blank).
    fn ours(format: &str, fields: &[Fld], row: &Row) -> (Vec<u8>, String) {
        let rec = savout::encode(&savout::model(format, fields), row).unwrap();
        let text = ccsid::from_ebcdic(&rec).replace('\0', " ");
        (rec, text)
    }

    /// `rec` split into `fields`, those named in `mask` left out.
    fn split<'a>(fields: &'a [Fld], rec: &str, mask: &[&str]) -> Vec<(&'a str, String)> {
        let chars: Vec<char> = rec.chars().collect();
        let mut at = 0;
        let mut out = Vec::new();
        for (name, _, l, _) in fields {
            let l = usize::from(*l);
            if !mask.contains(name) {
                out.push((*name, chars[at..at + l].iter().collect()));
            }
            at += l;
        }
        out
    }

    /// Field `name` of record `rec`.
    fn field(fields: &[Fld], rec: &str, name: &str) -> String {
        split(fields, rec, &[])
            .into_iter()
            .find(|(n, _)| *n == name)
            .unwrap()
            .1
    }

    fn ptf(
        id: &str,
        product: &str,
        status: Status,
        delayed: bool,
        by: &str,
        changed: u64,
    ) -> PtfRecord {
        PtfRecord {
            ptf: id.into(),
            product: product.into(),
            option: "*BASE".into(),
            release: "V7R1M0".into(),
            status,
            action: Action::None,
            delayed,
            lic: product == "5770999",
            savf: String::new(),
            superseded_by: by.into(),
            prereqs: Vec::new(),
            coreqs: Vec::new(),
            replaced: Vec::new(),
            changed,
            loaded: changed,
        }
    }

    #[test]
    fn qscptf_records_are_shaped_as_7_1s() {
        let data = snaps!(
            "outdata",
            "outf-ptf-1",
            "outf-ptf-2",
            "outf-ptf-3",
            "outf-ptf-4",
            "outf-ptf-5"
        );
        let codes = snaps!("outcodes", "ptfc-1", "ptfc-2", "ptfc-3", "ptfc-4", "ptfc-5");
        let cases = [
            (
                data,
                0,
                ptf(
                    "RE16082",
                    "5770999",
                    Status::PermApplied,
                    false,
                    "",
                    1_458_564_024,
                ),
            ),
            (
                codes,
                0,
                ptf(
                    "RE11195",
                    "5770999",
                    Status::Superseded,
                    false,
                    "RE11221",
                    1_458_564_024,
                ),
            ),
            (
                codes,
                2,
                ptf(
                    "MF99011",
                    "5770999",
                    Status::PermApplied,
                    true,
                    "",
                    1_443_522_212,
                ),
            ),
            (
                codes,
                4,
                ptf(
                    "SI42904",
                    "5770SS1",
                    Status::PermApplied,
                    false,
                    "",
                    1_299_742_815,
                ),
            ),
        ];
        for (snaps, row, p) in cases {
            let theirs = oracle(snaps, row, 323);
            let f = |n: &str| field(QSCPTF, &theirs, n);
            // What the machine and the run decide comes from the oracle's record.
            let run = PtfRun {
                cen: f("SCCENTRY"),
                date: f("SCDATE"),
                time: f("SCTIME"),
                serial: f("SCSRLNBR"),
                sysname: f("SCSYSNM"),
                isrc: f("SCISRC"),
                level: f("SCXPFV"),
                rel_date: f("SCXPFDAT"),
                product: String::new(),
            };
            let (_, mine) = ours("QSCPTF", QSCPTF, &ptf_row(&run, &p));
            // The LIC group and the creation stamp are IBM's own; the
            // operating system's product ID is Quartz's.
            let mask = ["SCMCGG", "SCCRTDAT", "SCCRTTIM", "SCXPFID"];
            assert_eq!(
                split(QSCPTF, &mine, &mask),
                split(QSCPTF, &theirs, &mask),
                "{}",
                p.ptf
            );
            assert_eq!(field(QSCPTF, &mine, "SCXPFID"), release::PRODUCT_ID);
        }
    }

    #[test]
    fn qarzlcgd_records_are_shaped_as_7_1s() {
        let snaps = snaps!(
            "outdata",
            "outf-sfw-1",
            "outf-sfw-2",
            "outf-sfw-3",
            "outf-sfw-4",
            "outf-sfw-5"
        );
        let row = |v: [&str; 12]| v.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        let cases = [
            (
                1,
                row([
                    "5770SS1", "*BASE", "5050", "", "*CODE", "QSYS", "V7R1M0", "QSZ0050",
                    "CPX0C0C", "QCPFMSG", "QSYS", "",
                ]),
            ),
            (
                2,
                row([
                    "5770SS1", "*BASE", "2924", "", "*LNG", "QSYS", "V7R1M0", "QSZ0029", "CPX0C0C",
                    "QCPFMSG", "QSYS", "",
                ]),
            ),
            (
                3,
                row([
                    "5770SS1", "0001", "5050", "", "*CODE", "QSYS2", "V7R1M0", "QLP0150",
                    "CPX0C02", "QCPFMSG", "QSYS", "",
                ]),
            ),
        ];
        for (n, r) in cases {
            let theirs = oracle(snaps, n, 311);
            let f = |k: &str| field(QARZLCGD, &theirs, k);
            let (_, mine) = ours(
                "QARZLCGD",
                QARZLCGD,
                &sfwrsc_row(&f("LCCEN"), &f("LCDDAT"), &f("LCSYSN"), &r),
            );
            // The copyright line is each vendor's; sizes are the machine's.
            let mask = ["LCCLEN", "LCCPYR", "LCRUSZ", "LCNMUT"];
            assert_eq!(
                split(QARZLCGD, &mine, &mask),
                split(QARZLCGD, &theirs, &mask),
                "row {n}"
            );
            let cpyr = field(QARZLCGD, &mine, "LCCPYR");
            assert_eq!(&cpyr[..9], &field(QARZLCGD, &theirs, "LCCPYR")[..9]);
        }
    }

    #[test]
    fn qsydsaut_records_are_shaped_as_7_1s() {
        use sec::auth::bits::Auth;
        use sec::cmd::object::AuthRow;
        let snaps = snaps!("outdata", "outf-oaut-1", "outf-oaut-2");
        let theirs = oracle(snaps, 0, 148);
        let f = |k: &str| field(QSYDSAUT, &theirs, k);
        let d = f("OADDAT");
        let st = format!("{}{}{}{}", f("OADCEN"), &d[4..6], &d[..4], f("OADTIM"));
        let row = |user: &str, shown: &str, bits| AuthRow {
            user: user.into(),
            group: String::new(),
            shown: shown.into(),
            bits,
        };
        let a = ObjectAuthorities {
            name: "QCMD".into(),
            lib: "QSYS".into(),
            ty: "*PGM".into(),
            owner: "QSYS".into(),
            pgp: "*NONE".into(),
            autl: "*NONE".into(),
            rows: vec![
                row("*PUBLIC", "*USE", Auth::USE),
                row("QSYS", "*ALL", Auth::ALL),
            ],
        };
        let rows = objaut_rows(&st, &f("OASYST"), &a);
        for (i, r) in rows.iter().enumerate() {
            let (bytes, mine) = ours("QSYDSAUT", QSYDSAUT, r);
            assert_eq!(mine, oracle(snaps, i, 148), "row {i}");
            // The hex view: the reserved field starts with a null.
            assert_eq!(bytes[118], 0);
            assert_eq!(bytes[119], 0x40);
        }
    }
}
