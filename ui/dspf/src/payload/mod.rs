// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The display attributes of a `*FILE` object: a fixed header laid out
//! by [`LAYOUT`], the model as a tagged tail, then the DDS source as
//! [`SOURCE_LAYOUT`] records. `docs/DSPF.md` holds the tables that
//! `tests/spec.rs` compares these layouts with. Text in the header is
//! CP037, blank padded; integers are little-endian.

mod codec;

use db::spec::{Layout, field};
use dds::{RECORD_LEN, Source, SourceLine};

use crate::model::DisplayFile;

/// The fixed header.
pub const LAYOUT: Layout = Layout {
    name: "DisplayFile",
    size: Some(64),
    fields: &[
        field("Magic", 0, 8),
        field("Version", 8, 2),
        field("Flags", 10, 2),
        field("Sizes", 12, 1),
        field("Rows1", 13, 1),
        field("Cols1", 14, 1),
        field("Rows2", 15, 1),
        field("Cols2", 16, 1),
        field("MsgLoc", 17, 1),
        field("Ccsid", 18, 2),
        field("Records", 20, 2),
        field("SourceLines", 22, 4),
        field("ModelLen", 26, 4),
        field("SourceLen", 30, 4),
        field("RefFile", 34, 10),
        field("RefLib", 44, 10),
        field("Reserved", 54, 10),
    ],
};

/// One source record after the model.
pub const SOURCE_LAYOUT: Layout = Layout {
    name: "SourceRecord",
    size: Some(88),
    fields: &[
        field("Seq", 0, 4),
        field("Date", 4, 4),
        field("Text", 8, 80),
    ],
};

const MAGIC: &[u8; 8] = b"QSUIDSPF";
const VERSION: u16 = 1;
const HEADER: usize = 64;
const SOURCE_RECORD: usize = 88;

/// `Flags` bits.
mod flags {
    pub const INDARA: u16 = 1;
    pub const RSTDSP: u16 = 2;
    pub const USRDSPMGT: u16 = 4;
    pub const CSRINPONLY: u16 = 8;
    pub const DSPRL: u16 = 16;
    pub const ERRSFL: u16 = 32;
}

/// Why a payload could not be read.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PayloadError {
    /// Fewer bytes than the header or its sections say.
    #[error("display file payload: {have} bytes, {need} needed")]
    Short {
        /// Bytes needed.
        need: usize,
        /// Bytes there.
        have: usize,
    },
    /// The magic is not ours.
    #[error("not a display file payload")]
    NotDisplayFile,
    /// A version this code does not read.
    #[error("display file payload version {0} is not supported")]
    Version(u16),
    /// The model section does not decode.
    #[error("display file payload is damaged: {0}")]
    Damaged(&'static str),
}

fn slot(name: &str) -> (usize, usize) {
    let f = LAYOUT
        .fields
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("DisplayFile has no field {name}"));
    (f.offset.expect("fixed"), f.size.expect("fixed"))
}

fn put(buf: &mut [u8], name: &str, bytes: &[u8]) {
    let (o, n) = slot(name);
    let k = bytes.len().min(n);
    buf[o..o + k].copy_from_slice(&bytes[..k]);
}

fn put_uint(buf: &mut [u8], name: &str, v: u64) {
    let (_, n) = slot(name);
    put(buf, name, &v.to_le_bytes()[..n.min(8)]);
}

fn put_text(buf: &mut [u8], name: &str, s: &str) {
    let (_, n) = slot(name);
    let mut b: Vec<u8> = s
        .chars()
        .map(|c| ccsid::byte_of(c).unwrap_or(ccsid::SPACE))
        .take(n)
        .collect();
    b.resize(n, ccsid::SPACE);
    put(buf, name, &b);
}

fn get<'a>(buf: &'a [u8], name: &str) -> &'a [u8] {
    let (o, n) = slot(name);
    &buf[o..o + n]
}

fn get_uint(buf: &[u8], name: &str) -> u64 {
    let b = get(buf, name);
    let mut v = [0u8; 8];
    v[..b.len().min(8)].copy_from_slice(&b[..b.len().min(8)]);
    u64::from_le_bytes(v)
}

fn get_text(buf: &[u8], name: &str) -> String {
    ccsid::decode_trimmed(get(buf, name))
}

