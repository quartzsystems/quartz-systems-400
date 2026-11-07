// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Job accounting (`docs/WORKMGMT.md`, "Job accounting"). `QACGLVL`
//! says what is collected; the entries go to `QSYS/QACGJRN` as journal
//! code `A`: `JB` when a job ends (or changes its accounting code), `SP`
//! when a spooled file prints, `DP` for a printer file that does not
//! spool. Their entry-specific data is the part of IBM's model files
//! `QAJBACG` and `QAPTACG` after the journal heading, so that a `DSPJRN`
//! outfile in the model's format reads field for field. `DSPJRN`'s
//! `*TYPE1` outfile records are made here too.

use dds::{CompileOpts, NoRefs, PfDefinition, RecordFormat, Source, compile_pf};
use qsname::Name;
use store::obj::{ObjectId, types};
use store::txn::Txn;

/// The journal.
pub const JOURNAL: &str = "QACGJRN";
/// The journal code of accounting entries.
pub const CODE: u8 = b'A';
/// The length of the `*TYPE1` journal heading the models start with.
pub const HEADING: usize = 125;

/// The model files and their DDS sources.
pub const FILES: &[(&str, &str)] = &[
    ("QAJBACG", include_str!("../dds/QAJBACG.dds")),
    ("QAPTACG", include_str!("../dds/QAPTACG.dds")),
];

/// `DSPJRN`'s `*TYPE1` outfile (`QADSPJRN`), `JOESD` last at `{ESD}`
/// bytes.
const TYPE1: &str = "     A          R QJORDJE
     A            JOENTL         5S 0
     A            JOSEQN        10S 0
     A            JOCODE         1A
     A            JOENTT         2A
     A            JODATE         6A
     A            JOTIME         6S 0
     A            JOJOB         10A
     A            JOUSER        10A
     A            JONBR          6S 0
     A            JOPGM         10A
     A            JOOBJ         10A
     A            JOLIB         10A
     A            JOMBR         10A
     A            JOCTRR        10S 0
     A            JOFLAG         1A
     A            JOCCID        10S 0
     A            JOINCDAT       1A
     A            JOMINESD       1A
     A            JORES          6A
     A            JOESD     {ESD}A
";

/// What `QACGLVL` asks for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Level {
    /// `*JOB`: `JB` entries.
    pub job: bool,
    /// `*PRINT`: `DP` and `SP` entries.
    pub print: bool,
}

impl Level {
    /// From the value's text (`*NONE`, `*JOB *PRINT` ...).
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let w: Vec<String> = text
            .split_whitespace()
            .map(str::to_ascii_uppercase)
            .collect();
        Self {
            job: w.iter().any(|x| x == "*JOB"),
            print: w.iter().any(|x| x == "*PRINT"),
        }
    }

    /// Nothing collected.
    #[must_use]
    pub fn none(self) -> bool {
        !self.job && !self.print
    }
}

/// Compile DDS text.
fn compile(src: &str) -> Result<PfDefinition, String> {
    let (source, _) = Source::from_text(src);
    let c = compile_pf(&source, &NoRefs, &CompileOpts::default());
    c.definition.ok_or_else(|| c.listing.clone())
}

/// Model file `name` compiled.
pub fn definition(name: &str) -> Result<PfDefinition, String> {
    let src = FILES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, s)| *s)
        .ok_or_else(|| format!("{name} is not an accounting model file"))?;
    compile(src).map_err(|l| format!("{name}: {l}"))
}

/// The `*TYPE1` outfile with `esd` bytes of entry-specific data.
pub fn type1_definition(esd: usize) -> Result<PfDefinition, String> {
    let n = esd.clamp(1, 32_000);
    compile(&TYPE1.replace("{ESD}", &format!("{n:>6}"))).map_err(|l| format!("QADSPJRN: {l}"))
}

