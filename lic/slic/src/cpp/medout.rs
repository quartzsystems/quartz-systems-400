// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `OUTPUT(*OUTFILE)` of the media displays: IBM's model formats
//! `QTADOUTF` (`QSYS/QATADOF`, `DSPTAP`), `QTAVOUTF` (`QSYS/QATAVOF`,
//! `DSPTAPCTG`) and `QAMODVA`, `QAMODPA`, `QAMODFA` (`QSYS/QAMODVAF`,
//! `QAMODPAF`, `QAMODFAF`, `DSPOPT`), field for field as pub400's
//! `QSYS2.SYSCOLUMNS` lists them
//! (`tools/conformance/golden/audit/outfiles/MODELS.txt`).

use cmd::block::Parms;

use super::meddsp::{CtgView, OptView, TapView, opt_hms, opt_mdy};
use super::savout::{Fld, Row, stamp, write};
use crate::ipl::system::{System, now_unix};

/// `QTADOUTF`, `DSPTAP`'s labels.
const QTADOUTF: &[Fld] = &[
    ("RDSYSN", 'A', 8, 0),
    ("RDDTTM", 'A', 13, 0),
    ("RDRECT", 'A', 1, 0),
    ("RDDEVN", 'A', 10, 0),
    ("RDCTGI", 'A', 6, 0),
    ("RDVOLL", 'A', 80, 0),
    ("RDDENS", 'A', 10, 0),
    ("RDCODE", 'A', 1, 0),
    ("RDTRL1", 'A', 80, 0),
    ("RDTRL2", 'A', 80, 0),
    ("RDLBID", 'A', 32, 0),
    ("RDSLT", 'A', 1, 0),
    ("RDLTM", 'A', 1, 0),
    ("RDLFSQ", 'A', 10, 0),
    ("RDLASQ", 'A', 10, 0),
    ("RDFILT", 'A', 8, 0),
];

/// `QTAVOUTF`, `DSPTAPCTG`'s cartridges.
const QTAVOUTF: &[Fld] = &[
    ("RDSYSN", 'A', 8, 0),
    ("RDDTTM", 'A', 13, 0),
    ("RDMID", 'A', 6, 0),
    ("RDVOL", 'A', 6, 0),
    ("RDLIB", 'A', 10, 0),
    ("RDCGY", 'A', 10, 0),
    ("RDSYS", 'A', 8, 0),
    ("RDDEN", 'A', 10, 0),
    ("RDCD", 'A', 7, 0),
    ("RDCT", 'A', 6, 0),
    ("RDRD", 'A', 7, 0),
    ("RDRT", 'A', 6, 0),
    ("RDLOC", 'A', 10, 0),
    ("RDLOCI", 'A', 1, 0),
    ("RDVSTS", 'A', 2, 0),
    ("RDOWN", 'A', 17, 0),
    ("RDWPR", 'A', 1, 0),
    ("RDCODE", 'A', 1, 0),
    ("RDMGEN", 'A', 1, 0),
    ("RDMIE", 'A', 1, 0),
    ("RDMTYP", 'A', 2, 0),
    ("RDRSV", 'A', 2, 0),
];

