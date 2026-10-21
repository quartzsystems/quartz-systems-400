// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `*MSGF` payload: a header, then one record per description in
//! identifier order.

use store::spec::{Layout, field};

use crate::desc::{MsgDesc, VarType};
use crate::error::{MsgfError, Result};
use crate::source::MsgFile;

/// Size of `MsgfHdr`.
pub const MSGF_HDR_LEN: usize = 32;
/// Size of the fixed part of `MsgEntry`.
pub const MSG_ENTRY_LEN: usize = 32;
/// Size of `MsgVar`.
pub const MSG_VAR_LEN: usize = 8;
const MAGIC: &[u8; 8] = b"QS400MSG";
const VERSION: u32 = 1;

/// Layout of `MsgfHdr`, for the spec test.
pub static HDR_LAYOUT: Layout = Layout {
    name: "MsgfHdr",
    size: Some(MSGF_HDR_LEN),
    fields: &[
        field("magic", 0, 8),
        field("version", 8, 4),
        field("count", 12, 4),
        field("ccsid", 16, 2),
        field("reserved", 18, 14),
    ],
};

/// Layout of `MsgEntry`, for the spec test.
pub static ENTRY_LAYOUT: Layout = Layout {
    name: "MsgEntry",
    size: Some(MSG_ENTRY_LEN),
    fields: &[
        field("id", 0, 7),
        field("reserved", 7, 1),
        field("severity", 8, 2),
        field("var_count", 10, 2),
        field("dump_count", 12, 2),
        field("reserved2", 14, 2),
        field("text_len", 16, 4),
        field("help_len", 20, 4),
        field("dft_reply_len", 24, 4),
        field("valid_len", 28, 4),
    ],
};

/// Layout of `MsgVar`, for the spec test.
pub static VAR_LAYOUT: Layout = Layout {
    name: "MsgVar",
    size: Some(MSG_VAR_LEN),
    fields: &[
        field("kind", 0, 1),
        field("prefix", 1, 1),
        field("scale", 2, 1),
        field("reserved", 3, 1),
        field("len", 4, 4),
    ],
};

fn var_code(v: &VarType) -> (u8, u8, u8, u32) {
    match v {
        VarType::Char(n) => (1, 0, 0, *n),
        VarType::VaryChar(p) => (2, *p, 0, 0),
        VarType::Dec(d, s) => (3, 0, *s, u32::from(*d)),
        VarType::Hex(n) => (4, 0, 0, *n),
        VarType::QtdChar(n) => (5, 0, 0, *n),
        VarType::Bin(p) => (6, *p, 0, 0),
        VarType::Dts => (7, 0, 0, 0),
        VarType::CChar(n) => (8, 0, 0, *n),
        VarType::IType(n) => (9, 0, 0, *n),
        VarType::Utc => (10, 0, 0, 0),
        VarType::Spp => (11, 0, 0, 0),
        VarType::Syp => (12, 0, 0, 0),
    }
}

fn var_of(kind: u8, prefix: u8, scale: u8, len: u32) -> Result<VarType> {
    Ok(match kind {
        1 => VarType::Char(len),
        2 => VarType::VaryChar(prefix),
        3 => VarType::Dec(
            u8::try_from(len).map_err(|_| MsgfError::Payload("decimal digits".into()))?,
            scale,
        ),
        4 => VarType::Hex(len),
        5 => VarType::QtdChar(len),
        6 => VarType::Bin(prefix),
        7 => VarType::Dts,
        8 => VarType::CChar(len),
        9 => VarType::IType(len),
        10 => VarType::Utc,
        11 => VarType::Spp,
        12 => VarType::Syp,
        k => return Err(MsgfError::Payload(format!("{k} is not a variable kind"))),
    })
}

