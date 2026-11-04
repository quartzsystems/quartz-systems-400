// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Collection Services database files: the model files shipped in
//! `QSYS` (their DDS under `work/perf/dds/`, field for field IBM's
//! V7R1M0 layout) and the records `CRTPFRDTA` writes into them from a
//! collection's intervals. Fields this release has no figure for are
//! zero or blank.

use dds::{CompileOpts, NoRefs, PfDefinition, RecordFormat, Source, compile_pf};

use crate::mgtcol::{Collection, civil};
use crate::sample::{Interval, JobData};

/// Every model file and its DDS source.
pub const FILES: &[(&str, &str)] = &[
    ("QAPMCONF", include_str!("../dds/QAPMCONF.dds")),
    ("QAPMDISK", include_str!("../dds/QAPMDISK.dds")),
    ("QAPMETH", include_str!("../dds/QAPMETH.dds")),
    ("QAPMJOBMI", include_str!("../dds/QAPMJOBMI.dds")),
    ("QAPMJOBOS", include_str!("../dds/QAPMJOBOS.dds")),
    ("QAPMJSUM", include_str!("../dds/QAPMJSUM.dds")),
    ("QAPMPOOL", include_str!("../dds/QAPMPOOL.dds")),
    ("QAPMSYSTEM", include_str!("../dds/QAPMSYSTEM.dds")),
];

/// The DDS source of model file `name`.
#[must_use]
pub fn source(name: &str) -> Option<&'static str> {
    FILES.iter().find(|(n, _)| *n == name).map(|(_, s)| *s)
}

/// Model file `name` compiled.
pub fn definition(name: &str) -> Result<PfDefinition, String> {
    let src = source(name).ok_or_else(|| format!("{name} is not a performance file"))?;
    let (source, _) = Source::from_text(src);
    let c = compile_pf(&source, &NoRefs, &CompileOpts::default());
    c.definition.ok_or_else(|| format!("{name}: {}", c.listing))
}

/// Create model file `name` in `library`: no member, any number of
/// them (`CRTPFRDTA` adds one per collection).
pub fn create(
    txn: &mut store::txn::Txn<'_>,
    library: store::obj::ObjectId,
    name: &str,
    now: u64,
) -> Result<store::obj::ObjectId, String> {
    let def = definition(name)?;
    let n = qsname::Name::parse(name).map_err(|e| e.to_string())?;
    db::file::create_pf(
        txn,
        library,
        n,
        &def,
        &db::file::CreateFileOpts {
            now,
            member: db::file::MemberOpt::None,
            max_members: 0,
            ..db::file::CreateFileOpts::default()
        },
    )
    .map_err(|e| e.to_string())
}

/// The model files missing from `library` (`QSYS`), created; how many.
pub fn install_models(
    txn: &mut store::txn::Txn<'_>,
    library: store::obj::ObjectId,
    now: u64,
) -> Result<usize, String> {
    let lib = store::obj::library::Library::load(txn, library).map_err(|e| e.to_string())?;
    let mut made = 0;
    for (name, _) in FILES {
        let n = qsname::Name::parse(name).map_err(|e| e.to_string())?;
        if lib
            .lookup(txn, store::obj::types::FILE, n)
            .map_err(|e| e.to_string())?
            .is_none()
        {
            create(txn, library, name, now)?;
            made += 1;
        }
    }
    Ok(made)
}

