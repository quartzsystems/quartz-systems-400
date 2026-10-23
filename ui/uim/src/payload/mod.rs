// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `*PNLGRP` payload: a fixed [`HEADER_LAYOUT`] header, then
//! tagged records ([`TAIL_LAYOUT`]) holding the source lines and the
//! model. Text in the header is CP037, blank padded; integers are
//! little-endian. A reader skips records whose tag it does not know.
//! `docs/UIM.md` §6 holds the tables.

mod codec;

use thiserror::Error;

use crate::model::PanelGroup;
use crate::source::SourceLine;
use crate::spec::{Layout, field};

/// The header.
pub const HEADER_LAYOUT: Layout = Layout {
    name: "PnlgrpHdr",
    size: Some(64),
    fields: &[
        field("Magic", 0, 8),
        field("Version", 8, 2),
        field("Flags", 10, 2),
        field("Name", 12, 10),
        field("Library", 22, 10),
        field("PanelCount", 32, 2),
        field("PrtPanelCount", 34, 2),
        field("HelpCount", 36, 2),
        field("VarCount", 38, 2),
        field("ClassCount", 40, 2),
        field("CondCount", 42, 2),
        field("KeylCount", 44, 2),
        field("ImportCount", 46, 2),
        field("SourceLines", 48, 4),
        field("Reserved", 52, 12),
    ],
};

/// One record of the tail; its bytes follow.
pub const TAIL_LAYOUT: Layout = Layout {
    name: "PnlgrpTail",
    size: Some(8),
    fields: &[
        field("Tag", 0, 2),
        field("Reserved", 2, 2),
        field("Len", 4, 4),
    ],
};

const MAGIC: &[u8; 8] = b"QSUIPNLG";
const VERSION: u16 = 1;
const TAG_SOURCE: u16 = 1;
const TAG_MODEL: u16 = 2;

/// What the payload holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Payload {
    /// The panel group's name.
    pub name: String,
    /// Its library.
    pub library: String,
    /// The model.
    pub group: PanelGroup,
    /// The source as compiled, imbeds expanded.
    pub source: Vec<SourceLine>,
}

/// Why a payload could not be read.
#[derive(Debug, Error)]
pub enum PayloadError {
    /// Fewer bytes than the header.
    #[error("payload of {0} bytes is shorter than the header")]
    Short(usize),
    /// Not a panel group.
    #[error("payload is not a panel group")]
    Magic,
    /// A version this code does not read.
    #[error("payload version {0} is not supported")]
    Version(u16),
    /// A tail record runs past the end.
    #[error("tail record {tag} of {len} bytes runs past the end")]
    Truncated {
        /// Its tag.
        tag: u16,
        /// Its length.
        len: usize,
    },
    /// The model record is missing.
    #[error("payload has no model record")]
    NoModel,
    /// A record that does not decode.
    #[error("record {tag}: {source}")]
    Record {
        /// Its tag.
        tag: u16,
        /// The codec's error.
        #[source]
        source: ipc::WireError,
    },
}

/// Encode a payload.
#[must_use]
pub fn encode(p: &Payload) -> Vec<u8> {
    let mut out = vec![0u8; 64];
    out[..8].copy_from_slice(MAGIC);
    put_u16(&mut out, 8, VERSION);
    put_text(&mut out, 12, &p.name);
    put_text(&mut out, 22, &p.library);
    let g = &p.group;
    put_u16(&mut out, 32, count(g.panels.len()));
    put_u16(&mut out, 34, count(g.print_panels.len()));
    put_u16(&mut out, 36, count(g.helps.len()));
    put_u16(&mut out, 38, count(g.variables.len()));
    put_u16(&mut out, 40, count(g.classes.len()));
    put_u16(&mut out, 42, count(g.conditions.len()));
    put_u16(&mut out, 44, count(g.key_lists.len()));
    put_u16(&mut out, 46, count(g.imports.len()));
    let lines = u32::try_from(p.source.len()).unwrap_or(u32::MAX);
    out[48..52].copy_from_slice(&lines.to_le_bytes());
    record(&mut out, TAG_SOURCE, &ipc::encode(&p.source));
    record(&mut out, TAG_MODEL, &ipc::encode(&p.group));
    out
}