/// `QAMODVA`, `DSPOPT DATA(*VOLATR)`.
const QAMODVA: &[Fld] = &[
    ("OVDCEN", 'A', 1, 0),
    ("OVDDAT", 'A', 6, 0),
    ("OVDTIM", 'A', 6, 0),
    ("OVNAME", 'A', 32, 0),
    ("OVLIBR", 'A', 10, 0),
    ("OVCSI", 'A', 8, 0),
    ("OVCSIL", 'A', 10, 0),
    ("OVALST", 'A', 10, 0),
    ("OVIVID", 'A', 32, 0),
    ("OVSERN", 'P', 11, 0),
    ("OVTYPE", 'P', 3, 0),
    ("OVCSID", 'A', 2, 0),
    ("OVMTYP", 'P', 3, 0),
    ("OVMFMT", 'P', 3, 0),
    ("OVVFTH", 'P', 5, 0),
    ("OVVSQN", 'P', 9, 0),
    ("OVCRDT", 'A', 7, 0),
    ("OVCRTM", 'A', 6, 0),
    ("OVTEXT", 'A', 50, 0),
    ("OVLRDT", 'A', 7, 0),
    ("OVOPVN", 'A', 32, 0),
    ("OVBSIZ", 'P', 9, 0),
    ("OVVSIZ", 'P', 13, 0),
    ("OVSPCA", 'P', 13, 0),
    ("OVVLOC", 'A', 1, 0),
    ("OVSFID", 'A', 50, 0),
    ("OVVACC", 'A', 1, 0),
    ("OVDBLV", 'A', 1, 0),
    ("OVDBLS", 'A', 1, 0),
    ("OVIPL", 'A', 1, 0),
    ("OVLSTV", 'A', 1, 0),
    ("OVHELD", 'A', 8, 0),
    ("OVRSRV", 'A', 15, 0),
    ("OVPNAM", 'A', 32, 0),
    ("OVPSER", 'P', 11, 0),
    ("OVCSDT", 'A', 7, 0),
    ("OVCSTM", 'A', 6, 0),
    ("OVCEDT", 'A', 7, 0),
    ("OVCETM", 'A', 6, 0),
    ("OVCHDT", 'A', 7, 0),
    ("OVCHTM", 'A', 6, 0),
    ("OVAMDT", 'A', 7, 0),
    ("OVAMTM", 'A', 6, 0),
    ("OVAXDT", 'A', 7, 0),
    ("OVAXTM", 'A', 6, 0),
    ("OVAEDT", 'A', 7, 0),
    ("OVAETM", 'A', 6, 0),
    ("OVCPYR", 'A', 37, 0),
    ("OVABST", 'A', 37, 0),
    ("OVBIBL", 'A', 37, 0),
    ("OVKEY1", 'A', 1, 0),
    ("OVPUBL", 'A', 128, 0),
    ("OVKEY2", 'A', 1, 0),
    ("OVPREP", 'A', 128, 0),
    ("OVKEY3", 'A', 1, 0),
    ("OVDATS", 'A', 128, 0),
    ("OVCVOL", 'A', 32, 0),
    ("OVICLG", 'A', 256, 0),
];

/// `QAMODPA`, `DSPOPT DATA(*DIRATR)`.
const QAMODPA: &[Fld] = &[
    ("OPDCEN", 'A', 1, 0),
    ("OPDDAT", 'A', 6, 0),
    ("OPDTIM", 'A', 6, 0),
    ("OPDIR", 'A', 256, 0),
    ("OPVID", 'A', 32, 0),
    ("OPLIB", 'A', 10, 0),
    ("OPCRDT", 'A', 7, 0),
    ("OPCRTM", 'A', 6, 0),
    ("OPRSVR", 'A', 25, 0),
];

/// `QAMODFA`, `DSPOPT DATA(*FILATR)`.
const QAMODFA: &[Fld] = &[
    ("OFDCEN", 'A', 1, 0),
    ("OFDDAT", 'A', 6, 0),
    ("OFDTIM", 'A', 6, 0),
    ("OFFILE", 'A', 256, 0),
    ("OFVID", 'A', 32, 0),
    ("OFLIB", 'A', 10, 0),
    ("OFSIZE", 'P', 9, 0),
    ("OFCRDT", 'A', 7, 0),
    ("OFCRTM", 'A', 6, 0),
    ("OFMDDT", 'A', 7, 0),
    ("OFMDTM", 'A', 6, 0),
    ("OFEXDT", 'A', 7, 0),
    ("OFEXTM", 'A', 6, 0),
    ("OFCNTP", 'A', 1, 0),
    ("OFCNTN", 'A', 1, 0),
    ("OFSVID", 'A', 32, 0),
    ("OFATNM", 'A', 25, 0),
    ("OFATDA", 'A', 75, 0),
    ("OFSIZE2", 'P', 15, 0),
    ("OFRSVR", 'A', 17, 0),
];