/// One record: field names and values (numbers as decimal text).
pub type Row = Vec<(&'static str, String)>;

#[allow(clippy::needless_pass_by_value)] // numbers and strings alike
fn pair(k: &'static str, v: impl ToString) -> (&'static str, String) {
    (k, v.to_string())
}

/// `YYMMDDHHMMSS` and the century digit of a Unix time.
#[must_use]
pub fn dtetim(secs: u64) -> (String, String) {
    let (y, m, d) = civil(secs / 86_400);
    let s = secs % 86_400;
    (
        format!(
            "{:02}{m:02}{d:02}{:02}{:02}{:02}",
            y % 100,
            s / 3600,
            s / 60 % 60,
            s % 60
        ),
        if y >= 2000 { "1" } else { "0" }.to_string(),
    )
}

/// Milliseconds as seconds with three decimals.
fn secs3(ms: u64) -> String {
    format!("{}.{:03}", ms / 1000, ms % 1000)
}

/// The fields every interval record starts with.
fn head(i: &Interval, century: bool) -> Row {
    let (t, c) = dtetim(i.end);
    let mut r = vec![pair("INTNUM", i.number), pair("DTETIM", t), pair("INTSEC", i.secs)];
    if century {
        r.push(pair("DTECEN", c));
    }
    r
}

fn job_head(r: &mut Row, j: &JobData) {
    r.extend([
        pair("JBNAME", &j.name),
        pair("JBUSER", &j.user),
        pair("JBNBR", format!("{:06}", j.number)),
        pair("JBTYPE", &j.kind),
        pair("JBSTYP", &j.sub_type),
    ]);
}

/// `QAPMSYSTEM`: one record per interval.
fn system(i: &Interval) -> Row {
    let s = &i.system;
    let mut r = head(i, true);
    r.extend([
        pair("SYSASP", s.asp_bytes),
        pair("SYNUAL", s.asp_bytes.saturating_sub(s.asp_free)),
        pair("SYFOPN", s.opens),
        pair("SYSPTU", s.cpu_used_ms),
        pair("SYSCTA", s.cpu_avail_ms),
        pair("SYSUTA", s.cpu_avail_ms),
        pair("SYVCPU", s.cpus),
        pair("SYSPLU", s.cpu_used_ms),
        pair("SYSPLA", s.cpu_avail_ms),
        pair("SYDPGF", i.pools.iter().map(|p| p.db_faults).sum::<u64>()),
        pair("SYAPGF", i.pools.iter().map(|p| p.ndb_faults).sum::<u64>()),
    ]);
    r
}

/// `QAPMJOBMI`: one record per job.
fn jobmi(i: &Interval, j: &JobData) -> Row {
    let mut r = head(i, true);
    job_head(&mut r, j);
    r.extend([
        pair("JBPOOL", format!("{:02}", j.pool)),
        pair("JBPRTY", format!("{:03}", j.priority)),
        pair("JBCPU", secs3(j.cpu_ms)),
        pair("JBTCPU", secs3(j.cpu_ms)),
        pair("JBRSP", secs3(j.response_ms)),
        pair("JBNTR", j.transactions),
        pair("JBDBR", j.db_reads),
        pair("JBNDB", j.other_io),
        pair("JBDBW", j.db_writes),
        pair("JBWRT", j.db_writes),
        pair("JBAW", j.aw),
        pair("JBWI", j.wi),
        pair("JBAI", j.ai),
        pair("JBTHAC", j.threads),
        pair("JBTHCT", j.threads),
        pair("JBTHDF", 0),
        pair("JBCUSR", &j.user),
    ]);
    r
}

/// `QAPMJOBOS`: one record per job.
fn jobos(i: &Interval, j: &JobData) -> Row {
    let mut r = head(i, true);
    job_head(&mut r, j);
    r.extend([
        pair("JBSSYS", &j.subsystem),
        pair("JBSLIB", &j.sbs_lib),
        pair("JBRSP", secs3(j.response_ms)),
        pair("JBNTR", j.transactions),
        pair("JBLRD", j.db_reads),
        pair("JBLWT", j.db_writes),
        pair("JBDBU", j.db_reads + j.db_writes),
        pair("JBFSOPN", j.opens),
    ]);
    r
}

/// `QAPMJSUM`'s group of a job: interactive, batch or system.
fn group(j: &JobData) -> &'static str {
    match j.kind.as_str() {
        "I" => "INT",
        "B" => "BCH",
        _ => "SYS",
    }
}