/// The model files missing from `QSYS`, created with an empty member;
/// how many.
pub fn install_models(txn: &mut Txn<'_>, qsys: ObjectId, now: u64) -> Result<usize, String> {
    let lib = store::obj::library::Library::load(txn, qsys).map_err(|e| e.to_string())?;
    let mut made = 0;
    for (name, _) in FILES {
        let n = Name::parse(name).map_err(|e| e.to_string())?;
        if lib
            .lookup(txn, types::FILE, n)
            .map_err(|e| e.to_string())?
            .is_some()
        {
            continue;
        }
        let def = definition(name)?;
        db::file::create_pf(
            txn,
            qsys,
            n,
            &def,
            &db::file::CreateFileOpts {
                now,
                owner: "QSYS".into(),
                ..db::file::CreateFileOpts::default()
            },
        )
        .map_err(|e| e.to_string())?;
        made += 1;
    }
    Ok(made)
}

/// One record's values by field name; numbers as decimal text.
pub type Row = Vec<(&'static str, String)>;

/// A record of `fmt` from `row`: character fields blank padded,
/// numbers zero when missing.
pub fn encode(fmt: &RecordFormat, row: &Row) -> Result<Vec<u8>, String> {
    let mut rec = vec![0x40u8; usize::try_from(fmt.record_len).unwrap_or(0)];
    for f in &fmt.fields {
        let name = f.name.to_string();
        let v = row
            .iter()
            .find(|(k, _)| *k == name)
            .map_or("", |(_, v)| v.as_str());
        let val = if f.data_type.is_numeric() {
            let d = qdecimal::Decimal::parse(if v.is_empty() { "0" } else { v })
                .map_err(|e| format!("{name}: {e}"))?;
            db::Value::Num(d)
        } else {
            db::Value::Char(ccsid::to_ebcdic(v).unwrap_or_default())
        };
        db::record::write_field(f, &mut rec, &val).map_err(|e| format!("{name}: {e}"))?;
    }
    Ok(rec)
}

/// The entry-specific data of `row` in model `model`: the record after
/// the journal heading.
pub fn entry_data(model: &str, row: &Row) -> Result<Vec<u8>, String> {
    let def = definition(model)?;
    let rec = encode(&def.format, row)?;
    Ok(rec.get(HEADING..).unwrap_or_default().to_vec())
}

/// Civil date and time of Unix seconds: `(yyyy, mm, dd, hh, mi, ss)`.
#[must_use]
#[allow(clippy::many_single_char_names)] // the calendar's usual names
pub fn civil(secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let m = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    let y = yoe + era * 400 + i64::from(m <= 2);
    let r = u32::try_from(rem).unwrap_or(0);
    (y, m, d, r / 3600, r / 60 % 60, r % 60)
}

/// A six-digit date in a job's date format (`*MDY`, `*DMY`, `*YMD`;
/// `*JUL` as `YYDDD` with a leading zero).
#[must_use]
pub fn short_date(secs: i64, datfmt: &str) -> String {
    let (y, m, d, ..) = civil(secs);
    let yy = y.rem_euclid(100);
    match datfmt {
        "*MDY" => format!("{m:02}{d:02}{yy:02}"),
        "*DMY" => format!("{d:02}{m:02}{yy:02}"),
        "*JUL" => {
            let jan1 = secs - i64::from(days_before(y, m) + d - 1) * 86_400;
            let ddd = (secs - jan1) / 86_400 + 1;
            format!("0{yy:02}{ddd:03}")
        }
        _ => format!("{yy:02}{m:02}{d:02}"),
    }
}

fn days_before(y: i64, m: u32) -> u32 {
    const CUM: [u32; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    CUM[(m as usize).saturating_sub(1).min(11)] + u32::from(leap && m > 2)
}

/// `HHMMSS`.
#[must_use]
pub fn hms(secs: i64) -> String {
    let (.., h, mi, s) = civil(secs);
    format!("{h:02}{mi:02}{s:02}")
}

/// `YYYYMMDDHHMMSS`.
#[must_use]
#[allow(clippy::many_single_char_names)] // date and time parts
pub fn long_stamp(secs: i64) -> String {
    let (y, m, d, h, mi, s) = civil(secs);
    format!("{y:04}{m:02}{d:02}{h:02}{mi:02}{s:02}")
}

/// What a job used, for its `JB` entry. Times are local Unix seconds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JobUse {
    /// Job name.
    pub job: String,
    /// User.
    pub user: String,
    /// Number.
    pub number: u32,
    /// The accounting code the use is charged to.
    pub code: String,
    /// The job's type (`I`, `B`, `A` ...).
    pub kind: char,
    /// The job's date format, for the six-digit dates.
    pub datfmt: String,
    /// When it entered the system.
    pub entered: i64,
    /// When it (or this accounting segment) started.
    pub started: i64,
    /// When the segment ended.
    pub ended: i64,
    /// Processing unit time, milliseconds.
    pub cpu_ms: u64,
    /// Routing steps.
    pub routing_steps: u32,
    /// Interactive transactions and their total time in seconds.
    pub transactions: u64,
    /// Transaction time, seconds.
    pub transaction_secs: u64,
    /// Synchronous auxiliary I/O.
    pub aux_io: u64,
    /// Completion code (0 normal, 10 ...).
    pub completion: u16,
    /// Lines, pages and printer files printed.
    pub lines: u64,
    /// Pages.
    pub pages: u64,
    /// Printer files.
    pub files: u64,
    /// Database writes, reads and updates.
    pub db_puts: u64,
    /// Database reads.
    pub db_gets: u64,
    /// Database updates.
    pub db_updates: u64,
    /// Time suspended, milliseconds.
    pub suspended_ms: u64,
}

/// The `JB` entry-specific data.
pub fn jb_data(u: &JobUse) -> Result<Vec<u8>, String> {
    let n = |v: u64| v.to_string();
    let active_ms = u64::try_from(u.ended.saturating_sub(u.started)).unwrap_or(0) * 1000;
    let row: Row = vec![
        ("JAJOB", u.job.clone()),
        ("JAUSER", u.user.clone()),
        ("JANBR", n(u64::from(u.number))),
        ("JACDE", u.code.clone()),
        ("JACPU", n(u.cpu_ms)),
        ("JARTGS", n(u64::from(u.routing_steps))),
        ("JAEDTE", short_date(u.entered, &u.datfmt)),
        ("JAETIM", hms(u.entered)),
        ("JASDTE", short_date(u.started, &u.datfmt)),
        ("JASTIM", hms(u.started)),
        ("JATRNT", n(u.transaction_secs)),
        ("JATRNS", n(u.transactions)),
        ("JAAUX", n(u.aux_io)),
        ("JATYPE", u.kind.to_string()),
        ("JACCDE", n(u64::from(u.completion))),
        ("JALINE", n(u.lines)),
        ("JAPAGE", n(u.pages)),
        ("JAPRTF", n(u.files)),
        ("JADBPT", n(u.db_puts)),
        ("JADBGT", n(u.db_gets)),
        ("JADBUP", n(u.db_updates)),
        ("JACMPT", n(0)),
        ("JACMGT", n(0)),
        ("JAACT", n(active_ms)),
        ("JASPN", n(u.suspended_ms)),
        ("JAEDTL", long_stamp(u.entered)),
        ("JASDTL", long_stamp(u.started)),
        ("JAAIO", n(0)),
        ("JAXCPU", n(u.cpu_ms)),
        ("JAXSIO", n(u.aux_io)),
        ("JAXAIO", n(0)),
        ("JAXDBP", n(u.db_puts)),
        ("JAXDBG", n(u.db_gets)),
        ("JAXDBU", n(u.db_updates)),
        ("JAXLIN", n(u.lines)),
        ("JAXPAG", n(u.pages)),
        ("JAXPRT", n(u.files)),
    ];
    entry_data("QAJBACG", &capped(row))
}

/// The packed fields of the short forms hold eleven digits: a larger
/// count shows as the largest (the `JAX` fields carry the whole).
fn capped(mut row: Row) -> Row {
    const SHORT: &[&str] = &[
        "JACPU", "JATRNT", "JATRNS", "JAAUX", "JALINE", "JAPAGE", "JAPRTF", "JADBPT", "JADBGT",
        "JADBUP", "JAACT", "JASPN",
    ];
    for (k, v) in &mut row {
        if SHORT.contains(k) && v.len() > 11 {
            *v = "99999999999".into();
        }
    }
    row
}

/// A printed file, for its `SP` or `DP` entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Printed {
    /// The job that made it.
    pub job: String,
    /// Its user.
    pub user: String,
    /// Its number.
    pub number: u32,
    /// The accounting code.
    pub code: String,
    /// Printer file and library.
    pub file: (String, String),
    /// Device, its type and model.
    pub device: (String, String, String),
    /// Pages and lines.
    pub pages: u64,
    /// Lines.
    pub lines: u64,
    /// Spooled file name and number (`SP` only).
    pub splf: Option<(String, u32)>,
    /// Output priority.
    pub outpty: u8,
    /// Form type.
    pub formtype: String,
    /// Bytes.
    pub bytes: u64,
    /// User data.
    pub usrdta: String,
}