/// The bytes of a display file.
#[must_use]
pub fn encode(df: &DisplayFile) -> Vec<u8> {
    let model = codec::write(df);
    let mut buf = vec![0u8; HEADER];
    put(&mut buf, "Magic", MAGIC);
    put_uint(&mut buf, "Version", u64::from(VERSION));
    let f = &df.file;
    let mut fl = 0u16;
    for (on, bit) in [
        (f.indara, flags::INDARA),
        (f.rstdsp, flags::RSTDSP),
        (f.usrdspmgt, flags::USRDSPMGT),
        (f.csrinponly, flags::CSRINPONLY),
        (f.dsprl, flags::DSPRL),
        (f.errsfl, flags::ERRSFL),
    ] {
        if on {
            fl |= bit;
        }
    }
    put_uint(&mut buf, "Flags", u64::from(fl));
    put_uint(&mut buf, "Sizes", f.sizes.len() as u64);
    if let Some(s) = f.sizes.first() {
        put_uint(&mut buf, "Rows1", u64::from(s.rows));
        put_uint(&mut buf, "Cols1", u64::from(s.cols));
    }
    if let Some(s) = f.sizes.get(1) {
        put_uint(&mut buf, "Rows2", u64::from(s.rows));
        put_uint(&mut buf, "Cols2", u64::from(s.cols));
    }
    put_uint(&mut buf, "MsgLoc", u64::from(f.msgloc.unwrap_or(0)));
    put_uint(&mut buf, "Ccsid", u64::from(f.ccsid.unwrap_or(0)));
    put_uint(&mut buf, "Records", df.records.len() as u64);
    put_uint(&mut buf, "SourceLines", df.source.len() as u64);
    put_uint(&mut buf, "ModelLen", model.len() as u64);
    put_uint(
        &mut buf,
        "SourceLen",
        (df.source.len() * SOURCE_RECORD) as u64,
    );
    if let Some(r) = &f.ref_file {
        put_text(&mut buf, "RefFile", &r.file.to_string());
        put_text(&mut buf, "RefLib", &r.library.to_string());
    } else {
        put_text(&mut buf, "RefFile", "");
        put_text(&mut buf, "RefLib", "");
    }
    buf.extend_from_slice(&model);
    for l in df.source.lines() {
        buf.extend_from_slice(&l.seq.to_le_bytes());
        buf.extend_from_slice(&l.date.to_le_bytes());
        buf.extend_from_slice(&l.text);
    }
    buf
}

/// A display file back from its bytes.
pub fn decode(b: &[u8]) -> Result<DisplayFile, PayloadError> {
    if b.len() < HEADER {
        return Err(PayloadError::Short {
            need: HEADER,
            have: b.len(),
        });
    }
    if get(b, "Magic") != MAGIC {
        return Err(PayloadError::NotDisplayFile);
    }
    let version = u16::try_from(get_uint(b, "Version")).unwrap_or(u16::MAX);
    if version != VERSION {
        return Err(PayloadError::Version(version));
    }
    let model_len = usize::try_from(get_uint(b, "ModelLen")).unwrap_or(usize::MAX);
    let source_len = usize::try_from(get_uint(b, "SourceLen")).unwrap_or(usize::MAX);
    let need = HEADER
        .checked_add(model_len)
        .and_then(|n| n.checked_add(source_len))
        .ok_or(PayloadError::Damaged("section lengths overflow"))?;
    if b.len() < need {
        return Err(PayloadError::Short {
            need,
            have: b.len(),
        });
    }
    let mut df =
        codec::read(&b[HEADER..HEADER + model_len]).ok_or(PayloadError::Damaged("model"))?;
    let src = &b[HEADER + model_len..need];
    if !src.len().is_multiple_of(SOURCE_RECORD) {
        return Err(PayloadError::Damaged("source records"));
    }
    let lines = src.chunks(SOURCE_RECORD).map(|c| {
        let seq = u32::from_le_bytes([c[0], c[1], c[2], c[3]]);
        let date = u32::from_le_bytes([c[4], c[5], c[6], c[7]]);
        (seq, date, &c[8..8 + RECORD_LEN])
    });
    df.source = Source::from_records(lines);
    let _ = get_text(b, "RefFile");
    Ok(df)
}

/// The source records as the payload holds them, for callers that
/// only want the DDS back.
#[must_use]
pub fn source_lines(df: &DisplayFile) -> &[SourceLine] {
    df.source.lines()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_round_trips() {
        let (source, _) = Source::from_text("     A          R REC\n");
        let df = DisplayFile {
            source,
            ..DisplayFile::default()
        };
        let bytes = encode(&df);
        assert_eq!(&bytes[..8], MAGIC);
        assert_eq!(get_uint(&bytes, "Rows1"), 24);
        assert_eq!(get_uint(&bytes, "SourceLines"), 1);
        assert_eq!(get_text(&bytes, "RefFile"), "");
        let back = decode(&bytes).unwrap();
        assert_eq!(back, df);
        assert_eq!(source_lines(&back).len(), 1);
    }

    #[test]
    fn damage_is_reported() {
        assert_eq!(
            decode(&[0; 10]),
            Err(PayloadError::Short { need: 64, have: 10 })
        );
        let mut b = encode(&DisplayFile::default());
        b[0] = b'X';
        assert_eq!(decode(&b), Err(PayloadError::NotDisplayFile));
        let mut b = encode(&DisplayFile::default());
        b[8] = 9;
        assert_eq!(decode(&b), Err(PayloadError::Version(9)));
        let mut b = encode(&DisplayFile::default());
        b[26] = 0xFF;
        assert!(matches!(decode(&b), Err(PayloadError::Short { .. })));
        let mut b = encode(&DisplayFile::default());
        let n = b.len();
        b[HEADER..n].fill(0xEE);
        assert_eq!(decode(&b), Err(PayloadError::Damaged("model")));
    }
}