/// `CYYMMDD` of an `MM/DD/YY` date, blank for blank.
fn cyymmdd(mdy: &str) -> String {
    let p: Vec<&str> = mdy.split('/').collect();
    match p.as_slice() {
        [mm, dd, yy] => {
            let cent = if yy.parse::<u8>().unwrap_or(0) < 40 {
                '1'
            } else {
                '0'
            };
            format!("{cent}{yy}{mm}{dd}")
        }
        _ => String::new(),
    }
}

/// `HHMMSS` of `HH:MM:SS`.
fn hhmmss(t: &str) -> String {
    t.replace(':', "")
}

/// `DSPTAP`: one `QTADOUTF` record per file label.
pub(super) fn tape(system: &mut System, parms: &Parms, v: &TapView) -> Result<String, String> {
    let now = stamp(now_unix());
    let sys = system.system_name();
    let rows: Vec<Row> = v
        .files
        .iter()
        .map(|f| {
            vec![
                ("RDSYSN", sys.clone()),
                ("RDDTTM", now.clone()),
                ("RDRECT", "1".into()),
                ("RDDEVN", v.dev.clone()),
                ("RDCTGI", String::new()),
                ("RDVOLL", v.vol1.clone()),
                ("RDDENS", v.density.clone()),
                ("RDCODE", if v.code == "*ASCII" { "1" } else { "0" }.into()),
                ("RDTRL1", f.trl.0.clone()),
                ("RDTRL2", f.trl.1.clone()),
                ("RDLBID", String::new()),
                ("RDSLT", if v.vol_type == "*SL" { "1" } else { "0" }.into()),
                ("RDLTM", "0".into()),
                ("RDLFSQ", format!("{:010}", f.seq)),
                ("RDLASQ", format!("{:010}", f.volseq)),
                ("RDFILT", String::new()),
            ]
        })
        .collect();
    write(system, parms, "QTADOUTF", QTADOUTF, &rows)
}

/// `DSPTAPCTG`: one `QTAVOUTF` record per cartridge.
pub(super) fn cartridges(
    system: &mut System,
    parms: &Parms,
    all: &[CtgView],
) -> Result<String, String> {
    let now = stamp(now_unix());
    let sys = system.system_name();
    let rows: Vec<Row> = all
        .iter()
        .map(|c| {
            let r = &c.row;
            let sts = match r.status.as_str() {
                "Mounted" => "2",
                "Ejected" => "4",
                "Inserted" => "3",
                _ => "1",
            };
            vec![
                ("RDSYSN", sys.clone()),
                ("RDDTTM", now.clone()),
                ("RDMID", r.id.clone()),
                (
                    "RDVOL",
                    if r.volume.starts_with('*') {
                        String::new()
                    } else {
                        r.volume.clone()
                    },
                ),
                ("RDLIB", c.dev.clone()),
                ("RDCGY", r.category.clone()),
                ("RDSYS", sys.clone()),
                ("RDDEN", String::new()),
                ("RDLOC", r.location.chars().take(10).collect()),
                ("RDLOCI", "0".into()),
                ("RDVSTS", sts.into()),
                ("RDWPR", "0".into()),
                ("RDCODE", "0".into()),
                ("RDMGEN", "0".into()),
                ("RDMIE", "0".into()),
            ]
        })
        .collect();
    write(system, parms, "QTAVOUTF", QTAVOUTF, &rows)
}