/// The `SP` or `DP` entry-specific data.
pub fn print_data(p: &Printed) -> Result<Vec<u8>, String> {
    let n = |v: u64| v.to_string();
    let (splf, splnbr) = p
        .splf
        .as_ref()
        .map_or((String::new(), String::new()), |(f, nbr)| {
            (f.clone(), format!("{:04}", nbr % 10_000))
        });
    let row: Row = vec![
        ("JAJOB", p.job.clone()),
        ("JAUSER", p.user.clone()),
        ("JANBR", n(u64::from(p.number))),
        ("JACDE", p.code.clone()),
        ("JADFN", p.file.0.clone()),
        ("JADFNL", p.file.1.clone()),
        ("JADEVN", p.device.0.clone()),
        ("JADEVT", p.device.1.clone()),
        ("JADEVM", p.device.2.clone()),
        ("JATPAG", n(p.pages)),
        ("JATLIN", n(p.lines)),
        ("JASPFN", splf),
        ("JASPNB", splnbr),
        ("JAOPTY", p.outpty.to_string()),
        ("JAFMTP", p.formtype.clone()),
        ("JABYTE", n(p.bytes)),
        ("JAUSRD", p.usrdta.clone()),
    ];
    entry_data("QAPTACG", &capped(row))
}

/// `QSYS/QACGJRN`, if it exists.
pub fn journal(txn: &mut Txn<'_>, qsys: ObjectId) -> Result<Option<ObjectId>, String> {
    let lib = store::obj::library::Library::load(txn, qsys).map_err(|e| e.to_string())?;
    let name = Name::parse(JOURNAL).map_err(|e| e.to_string())?;
    lib.lookup(txn, jrn::JRN, name).map_err(|e| e.to_string())
}

