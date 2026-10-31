// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! IBM standard labels: VOL1 at the beginning of the volume, HDR1 and
//! HDR2 before each file, EOF1 and EOF2 after it, EOV1 and EOV2 where a
//! file continues on the next volume. The layouts are in [`layout`] and
//! in `docs/MEDIA.md`.

mod code;
mod date;
pub mod layout;

pub use code::Code;
pub use date::{Date, Expiry, PERM_LABEL};

use layout as l;

/// The system code written in HDR1.
pub const SYSTEM_CODE: &str = "QS/400";

/// A label could not be built or read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LabelError {
    /// A value does not fit its field or code.
    #[error("value {0:?} does not fit its label field")]
    Field(String),
    /// The block is not the label expected.
    #[error("expected a {expected} label, found {found:?}")]
    Wrong {
        /// The label expected.
        expected: &'static str,
        /// What the block starts with.
        found: String,
    },
}

/// The VOL1 label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vol1 {
    /// Volume identifier, 1 to 6 characters.
    pub volume_id: String,
    /// Owner identifier, up to 14 characters.
    pub owner: String,
}

impl Vol1 {
    /// The 80-byte label.
    pub fn encode(&self, code: Code) -> Result<[u8; l::LABEL_LEN], LabelError> {
        let mut rec = [code.space(); l::LABEL_LEN];
        code.put_field(&mut rec, l::LABEL_ID, "VOL")?;
        code.put_field(&mut rec, l::LABEL_NUMBER, "1")?;
        if self.volume_id.is_empty() {
            return Err(LabelError::Field(String::new()));
        }
        code.put_field(&mut rec, l::VOL_ID, &self.volume_id)?;
        code.put_field(&mut rec, l::VOL_OWNER, &self.owner)?;
        Ok(rec)
    }

    /// Decode a block that may be a VOL1 label, finding its code.
    pub fn decode(rec: &[u8]) -> Result<(Self, Code), LabelError> {
        let wrong = || LabelError::Wrong {
            expected: "VOL1",
            found: Code::Ebcdic.get(&rec[..rec.len().min(4)]),
        };
        if rec.len() != l::LABEL_LEN {
            return Err(wrong());
        }
        let code = Code::detect(rec, "VOL1").ok_or_else(wrong)?;
        Ok((
            Self {
                volume_id: code.get_field(rec, l::VOL_ID),
                owner: code.get_field(rec, l::VOL_OWNER),
            },
            code,
        ))
    }
}

/// Which pair of file labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelSet {
    /// HDR1 and HDR2, before the data.
    Header,
    /// EOF1 and EOF2, after the file's last data.
    EndOfFile,
    /// EOV1 and EOV2: the file continues on the next volume.
    EndOfVolume,
}

impl LabelSet {
    fn ident(self) -> &'static str {
        match self {
            Self::Header => "HDR",
            Self::EndOfFile => "EOF",
            Self::EndOfVolume => "EOV",
        }
    }

    fn of(ident: &str) -> Option<Self> {
        [Self::Header, Self::EndOfFile, Self::EndOfVolume]
            .into_iter()
            .find(|s| s.ident() == ident)
    }
}

/// Record format in HDR2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RecordFormat {
    /// Fixed-length records, blocked.
    #[default]
    Fixed,
    /// Variable-length records with descriptor words.
    Variable,
    /// Undefined: one record per block.
    Undefined,
}

impl RecordFormat {
    fn letter(self) -> &'static str {
        match self {
            Self::Fixed => "F",
            Self::Variable => "V",
            Self::Undefined => "U",
        }
    }

    fn of(s: &str) -> Option<Self> {
        match s {
            "F" => Some(Self::Fixed),
            "V" => Some(Self::Variable),
            "U" => Some(Self::Undefined),
            _ => None,
        }
    }
}

/// What HDR1 and HDR2 (or the EOF and EOV pairs) say about a file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FileLabel {
    /// File identifier (FILELABEL), up to 17 characters.
    pub file_id: String,
    /// Volume identifier of the file's first volume.
    pub set_id: String,
    /// Volume sequence number within the file, from 1.
    pub volume_seq: u32,
    /// File sequence number (SEQNBR).
    pub file_seq: u32,
    /// Generation number.
    pub generation: u32,
    /// Generation version.
    pub version: u32,
    /// Creation date.
    pub created: Option<Date>,
    /// Expiration.
    pub expires: Expiry,
    /// Data blocks on this volume (zero in HDR1).
    pub block_count: u64,
    /// System code of the writer.
    pub system_code: String,
    /// Record format.
    pub record_format: RecordFormat,
    /// Block length.
    pub block_length: u32,
    /// Record length.
    pub record_length: u32,
    /// The file continues from the previous volume.
    pub continued: bool,
    /// Job and step that wrote it.
    pub job_step: String,
}

fn ident_of(rec: &[u8], code: Code) -> String {
    code.get(&rec[..4])
}