/// `DSPOPT`: the volume's `QAMODVA` record, or one `QAMODPA` or
/// `QAMODFA` record per directory or file; `*SAVRST` has no outfile.
pub(super) fn optical(system: &mut System, parms: &Parms, v: &OptView) -> Result<String, String> {
    let now = stamp(now_unix());
    let (cent, date, time) = (
        now[..1].to_string(),
        now[1..7].to_string(),
        now[7..13].to_string(),
    );
    match v {
        OptView::Volume(o) => {
            let a = &o.attrs;
            let crt = a.created.as_ref();
            let chg = a.changed.as_ref();
            let row: Row = vec![
                ("OVDCEN", cent),
                ("OVDDAT", date),
                ("OVDTIM", time),
                ("OVNAME", a.volume_id.clone()),
                ("OVLIBR", o.dev.clone()),
                ("OVCSI", "*NONE".into()),
                ("OVALST", "*NONE".into()),
                ("OVTYPE", "0".into()),
                ("OVMTYP", if a.writable_layout { "4" } else { "2" }.into()),
                ("OVMFMT", "2".into()),
                ("OVVSQN", a.sequence.to_string()),
                (
                    "OVCRDT",
                    crt.map(|x| cyymmdd(&opt_mdy(x))).unwrap_or_default(),
                ),
                (
                    "OVCRTM",
                    crt.map(|x| hhmmss(&opt_hms(x))).unwrap_or_default(),
                ),
                ("OVTEXT", a.text.clone()),
                ("OVBSIZ", a.block_size.to_string()),
                ("OVVSIZ", a.capacity.to_string()),
                ("OVSPCA", a.free.to_string()),
                ("OVVLOC", "2".into()),
                ("OVVACC", if a.write_protected { "1" } else { "3" }.into()),
                (
                    "OVAMDT",
                    chg.map(|x| cyymmdd(&opt_mdy(x))).unwrap_or_default(),
                ),
                (
                    "OVAMTM",
                    chg.map(|x| hhmmss(&opt_hms(x))).unwrap_or_default(),
                ),
            ];
            write(system, parms, "QAMODVA", QAMODVA, &[row])
        }
        OptView::Dirs(dev, vol, dirs) => {
            let rows: Vec<Row> = dirs
                .iter()
                .map(|x| {
                    vec![
                        ("OPDCEN", cent.clone()),
                        ("OPDDAT", date.clone()),
                        ("OPDTIM", time.clone()),
                        ("OPDIR", x.path.clone()),
                        ("OPVID", vol.clone()),
                        ("OPLIB", dev.clone()),
                    ]
                })
                .collect();
            write(system, parms, "QAMODPA", QAMODPA, &rows)
        }
        OptView::Files(dev, vol, files) => {
            let rows: Vec<Row> = files
                .iter()
                .map(|x| {
                    vec![
                        ("OFDCEN", cent.clone()),
                        ("OFDDAT", date.clone()),
                        ("OFDTIM", time.clone()),
                        ("OFFILE", x.path.clone()),
                        ("OFVID", vol.clone()),
                        ("OFLIB", dev.clone()),
                        ("OFSIZE", x.size.min(999_999_999).to_string()),
                        ("OFCNTP", "0".into()),
                        ("OFCNTN", "0".into()),
                        ("OFSVID", vol.clone()),
                        ("OFSIZE2", x.size.to_string()),
                    ]
                })
                .collect();
            write(system, parms, "QAMODFA", QAMODFA, &rows)
        }
        OptView::Saves(..) => {
            Err("CPD0084 OUTPUT(*OUTFILE) is not valid with DATA(*SAVRST).".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The record lengths pub400's models have.
    #[test]
    fn the_formats_are_ibms() {
        let len = |f: &[Fld]| -> usize {
            f.iter()
                .map(|(_, t, l, _)| {
                    if *t == 'P' {
                        usize::from(*l) / 2 + 1
                    } else {
                        usize::from(*l)
                    }
                })
                .sum()
        };
        assert_eq!(QTADOUTF.len(), 16);
        assert_eq!(len(QTADOUTF), 351);
        assert_eq!(QTAVOUTF.len(), 22);
        assert_eq!(len(QTAVOUTF), 135);
        assert_eq!(QAMODVA.len(), 58);
        assert_eq!(QAMODPA.len(), 9);
        assert_eq!(QAMODFA.len(), 20);
        assert_eq!(cyymmdd("10/31/26"), "1261031");
    }
}
