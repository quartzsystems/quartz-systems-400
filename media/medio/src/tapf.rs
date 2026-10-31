// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Tape files (`*FILE TAPF`, `CRTTAPF`) and the data side of `CPYTOTAP`
//! and `CPYFRMTAP`: the file's attributes as the object keeps them, the
//! command values over them (`*TAPF` takes the file's), and records to
//! and from a labeled or unlabeled tape file.

use std::fmt::Write as _;

use tape::label::{Date, Expiry, FileLabel, RecordFormat};
use tape::volume::{FileReader, FileWriter, ReadOptions, WriteOptions, WriteSummary, blocks_to_records, records_to_blocks};
use tape::{EndOpt, MAX_BLOCK, SeqNbr, TapeTransport, VolumeProvider};

use crate::{MediaError, Result};

const MAGIC: &str = "QS400-TAPF 1";

/// The largest block `*CALC` makes for blocked records.
pub const CALC_BLOCK: u32 = 32_760;

/// A tape file's attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TapeFileDef {
    /// `DEV`: the devices, first used first.
    pub devs: Vec<String>,
    /// `VOL`: the volumes, empty for `*NONE` (the one mounted).
    pub vols: Vec<String>,
    /// `REELS(*NL)`.
    pub unlabeled: bool,
    /// `SEQNBR`; `None` for `*END`.
    pub seq: Option<u32>,
    /// `LABEL`; empty for `*NONE`.
    pub label: String,
    /// `RCDLEN`; 0 for `*CALC`.
    pub rcdlen: u32,
    /// `BLKLEN`; 0 for `*CALC`.
    pub blklen: u32,
    /// `RCDBLKTYP`: `*F`, `*FB`, `*V`, `*VB`, `*U`.
    pub rcdblk: String,
    /// `CODE(*ASCII)`.
    pub ascii: bool,
    /// `EXPDATE`.
    pub expires: Expiry,
    /// `ENDOPT`.
    pub end_opt: EndOpt,
    /// `TEXT`.
    pub text: String,
}

impl Default for TapeFileDef {
    fn default() -> Self {
        Self {
            devs: Vec::new(),
            vols: Vec::new(),
            unlabeled: false,
            seq: Some(1),
            label: String::new(),
            rcdlen: 0,
            blklen: 0,
            rcdblk: "*FB".into(),
            ascii: false,
            expires: Expiry::None,
            end_opt: EndOpt::Rewind,
            text: String::new(),
        }
    }
}

fn expiry_text(e: Expiry) -> String {
    match e {
        Expiry::None => "*NONE".into(),
        Expiry::Perm => "*PERM".into(),
        Expiry::Date(d) => format!("{}.{:03}", d.year, d.day),
    }
}

fn parse_expiry(s: &str) -> Expiry {
    match s {
        "*PERM" => Expiry::Perm,
        _ => s
            .split_once('.')
            .and_then(|(y, d)| Some(Date { year: y.parse().ok()?, day: d.parse().ok()? }))
            .map_or(Expiry::None, Expiry::Date),
    }
}

/// `ENDOPT` as text.
#[must_use]
pub fn end_opt_name(e: EndOpt) -> &'static str {
    match e {
        EndOpt::Rewind => "*REWIND",
        EndOpt::Leave => "*LEAVE",
        EndOpt::Unload => "*UNLOAD",
    }
}

/// `ENDOPT` from text; `*REWIND` for anything else.
#[must_use]
pub fn parse_end_opt(s: &str) -> EndOpt {
    match s {
        "*LEAVE" => EndOpt::Leave,
        "*UNLOAD" => EndOpt::Unload,
        _ => EndOpt::Rewind,
    }
}