/// The payload bytes of a file.
#[must_use]
pub fn encode(f: &MsgFile) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(
        &u32::try_from(f.msgs.len())
            .expect("few messages")
            .to_le_bytes(),
    );
    out.extend_from_slice(&37u16.to_le_bytes());
    out.resize(MSGF_HDR_LEN, 0);
    for m in &f.msgs {
        let mut id = [ccsid::SPACE; 7];
        let _ = ccsid::encode_fixed(&m.id, &mut id);
        out.extend_from_slice(&id);
        out.push(0);
        out.extend_from_slice(&m.severity.to_le_bytes());
        out.extend_from_slice(&u16::try_from(m.vars.len()).expect("few").to_le_bytes());
        out.extend_from_slice(&u16::try_from(m.dump.len()).expect("few").to_le_bytes());
        out.extend_from_slice(&[0, 0]);
        let dft = m.dft_reply.clone().unwrap_or_default();
        let valid = m.valid_replies.join(",");
        for s in [&m.text, &m.help, &dft, &valid] {
            out.extend_from_slice(&u32::try_from(s.len()).expect("fits").to_le_bytes());
        }
        for s in [&m.text, &m.help, &dft, &valid] {
            out.extend_from_slice(s.as_bytes());
        }
        for v in &m.vars {
            let (k, p, s, len) = var_code(v);
            out.extend_from_slice(&[k, p, s, 0]);
            out.extend_from_slice(&len.to_le_bytes());
        }
        for d in &m.dump {
            out.extend_from_slice(&d.to_le_bytes());
        }
    }
    out
}

struct R<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> R<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(n)
            .ok_or_else(|| MsgfError::Payload("overflow".into()))?;
        if end > self.b.len() {
            return Err(MsgfError::Payload(format!("truncated at byte {}", self.at)));
        }
        let s = &self.b[self.at..end];
        self.at = end;
        Ok(s)
    }
    fn u16(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn str(&mut self, n: u32) -> Result<String> {
        let b = self.take(usize::try_from(n).map_err(|_| MsgfError::Payload("length".into()))?)?;
        String::from_utf8(b.to_vec()).map_err(|_| MsgfError::Payload("text is not UTF-8".into()))
    }
}

/// Read a payload; the name comes from the object header.
pub fn decode(bytes: &[u8], name: &str, text: &str) -> Result<MsgFile> {
    let mut r = R { b: bytes, at: 0 };
    if r.take(8)? != MAGIC {
        return Err(MsgfError::Payload("wrong magic".into()));
    }
    let version = r.u32()?;
    if version != VERSION {
        return Err(MsgfError::Payload(format!("payload version {version}")));
    }
    let count = r.u32()?;
    let _ccsid = r.u16()?;
    r.take(14)?;
    let mut msgs = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let id = ccsid::decode_trimmed(r.take(7)?);
        r.take(1)?;
        let severity = r.u16()?;
        let var_count = r.u16()?;
        let dump_count = r.u16()?;
        r.take(2)?;
        let text_len = r.u32()?;
        let help_len = r.u32()?;
        let dft_len = r.u32()?;
        let valid_len = r.u32()?;
        let text = r.str(text_len)?;
        let help = r.str(help_len)?;
        let dft = r.str(dft_len)?;
        let valid = r.str(valid_len)?;
        let mut vars = Vec::with_capacity(usize::from(var_count));
        for _ in 0..var_count {
            let h = r.take(4)?;
            let len = r.u32()?;
            vars.push(var_of(h[0], h[1], h[2], len)?);
        }
        let mut dump = Vec::with_capacity(usize::from(dump_count));
        for _ in 0..dump_count {
            dump.push(r.u16()?);
        }
        msgs.push(MsgDesc {
            id,
            severity,
            text,
            help,
            vars,
            dft_reply: if dft.is_empty() { None } else { Some(dft) },
            valid_replies: if valid.is_empty() {
                Vec::new()
            } else {
                valid.split(',').map(str::to_string).collect()
            },
            dump,
        });
    }
    Ok(MsgFile {
        name: name.into(),
        text: text.into(),
        msgs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_round_trips_the_builtin_files() {
        for f in crate::source::builtin() {
            let bytes = encode(&f);
            let again = decode(&bytes, &f.name, &f.text).unwrap();
            assert_eq!(again, f);
        }
    }
}