/// Write an accounting entry of `kind` (`JB`, `DP`, `SP`) from `job`.
/// `false` when there is no journal to write to.
pub fn write(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    kind: &str,
    data: Vec<u8>,
    job: jrn::JobInfo,
) -> Result<bool, String> {
    let Some(id) = journal(txn, qsys)? else {
        return Ok(false);
    };
    let k = kind.as_bytes();
    let mut j = jrn::Journal::load(txn, id).map_err(|e| e.to_string())?;
    j.append(
        txn,
        jrn::Draft {
            code: CODE,
            kind: [k[0], k[1]],
            flags: 0,
            job,
            program: String::new(),
            object: None,
            count: 0,
            commit_cycle: 0,
            data,
            time: None,
        },
    )
    .map_err(|e| e.to_string())?;
    Ok(true)
}

/// A `*TYPE1` outfile record of `e`: the 125-byte heading, then the
/// entry-specific data cut or blank padded to `esd` bytes. `local` is
/// the offset of local time from UTC in seconds; `datfmt` the job's
/// date format.
pub fn type1_record(
    fmt: &RecordFormat,
    e: &jrn::Entry,
    local: i64,
    datfmt: &str,
) -> Result<Vec<u8>, String> {
    let at = i64::try_from(e.time).unwrap_or(0) + local;
    let (obj, lib, mbr) = e.object.as_ref().map_or_else(
        || (String::new(), String::new(), String::new()),
        |o| {
            (
                o.object.to_string(),
                o.library.to_string(),
                o.member.map(|m| m.to_string()).unwrap_or_default(),
            )
        },
    );
    let row: Row = vec![
        ("JOENTL", (HEADING + e.data.len()).min(99_999).to_string()),
        ("JOSEQN", (e.seq % 10_000_000_000).to_string()),
        ("JOCODE", char::from(e.code).to_string()),
        ("JOENTT", String::from_utf8_lossy(&e.kind).into_owned()),
        ("JODATE", short_date(at, datfmt)),
        ("JOTIME", hms(at)),
        ("JOJOB", e.job.name.clone()),
        ("JOUSER", e.job.user.clone()),
        ("JONBR", e.job.number.to_string()),
        ("JOPGM", e.program.clone()),
        ("JOOBJ", obj),
        ("JOLIB", lib),
        ("JOMBR", mbr),
        ("JOCTRR", (e.count % 10_000_000_000).to_string()),
        ("JOFLAG", "0".into()),
        ("JOCCID", (e.commit_cycle % 10_000_000_000).to_string()),
        ("JOINCDAT", "0".into()),
        ("JOMINESD", "0".into()),
    ];
    let mut rec = encode(fmt, &row)?;
    let room = rec.len().saturating_sub(HEADING);
    let take = e.data.len().min(room);
    if take > 0 {
        rec[HEADING..HEADING + take].copy_from_slice(&e.data[..take]);
    }
    Ok(rec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn models_compile_with_ibm_lengths() {
        assert_eq!(definition("QAJBACG").unwrap().format.record_len, 449);
        assert_eq!(definition("QAPTACG").unwrap().format.record_len, 259);
        assert_eq!(type1_definition(100).unwrap().format.record_len, 225);
    }

    #[test]
    fn jb_data_lands_on_the_model_fields() {
        let u = JobUse {
            job: "ACGTEST".into(),
            user: "QSECOFR".into(),
            number: 123,
            code: "DEPT42".into(),
            kind: 'B',
            datfmt: "*YMD".into(),
            entered: 1_793_339_970,
            started: 1_793_339_975,
            ended: 1_793_339_985,
            cpu_ms: 1_234,
            routing_steps: 1,
            ..JobUse::default()
        };
        let d = jb_data(&u).unwrap();
        assert_eq!(d.len(), 449 - HEADING);
        let def = definition("QAJBACG").unwrap();
        let mut rec = vec![0x40u8; HEADING];
        rec.extend(&d);
        let field = |n: &str| {
            let f = def
                .format
                .fields
                .iter()
                .find(|f| f.name.to_string() == n)
                .unwrap();
            db::record::read_field(f, &rec, &[], None).unwrap()
        };
        let num = |n: &str| match field(n) {
            db::Value::Num(d) => d.to_string(),
            v => panic!("{v:?}"),
        };
        assert_eq!(num("JANBR"), "123");
        assert_eq!(num("JACPU"), "1234");
        match field("JACDE") {
            db::Value::Char(b) => assert_eq!(ccsid::from_ebcdic(&b).trim_end(), "DEPT42"),
            v => panic!("{v:?}"),
        }
        match field("JASDTL") {
            db::Value::Char(b) => assert_eq!(ccsid::from_ebcdic(&b), "20261030055935"),
            v => panic!("{v:?}"),
        }
    }

    #[test]
    fn levels_and_dates() {
        assert_eq!(
            Level::parse("*JOB *PRINT"),
            Level {
                job: true,
                print: true
            }
        );
        assert!(Level::parse("*NONE").none());
        assert_eq!(short_date(1_793_339_970, "*MDY"), "103026");
        assert_eq!(short_date(1_793_339_970, "*JUL"), "026303");
    }
}