impl TapeFileDef {
    /// The object's payload: the magic line, then `key<TAB>value`.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut s = format!("{MAGIC}\n");
        let _ = writeln!(s, "dev\t{}", self.devs.join(" "));
        let _ = writeln!(s, "vol\t{}", self.vols.join(" "));
        let _ = writeln!(s, "reels\t{}", if self.unlabeled { "*NL" } else { "*SL" });
        let _ = writeln!(s, "seqnbr\t{}", self.seq.map_or_else(|| "*END".to_string(), |n| n.to_string()));
        let _ = writeln!(s, "label\t{}", self.label);
        let _ = writeln!(s, "rcdlen\t{}", self.rcdlen);
        let _ = writeln!(s, "blklen\t{}", self.blklen);
        let _ = writeln!(s, "rcdblktyp\t{}", self.rcdblk);
        let _ = writeln!(s, "code\t{}", if self.ascii { "*ASCII" } else { "*EBCDIC" });
        let _ = writeln!(s, "expdate\t{}", expiry_text(self.expires));
        let _ = writeln!(s, "endopt\t{}", end_opt_name(self.end_opt));
        let _ = writeln!(s, "text\t{}", self.text.replace(['\n', '\t'], " "));
        s.into_bytes()
    }

    /// Read a payload back.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let text = String::from_utf8_lossy(b);
        let mut lines = text.lines();
        if lines.next() != Some(MAGIC) {
            return Err(MediaError::State("not a tape file".into()));
        }
        let mut d = Self::default();
        let words = |v: &str| v.split_whitespace().map(str::to_string).collect::<Vec<_>>();
        for l in lines {
            let Some((k, v)) = l.split_once('\t') else { continue };
            match k {
                "dev" => d.devs = words(v),
                "vol" => d.vols = words(v),
                "reels" => d.unlabeled = v == "*NL",
                "seqnbr" => d.seq = v.parse().ok(),
                "label" => d.label = v.to_string(),
                "rcdlen" => d.rcdlen = v.parse().unwrap_or(0),
                "blklen" => d.blklen = v.parse().unwrap_or(0),
                "rcdblktyp" => d.rcdblk = v.to_string(),
                "code" => d.ascii = v == "*ASCII",
                "expdate" => d.expires = parse_expiry(v),
                "endopt" => d.end_opt = parse_end_opt(v),
                "text" => d.text = v.to_string(),
                _ => {}
            }
        }
        Ok(d)
    }

    /// The `DSPFD` lines.
    #[must_use]
    pub fn describe(&self) -> String {
        let none = |v: &[String]| if v.is_empty() { "*NONE".to_string() } else { v.join(" ") };
        let calc = |n: u32| if n == 0 { "*CALC".to_string() } else { n.to_string() };
        let rows = [
            ("File attribute", "TAPF".to_string()),
            ("Tape device", none(&self.devs)),
            ("Volume identifier", none(&self.vols)),
            ("Tape reels", if self.unlabeled { "*NL" } else { "*SL" }.to_string()),
            ("Sequence number", self.seq.map_or_else(|| "*END".to_string(), |n| n.to_string())),
            ("Tape label", if self.label.is_empty() { "*NONE".to_string() } else { self.label.clone() }),
            ("Record length", calc(self.rcdlen)),
            ("Block length", calc(self.blklen)),
            ("Record block type", self.rcdblk.clone()),
            ("Code", if self.ascii { "*ASCII" } else { "*EBCDIC" }.to_string()),
            ("File expiration date", expiry_text(self.expires)),
            ("End of tape option", end_opt_name(self.end_opt).to_string()),
            ("Text", self.text.clone()),
        ];
        let mut s = String::new();
        for (k, v) in rows {
            let _ = writeln!(s, "{k:.<30} : {v}");
        }
        s
    }
}

/// A record block type's format and whether records are blocked.
#[must_use]
pub fn record_format(rcdblk: &str) -> (RecordFormat, bool) {
    match rcdblk {
        "*F" => (RecordFormat::Fixed, false),
        "*V" => (RecordFormat::Variable, false),
        "*VB" => (RecordFormat::Variable, true),
        "*U" => (RecordFormat::Undefined, false),
        _ => (RecordFormat::Fixed, true),
    }
}