impl FileLabel {
    /// The two 80-byte labels of `set`.
    pub fn encode(
        &self,
        set: LabelSet,
        code: Code,
    ) -> Result<([u8; l::LABEL_LEN], [u8; l::LABEL_LEN]), LabelError> {
        let mut r1 = [code.space(); l::LABEL_LEN];
        code.put_field(&mut r1, l::LABEL_ID, set.ident())?;
        code.put_field(&mut r1, l::LABEL_NUMBER, "1")?;
        code.put_field(&mut r1, l::H1_FILE_ID, &self.file_id)?;
        code.put_field(&mut r1, l::H1_SET_ID, &self.set_id)?;
        code.put_num(&mut r1, l::H1_VOL_SEQ, u64::from(self.volume_seq))?;
        code.put_num(&mut r1, l::H1_FILE_SEQ, u64::from(self.file_seq))?;
        code.put_num(&mut r1, l::H1_GENERATION, u64::from(self.generation))?;
        code.put_num(&mut r1, l::H1_VERSION, u64::from(self.version))?;
        let created = self
            .created
            .map_or_else(|| " 00000".to_string(), Date::to_label);
        code.put_field(&mut r1, l::H1_CREATED, &created)?;
        code.put_field(&mut r1, l::H1_EXPIRES, &self.expires.to_label())?;
        code.put_field(&mut r1, l::H1_SECURITY, "0")?;
        let blocks = if set == LabelSet::Header {
            0
        } else {
            self.block_count
        };
        if blocks >= 10_000_000_000 {
            return Err(LabelError::Field(blocks.to_string()));
        }
        code.put_num(&mut r1, l::H1_BLOCKS, blocks % 1_000_000)?;
        code.put_field(&mut r1, l::H1_SYSTEM, &self.system_code)?;
        code.put_num(&mut r1, l::H1_BLOCKS_HIGH, blocks / 1_000_000)?;

        let mut r2 = [code.space(); l::LABEL_LEN];
        code.put_field(&mut r2, l::LABEL_ID, set.ident())?;
        code.put_field(&mut r2, l::LABEL_NUMBER, "2")?;
        code.put_field(&mut r2, l::H2_FORMAT, self.record_format.letter())?;
        if self.block_length > 99_999 {
            code.put_num(&mut r2, l::H2_BLOCK_LEN, 0)?;
            code.put_num(&mut r2, l::H2_LARGE_BLOCK, u64::from(self.block_length))?;
        } else {
            code.put_num(&mut r2, l::H2_BLOCK_LEN, u64::from(self.block_length))?;
        }
        code.put_num(&mut r2, l::H2_RECORD_LEN, u64::from(self.record_length))?;
        code.put_field(
            &mut r2,
            l::H2_SWITCH,
            if self.continued { "1" } else { "0" },
        )?;
        code.put_field(&mut r2, l::H2_JOB, &self.job_step)?;
        let attr = match self.record_format {
            RecordFormat::Undefined => "",
            RecordFormat::Fixed | RecordFormat::Variable => "B",
        };
        code.put_field(&mut r2, l::H2_BLOCK_ATTR, attr)?;
        Ok((r1, r2))
    }

    /// Whether a block is the first label of a file label pair.
    #[must_use]
    pub fn peek(rec: &[u8], code: Code) -> Option<LabelSet> {
        if rec.len() != l::LABEL_LEN || code.get_field(rec, l::LABEL_NUMBER) != "1" {
            return None;
        }
        LabelSet::of(&code.get_field(rec, l::LABEL_ID))
    }

    /// Decode a label pair.
    pub fn decode(r1: &[u8], r2: &[u8], code: Code) -> Result<(LabelSet, Self), LabelError> {
        let set = Self::peek(r1, code).ok_or_else(|| LabelError::Wrong {
            expected: "HDR1, EOF1 or EOV1",
            found: ident_of(r1, code),
        })?;
        let want2 = format!("{}2", set.ident());
        if r2.len() != l::LABEL_LEN || ident_of(r2, code) != want2 {
            return Err(LabelError::Wrong {
                expected: match set {
                    LabelSet::Header => "HDR2",
                    LabelSet::EndOfFile => "EOF2",
                    LabelSet::EndOfVolume => "EOV2",
                },
                found: if r2.len() >= 4 {
                    ident_of(r2, code)
                } else {
                    String::new()
                },
            });
        }
        let n32 = |rec: &[u8], f| -> Result<u32, LabelError> {
            let v = code.get_num(rec, f)?;
            u32::try_from(v).map_err(|_| LabelError::Field(v.to_string()))
        };
        let large = n32(r2, l::H2_LARGE_BLOCK)?;
        let block_length = match n32(r2, l::H2_BLOCK_LEN)? {
            0 => large,
            n => n,
        };
        let format = code.get_field(r2, l::H2_FORMAT);
        Ok((
            set,
            Self {
                file_id: code.get_field(r1, l::H1_FILE_ID),
                set_id: code.get_field(r1, l::H1_SET_ID),
                volume_seq: n32(r1, l::H1_VOL_SEQ)?,
                file_seq: n32(r1, l::H1_FILE_SEQ)?,
                generation: n32(r1, l::H1_GENERATION)?,
                version: n32(r1, l::H1_VERSION)?,
                created: Date::from_label(&raw_field(r1, l::H1_CREATED, code)),
                expires: Expiry::from_label(&raw_field(r1, l::H1_EXPIRES, code)),
                block_count: code.get_num(r1, l::H1_BLOCKS_HIGH)? * 1_000_000
                    + code.get_num(r1, l::H1_BLOCKS)?,
                system_code: code.get_field(r1, l::H1_SYSTEM),
                record_format: RecordFormat::of(&format).ok_or(LabelError::Field(format))?,
                block_length,
                record_length: n32(r2, l::H2_RECORD_LEN)?,
                continued: code.get_field(r2, l::H2_SWITCH) == "1",
                job_step: code.get_field(r2, l::H2_JOB),
            },
        ))
    }
}

/// A field without trimming (dates keep their leading blank).
fn raw_field(rec: &[u8], f: crate::spec::Field, code: Code) -> String {
    let s = code.get_field(rec, f);
    format!("{s:>width$}", width = f.size)
}
