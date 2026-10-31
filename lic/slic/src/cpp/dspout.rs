// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `OUTPUT(*OUTFILE)` of `DSPPTF`, `DSPSFWRSC` and `DSPOBJAUT`: IBM's
//! model formats `QSCPTF` (`QADSPPTF`), `QARZLCGD` (`QARZLCOF`) and
//! `QSYDSAUT` (`QAOBJAUT`), field for field as the V7R1M0 oracle's
//! `DSPFFD` shows them and filled the way its records are
//! (`tools/conformance/probes/audit71/outfiles-*.tn`).

use cmd::block::Parms;
use ptf::PtfRecord;
use sec::cmd::object::ObjectAuthorities;
use store::obj::header::Authority as A;

use super::savout::{Fld, Row, stamp, write, write_to};
use crate::ipl::system::{System, now_unix};

/// `QSCPTF`, 48 fields, 323 bytes.
const QSCPTF: &[Fld] = &[
    ("SCCENTRY", 'A', 1, 0), ("SCDATE", 'A', 6, 0), ("SCTIME", 'A', 6, 0),
    ("SCSRLNBR", 'A', 8, 0), ("SCXPFID", 'A', 7, 0), ("SCXPFREL", 'A', 3, 0),
    ("SCXPFMOD", 'A', 3, 0), ("SCXPFDAT", 'A', 6, 0), ("SCLIB", 'A', 10, 0),
    ("SCPPID", 'A', 7, 0), ("SCPTFID", 'A', 7, 0), ("SCPPREL", 'A', 6, 0),
    ("SCSTATUS", 'A', 37, 0), ("SCSTDATE", 'A', 8, 0), ("SCSTTIME", 'A', 8, 0),
    ("SCTYPE", 'A', 18, 0), ("SCIPLACT", 'A', 34, 0), ("SCLANG", 'A', 8, 0),
    ("SCOBJLIB", 'A', 10, 0), ("SCISRC", 'A', 8, 0), ("SCSYSNM", 'A', 8, 0),
    ("SCONOR", 'A', 1, 0), ("SCSAVF", 'A', 1, 0), ("SCOPTP", 'A', 4, 0),
    ("SCSPTF", 'A', 7, 0), ("SCENST", 'A', 1, 0), ("SCENDT", 'A', 6, 0),
    ("SCENTP", 'A', 1, 0), ("SCENIA", 'A', 1, 0), ("SCENLG", 'A', 4, 0),
    ("SCXPFV", 'A', 6, 0), ("SCPTFV", 'A', 6, 0), ("SCMCGG", 'A', 7, 0),
    ("SCACTP", 'A', 8, 0), ("SCEACP", 'A', 1, 0), ("SCAACR", 'A', 15, 0),
    ("SCEACR", 'A', 1, 0), ("SCTGTV", 'A', 6, 0), ("SCXPFCEN", 'A', 1, 0),
    ("SCSTCEN", 'A', 1, 0), ("SCXPFLVL", 'A', 2, 0), ("SCMNLV", 'A', 2, 0),
    ("SCMXLV", 'A', 2, 0), ("SCSSBY", 'A', 7, 0), ("SCSVIPLS", 'A', 8, 0),
    ("SCSVIPLR", 'A', 1, 0), ("SCCRTDAT", 'A', 8, 0), ("SCCRTTIM", 'A', 6, 0),
];

/// `QARZLCGD`, 43 fields, 311 bytes.
const QARZLCGD: &[Fld] = &[
    ("LCCEN", 'A', 1, 0), ("LCDDAT", 'A', 6, 0), ("LCSYSN", 'A', 8, 0),
    ("LCDBRT", 'A', 1, 0), ("LCRSV1", 'A', 7, 0), ("LCLCGN", 'A', 10, 0),
    ("LCRSV2", 'A', 6, 0), ("LCLCGL", 'A', 10, 0), ("LCRSV3", 'A', 6, 0),
    ("LCPRDI", 'A', 7, 0), ("LCRSV4", 'A', 2, 0), ("LCLVLI", 'A', 2, 0),
    ("LCRSV8", 'A', 1, 0), ("LCPFGI", 'A', 4, 0), ("LCSFGI", 'A', 4, 0),
    ("LCVRSL", 'A', 4, 0), ("LCRLSL", 'A', 2, 0), ("LCMODL", 'A', 2, 0),
    ("LCPCID", 'A', 2, 0), ("LCCLEN", 'S', 2, 0), ("LCCPYR", 'A', 80, 0),
    ("LCRPLU", 'S', 4, 0), ("LCRUNM", 'A', 10, 0), ("LCRSV5", 'A', 1, 0),
    ("LCRUTY", 'A', 3, 0), ("LCOBJT", 'A', 2, 0), ("LCRULB", 'A', 10, 0),
    ("LCRSV6", 'A', 6, 0), ("LCRUDI", 'A', 8, 0), ("LCRUDC", 'A', 8, 0),
    ("LCRUTI", 'A', 8, 0), ("LCRUTC", 'A', 8, 0), ("LCRUCT", 'A', 4, 0),
    ("LCRUDV", 'A', 4, 0), ("LCPCLV", 'A', 4, 0), ("LCRUSZ", 'S', 10, 0),
    ("LCRSV7", 'A', 10, 0), ("LCDTMI", 'A', 7, 0), ("LCDTMF", 'A', 10, 0),
    ("LCDTML", 'A', 10, 0), ("LCFTYP", 'A', 1, 0), ("LCNMUT", 'S', 10, 0),
    ("LCUTSZ", 'S', 6, 0),
];