/// Decode a payload.
pub fn decode(bytes: &[u8]) -> Result<Payload, PayloadError> {
    if bytes.len() < 64 {
        return Err(PayloadError::Short(bytes.len()));
    }
    if &bytes[..8] != MAGIC {
        return Err(PayloadError::Magic);
    }
    let version = u16::from_le_bytes([bytes[8], bytes[9]]);
    if version != VERSION {
        return Err(PayloadError::Version(version));
    }
    let name = text_of(&bytes[12..22]);
    let library = text_of(&bytes[22..32]);
    let mut at = 64;
    let mut source = Vec::new();
    let mut group = None;
    while at + 8 <= bytes.len() {
        let tag = u16::from_le_bytes([bytes[at], bytes[at + 1]]);
        let len = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().expect("4 bytes")) as usize;
        at += 8;
        let body = bytes
            .get(at..at + len)
            .ok_or(PayloadError::Truncated { tag, len })?;
        at += len;
        match tag {
            TAG_SOURCE => {
                source =
                    ipc::decode(body).map_err(|source| PayloadError::Record { tag, source })?;
            }
            TAG_MODEL => {
                group =
                    Some(ipc::decode(body).map_err(|source| PayloadError::Record { tag, source })?);
            }
            _ => {}
        }
    }
    Ok(Payload {
        name,
        library,
        group: group.ok_or(PayloadError::NoModel)?,
        source,
    })
}

fn count(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

fn put_u16(out: &mut [u8], at: usize, v: u16) {
    out[at..at + 2].copy_from_slice(&v.to_le_bytes());
}

/// Ten bytes of CP037, blank padded; characters CP037 lacks become blanks.
fn put_text(out: &mut [u8], at: usize, s: &str) {
    let mut b: Vec<u8> = s
        .chars()
        .map(|c| ccsid::to_ebcdic(&c.to_string()).map_or(0x40, |v| v[0]))
        .take(10)
        .collect();
    b.resize(10, 0x40);
    out[at..at + 10].copy_from_slice(&b);
}

fn text_of(b: &[u8]) -> String {
    ccsid::from_ebcdic(b)
        .trim_end_matches([' ', '\0'])
        .to_string()
}

fn record(out: &mut Vec<u8>, tag: u16, body: &[u8]) {
    out.extend_from_slice(&tag.to_le_bytes());
    out.extend_from_slice(&[0, 0]);
    let len = u32::try_from(body.len()).expect("record under 4 GiB");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(body);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_and_errors() {
        let p = Payload {
            name: "MYGRP".into(),
            library: "QGPL".into(),
            group: PanelGroup::default(),
            source: vec![SourceLine {
                seq: 100,
                text: ":PNLGRP.".into(),
                imbed: None,
            }],
        };
        let bytes = encode(&p);
        assert_eq!(&bytes[..8], MAGIC);
        assert_eq!(u32::from_le_bytes(bytes[48..52].try_into().unwrap()), 1);
        assert_eq!(decode(&bytes).unwrap(), p);
        assert!(matches!(decode(&bytes[..10]), Err(PayloadError::Short(10))));
        let mut bad = bytes.clone();
        bad[0] = b'X';
        assert!(matches!(decode(&bad), Err(PayloadError::Magic)));
        let mut v2 = bytes.clone();
        v2[8] = 2;
        assert!(matches!(decode(&v2), Err(PayloadError::Version(2))));
        let short = &bytes[..bytes.len() - 1];
        assert!(matches!(
            decode(short),
            Err(PayloadError::Truncated { tag: 2, .. })
        ));
        assert!(matches!(decode(&bytes[..64]), Err(PayloadError::NoModel)));
    }
}