/// `QAPMJSUM`: one record per job group per interval.
fn jsum(i: &Interval) -> Vec<Row> {
    ["BCH", "INT", "SYS"]
        .iter()
        .filter_map(|g| {
            let jobs: Vec<&JobData> = i.jobs.iter().filter(|j| group(j) == *g).collect();
            if jobs.is_empty() {
                return None;
            }
            let sum = |f: fn(&JobData) -> u64| jobs.iter().map(|j| f(j)).sum::<u64>();
            let mut r = head(i, true);
            r.extend([
                pair("JSCBKT", *g),
                pair("JSCPU", sum(|j| j.cpu_ms)),
                pair("JSTRNT", secs3(sum(|j| j.response_ms))),
                pair("JSTRNS", sum(|j| j.transactions)),
                pair("JSJBCT", jobs.len()),
                pair("JSPDBR", sum(|j| j.db_reads)),
                pair("JSPNDB", sum(|j| j.other_io)),
                pair("JSPWRT", sum(|j| j.db_writes)),
                pair("JSLDBR", sum(|j| j.db_reads)),
                pair("JSLDBW", sum(|j| j.db_writes)),
            ]);
            Some(r)
        })
        .collect()
}

/// `QAPMPOOL`: one record per pool.
fn pools(i: &Interval) -> Vec<Row> {
    i.pools
        .iter()
        .map(|p| {
            let mut r = head(i, false);
            r.extend([
                pair("PONBR", format!("{:02}", p.number)),
                pair("POACTL", p.actlvl),
                pair("POSIZ", p.size_kb),
                pair("PORES", p.reserved_kb),
                pair("PODBF", p.db_faults),
                pair("PONDBF", p.ndb_faults),
                pair("PODBPG", p.db_pages),
                pair("PONDPG", p.ndb_pages),
                pair("POAW", p.aw),
                pair("POWI", p.wi),
                pair("POAI", p.ai),
                pair("PTTYPE", "F"),
                pair("PTPAGE", "F"),
            ]);
            r
        })
        .collect()
}

/// `QAPMDISK`: one record per unit.
fn disks(i: &Interval) -> Vec<Row> {
    // Samples of the busy state: one per 10 ms of the interval.
    let samples = u64::from(i.secs) * 100;
    i.disks
        .iter()
        .map(|d| {
            let mut r = head(i, false);
            r.extend([
                pair("IOPRN", "CMB01"),
                pair("DSARM", format!("{:04}", d.unit)),
                pair("DSTYPE", &d.type_code),
                pair("DSDRN", &d.resource),
                pair("DSMDLN", &d.model),
                pair("DSSRLN", &d.serial),
                pair("DSBLKR", d.read_blocks),
                pair("DSBLKW", d.write_blocks),
                pair("DSRDS", d.reads),
                pair("DSWRTS", d.writes),
                pair("DSSMPL", samples),
                pair("DSNBSY", (d.busy_ms / 10).min(samples)),
                pair("DSCAP", d.capacity),
                pair("DSAVL", d.available),
                pair("DSASP", d.asp),
                pair("DSASPN", "*SYSBAS"),
            ]);
            r
        })
        .collect()
}

/// `QAPMETH`: one record per Ethernet line.
fn lines(i: &Interval) -> Vec<Row> {
    i.lines
        .iter()
        .map(|l| {
            let mut r = head(i, false);
            r.extend([
                pair("IOPRN", &l.resource),
                pair("ETTYPE", "ETH"),
                pair("ETLLND", &l.line),
                pair("ETLLSP", l.speed),
                pair("ETLTFT", l.tx_bytes),
                pair("ETLTFR", l.rx_bytes),
                pair("ETMFTG", l.tx_frames),
                pair("ETMFRG", l.rx_frames),
                pair("ETMDCN", l.rx_errors + l.tx_errors),
            ]);
            r
        })
        .collect()
}

/// `QAPMCONF`: the collection's configuration records, a key and a
/// value of at most ten bytes each.
fn conf(c: &Collection) -> Vec<Row> {
    let h = &c.header;
    let (start, cen) = dtetim(h.started);
    let mut entries = vec![
        ("VR", h.release.clone()),
        ("SN", h.system.clone()),
        ("I ", format!("{}", h.interval)),
        ("ST", format!("{cen}{}", &start[..6])),
        ("SM", start[6..].to_string()),
        ("CP", h.profile.clone()),
    ];
    if let Some(first) = c.intervals.first() {
        entries.push(("PN", first.system.cpus.to_string()));
        entries.push(("MS", (first.system.main_kb / 1024).to_string()));
    }
    entries
        .into_iter()
        .map(|(k, v)| {
            vec![
                pair("GRES", ""),
                pair("GKEY", k),
                pair("GDES", v.chars().take(10).collect::<String>()),
            ]
        })
        .collect()
}