/// `QSYDSAUT`, 27 fields, 148 bytes.
const QSYDSAUT: &[Fld] = &[
    ("OADCEN", 'A', 1, 0), ("OADDAT", 'A', 6, 0), ("OADTIM", 'A', 6, 0),
    ("OAUSR", 'A', 10, 0), ("OAOBJA", 'A', 8, 0), ("OAOPR", 'A', 1, 0),
    ("OAOMGT", 'A', 1, 0), ("OAEXS", 'A', 1, 0), ("OAREAD", 'A', 1, 0),
    ("OAADD", 'A', 1, 0), ("OAUPD", 'A', 1, 0), ("OADLT", 'A', 1, 0),
    ("OAAMGT", 'A', 1, 0), ("OAANAM", 'A', 10, 0), ("OANAME", 'A', 10, 0),
    ("OALIB", 'A', 10, 0), ("OATYPE", 'A', 8, 0), ("OAOWN", 'A', 10, 0),
    ("OASYST", 'A', 8, 0), ("OAPGRP", 'A', 10, 0), ("OAGRPN", 'A', 10, 0),
    ("OAEXEC", 'A', 1, 0), ("OAALT", 'A', 1, 0), ("OAREF", 'A', 1, 0),
    ("OARES1", 'A', 10, 0), ("OAASPL", 'A', 10, 0), ("OAASPO", 'A', 10, 0),
];

/// The run's stamp split: century, `YYMMDD`, `HHMMSS`.
fn now_parts() -> (String, String, String) {
    let st = stamp(now_unix());
    (st[..1].to_string(), st[1..7].to_string(), st[7..13].to_string())
}

/// `VxRyMz` as `(00vv, rr, mm)`, the product levels of `QARZLCGD`.
fn vrm(release: &str) -> (String, String, String) {
    let num = |c: char| {
        release
            .split(c)
            .nth(1)
            .map(|s| s.chars().take_while(char::is_ascii_digit).collect::<String>())
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0)
    };
    (format!("{:04}", num('V')), format!("{:02}", num('R')), format!("{:02}", num('M')))
}

/// `DSPPTF OUTPUT(*OUTFILE)`: one `QSCPTF` record per PTF listed.
pub fn ptfs(system: &mut System, parms: &Parms, product: &str, rows: &[&PtfRecord]) -> Result<String, String> {
    let (cen, date, time) = now_parts();
    let sysname = system.system_name();
    let serial = crate::stgsvc::security::sysval_text(system, "QSRLNBR");
    let isrc = format!("##MACH#{:?}", system.boot.slot);
    let rel = release::RELEASE.to_string();
    let (v, r, m) = vrm(&rel);
    let level = format!("{}{r}{m}", &v[2..]);
    let out: Vec<Row> = rows
        .iter()
        .map(|p| {
            let st = stamp(p.changed);
            let mdy = format!("{}/{}/{}", &st[3..5], &st[5..7], &st[1..3]);
            let hms = format!("{}:{}:{}", &st[7..9], &st[9..11], &st[11..13]);
            let action = match p.action.display() {
                "" => "None",
                a => a,
            };
            let opt = p.option.trim().trim_start_matches('*');
            let opt = opt.parse::<u32>().map_or_else(|_| "0000".into(), |n| format!("{n:04}"));
            vec![
                ("SCCENTRY", cen.clone()),
                ("SCDATE", date.clone()),
                ("SCTIME", time.clone()),
                ("SCSRLNBR", serial.chars().take(8).collect()),
                ("SCXPFID", release::PRODUCT_ID.into()),
                ("SCLIB", "QGPL".into()),
                ("SCPPID", product.to_string()),
                ("SCPTFID", p.ptf.clone()),
                ("SCSTATUS", p.status.display().into()),
                ("SCSTDATE", if p.changed == 0 { String::new() } else { mdy }),
                ("SCSTTIME", if p.changed == 0 { String::new() } else { hms }),
                ("SCTYPE", if p.delayed { "Delayed" } else { "Immediate" }.into()),
                ("SCIPLACT", action.into()),
                ("SCLANG", "None".into()),
                ("SCOBJLIB", "QGPL".into()),
                ("SCISRC", isrc.clone()),
                ("SCSYSNM", sysname.clone()),
                ("SCONOR", "N".into()),
                ("SCSAVF", if p.savf.is_empty() { "N" } else { "Y" }.into()),
                ("SCOPTP", opt),
                ("SCSPTF", p.superseded_by.clone()),
                ("SCXPFV", level.clone()),
                ("SCPTFV", level.clone()),
                ("SCTGTV", p.release.clone()),
                ("SCXPFCEN", cen.clone()),
                ("SCSTCEN", st[..1].to_string()),
                ("SCXPFLVL", "00".into()),
                ("SCMNLV", "00".into()),
                ("SCMXLV", "00".into()),
                ("SCSVIPLR", "N".into()),
            ]
        })
        .collect();
    write(system, parms, "QSCPTF", QSCPTF, &out)
}