/// The record length `RCDLEN(*CALC)` means for records up to `longest`,
/// and the block length `BLKLEN(*CALC)` means for it.
#[must_use]
pub fn lengths(format: RecordFormat, blocked: bool, rcdlen: u32, blklen: u32, longest: u32) -> (u32, u32) {
    let rcdlen = match (rcdlen, format) {
        (0, RecordFormat::Variable) => longest + 4,
        (0, _) => longest.max(1),
        (n, _) => n,
    };
    let blklen = match (blklen, format, blocked) {
        (0, RecordFormat::Fixed, true) => (CALC_BLOCK / rcdlen).max(1) * rcdlen,
        (0, RecordFormat::Fixed, false) | (0, RecordFormat::Undefined, _) => rcdlen,
        (0, RecordFormat::Variable, true) => CALC_BLOCK,
        (0, RecordFormat::Variable, false) => rcdlen + 4,
        (n, _, _) => n,
    };
    (rcdlen, blklen)
}

/// The records of a `CPYTOTAP`, ready for the tape.
#[derive(Debug, Clone)]
pub struct CopyOut {
    /// `TOVOL`.
    pub vols: Vec<String>,
    /// `TOSEQNBR`.
    pub seq: SeqNbr,
    /// The labels: file identifier, dates, record format and lengths.
    pub label: FileLabel,
    /// `*NL`.
    pub unlabeled: bool,
    /// Today, for the active-file check.
    pub today: Option<Date>,
    /// Active files may be overwritten.
    pub allow_active: bool,
    /// `TOENDOPT`.
    pub end_opt: EndOpt,
    /// The records, already in the tape's code.
    pub records: Vec<Vec<u8>>,
    /// The pad byte of short fixed records.
    pub pad: u8,
}

/// Write `c`'s records as one tape file.
pub fn copy_to_tape<D: TapeTransport + ?Sized>(
    drive: &mut D,
    provider: &mut dyn VolumeProvider<D>,
    c: &CopyOut,
) -> Result<WriteSummary> {
    let blocks = records_to_blocks(
        c.records.iter().map(Vec::as_slice),
        c.label.record_format,
        c.label.record_length as usize,
        c.label.block_length as usize,
        c.pad,
    )?;
    let opts = WriteOptions {
        vols: c.vols.clone(),
        seq: c.seq,
        label: c.label.clone(),
        unlabeled: c.unlabeled,
        today: c.today,
        allow_active: c.allow_active,
        end_opt: c.end_opt,
    };
    let mut w = FileWriter::open(drive, provider, opts)?;
    for b in &blocks {
        w.write_block(b)?;
    }
    Ok(w.finish()?)
}

/// What a `CPYFRMTAP` reads.
#[derive(Debug, Clone, Default)]
pub struct CopyIn {
    /// `FROMVOL`.
    pub vols: Vec<String>,
    /// `FROMSEQNBR`.
    pub seq: Option<u32>,
    /// `FROMLABEL`.
    pub label: Option<String>,
    /// `*NL`.
    pub unlabeled: bool,
    /// `FROMENDOPT`.
    pub end_opt: EndOpt,
    /// For an unlabeled file, or over the labels: the format, the record
    /// length (0 for the labels' or the block's).
    pub format: Option<RecordFormat>,
    /// The record length; 0 for the labels'.
    pub rcdlen: u32,
}

/// Read a tape file's records; its labels (the file identifier empty
/// for an unlabeled file).
pub fn copy_from_tape<D: TapeTransport + ?Sized>(
    drive: &mut D,
    provider: &mut dyn VolumeProvider<D>,
    c: &CopyIn,
) -> Result<(Vec<Vec<u8>>, FileLabel)> {
    let opts = ReadOptions {
        vols: c.vols.clone(),
        seq: c.seq,
        file_label: c.label.clone(),
        unlabeled: c.unlabeled,
        end_opt: c.end_opt,
    };
    let mut r = FileReader::open(drive, provider, opts)?;
    let label = r.label().clone();
    let mut buf = vec![0u8; MAX_BLOCK];
    let mut blocks = Vec::new();
    while let Some(n) = r.read_block(&mut buf)? {
        blocks.push(buf[..n].to_vec());
    }
    r.finish()?;
    let format = c.format.unwrap_or(if c.unlabeled { RecordFormat::Undefined } else { label.record_format });
    let rcdlen = match c.rcdlen {
        0 if c.unlabeled => blocks.first().map_or(1, Vec::len),
        0 => label.record_length as usize,
        n => n as usize,
    };
    let records = blocks_to_records(blocks.iter().map(Vec::as_slice), format, rcdlen.max(1))?;
    Ok((records, label))
}