/// The records of model file `name` from collection `c`.
#[must_use]
pub fn rows(name: &str, c: &Collection) -> Vec<Row> {
    let per = |f: &dyn Fn(&Interval) -> Vec<Row>| c.intervals.iter().flat_map(f).collect();
    match name {
        "QAPMCONF" => conf(c),
        "QAPMSYSTEM" => per(&|i| vec![system(i)]),
        "QAPMJOBMI" => per(&|i| i.jobs.iter().map(|j| jobmi(i, j)).collect()),
        "QAPMJOBOS" => per(&|i| i.jobs.iter().map(|j| jobos(i, j)).collect()),
        "QAPMJSUM" => per(&jsum),
        "QAPMPOOL" => per(&pools),
        "QAPMDISK" => per(&disks),
        "QAPMETH" => per(&lines),
        _ => Vec::new(),
    }
}

/// One record of `row` in format `fmt`: character fields padded with
/// blanks, hex fields with the CP037 bytes of the text, numbers zero
/// when missing.
pub fn encode(fmt: &RecordFormat, row: &Row) -> Result<Vec<u8>, String> {
    let mut rec = vec![0u8; usize::try_from(fmt.record_len).unwrap_or(0)];
    for f in &fmt.fields {
        let fname = f.name.to_string();
        let v = row
            .iter()
            .find(|(k, _)| *k == fname)
            .map_or("", |(_, v)| v.as_str());
        let val = if f.data_type.is_numeric() {
            let d = qdecimal::Decimal::parse(if v.is_empty() { "0" } else { v })
                .map_err(|e| format!("{fname}: {e}"))?;
            db::Value::Num(d)
        } else {
            db::Value::Char(ccsid::to_ebcdic(v).unwrap_or_default())
        };
        db::record::write_field(f, &mut rec, &val).map_err(|e| format!("{fname}: {e}"))?;
    }
    Ok(rec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mgtcol::Header;
    use crate::sample::{PoolData, SysData};

    #[test]
    fn every_model_compiles_and_rows_encode() {
        let c = Collection {
            header: Header {
                started: 1_793_339_970,
                interval: 15,
                profile: "*STANDARD".into(),
                system: "QS400".into(),
                release: "V1R1M0".into(),
                ..Header::default()
            },
            ended: 0,
            intervals: vec![Interval {
                number: 1,
                end: 1_793_339_985,
                secs: 15,
                system: SysData {
                    cpu_used_ms: 1_500,
                    cpu_avail_ms: 30_000,
                    cpus: 2,
                    ..SysData::default()
                },
                jobs: vec![JobData {
                    name: "QPADEV0001".into(),
                    user: "QSECOFR".into(),
                    number: 42,
                    kind: "I".into(),
                    cpu_ms: 1_234,
                    ..JobData::default()
                }],
                pools: vec![PoolData {
                    number: 2,
                    name: "*BASE".into(),
                    size_kb: 1024,
                    ..PoolData::default()
                }],
                ..Interval::default()
            }],
        };
        for (name, _) in FILES {
            let def = definition(name).unwrap();
            for row in rows(name, &c) {
                let rec = encode(&def.format, &row).unwrap();
                assert_eq!(rec.len(), def.format.record_len as usize, "{name}");
            }
        }
        let job = &rows("QAPMJOBMI", &c)[0];
        assert!(job.contains(&("JBCPU", "1.234".to_string())));
        assert!(job.contains(&("JBNBR", "000042".to_string())));
        assert_eq!(dtetim(1_793_339_970), ("261030055930".to_string(), "1".to_string()));
        assert_eq!(rows("QAPMJSUM", &c).len(), 1);
        assert_eq!(rows("QAPMCONF", &c).len(), 8);
    }
}