/// `DSPSFWRSC OUTPUT(*OUTFILE)`: one `QARZLCGD` record (type 2, a
/// product load) per row of the display: product, option, feature,
/// text, load type, library, release.
pub fn sfwrsc(system: &mut System, parms: &Parms, rows: &[Vec<String>]) -> Result<String, String> {
    let (cen, date, _) = now_parts();
    let sysname = system.system_name();
    let copyright = "(C) COPYRIGHT QUARTZ SYSTEMS 2026".to_string();
    let out: Vec<Row> = rows
        .iter()
        .map(|r| {
            let get = |i: usize| r.get(i).cloned().unwrap_or_default();
            let opt = get(1);
            let base = opt == "*BASE";
            let opt = opt.parse::<u32>().map_or_else(|_| "0000".into(), |n| format!("{n:04}"));
            let (v, rl, m) = vrm(&get(6));
            vec![
                ("LCCEN", cen.clone()),
                ("LCDDAT", date.clone()),
                ("LCSYSN", sysname.clone()),
                ("LCDBRT", "2".into()),
                ("LCPRDI", get(0)),
                ("LCLVLI", if base { "00" } else { "" }.into()),
                ("LCPFGI", get(2)),
                ("LCSFGI", opt),
                ("LCVRSL", v),
                ("LCRLSL", rl),
                ("LCMODL", m),
                ("LCCLEN", copyright.len().to_string()),
                ("LCCPYR", copyright.clone()),
                ("LCRUTY", "LOD".into()),
                ("LCRULB", get(5)),
                ("LCFTYP", if get(4) == "*LNG" { "3" } else { "1" }.into()),
            ]
        })
        .collect();
    write(system, parms, "QARZLCGD", QARZLCGD, &out)
}

/// `DSPOBJAUT OUTPUT(*OUTFILE)`: one `QSYDSAUT` record per user shown,
/// to file `(name, lib, replace)`.
pub fn objaut(system: &mut System, file: (&str, &str, bool), a: &ObjectAuthorities) -> Result<String, String> {
    let st = stamp(now_unix());
    let (cen, mdy, time) = (
        st[..1].to_string(),
        format!("{}{}{}", &st[3..5], &st[5..7], &st[1..3]),
        st[7..13].to_string(),
    );
    let sysname = system.system_name();
    let out: Vec<Row> = a
        .rows
        .iter()
        .map(|r| {
            let x = |m: u32| if r.bits.0 & m != 0 { "X" } else { "" }.to_string();
            vec![
                ("OADCEN", cen.clone()),
                ("OADDAT", mdy.clone()),
                ("OADTIM", time.clone()),
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
                ("OASYST", sysname.clone()),
                ("OAPGRP", a.pgp.clone()),
                ("OAGRPN", r.group.clone()),
                ("OAEXEC", x(A::EXECUTE)),
                ("OAALT", x(A::OBJALTER)),
                ("OAREF", x(A::OBJREF)),
                ("OAASPL", "*SYSBAS".into()),
                ("OAASPO", "*SYSBAS".into()),
            ]
        })
        .collect();
    write_to(system, file, "QSYDSAUT", QSYDSAUT, &out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn len(f: &[Fld]) -> usize {
        f.iter()
            .map(|(_, t, l, _)| if *t == 'P' { usize::from(*l) / 2 + 1 } else { usize::from(*l) })
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
}
