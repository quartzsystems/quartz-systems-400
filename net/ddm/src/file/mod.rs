// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Record-level access as IBM i systems speak it to each other and as
//! `JTOpen` speaks it: the System/38 extension commands (`S38OPEN`,
//! `S38GET`, `S38GETK`, `S38GETM`, `S38PUTM`, `S38UPDAT`, `S38DEL`,
//! `S38FEOD`, `S38CLOSE`, `S38CMD`) with the file named by `DCLNAM`,
//! the user file control block (`S38UFCB`), the operation list
//! (`S38OPTL`), the control list (`S38CTLL`), and the replies: open
//! feedback (`S38OPNFB`), records (`S38BUF`), I/O feedback (`S38IOFB`)
//! and IBM i messages (`S38MSGRM`). These are DDM's open, get, insert,
//! update, delete and close for files; layouts follow the pub400 traces
//! (`tools/conformance/ddm/traces/rla-*.trace`).

pub mod feedback;
pub mod ufcb;

pub use feedback::{Iofb, OpenFeedback};
pub use ufcb::Ufcb;

use crate::Error;
use crate::cp::{param, s38};
use crate::wire::{Object, ebcdic_fixed, text};

/// The width of a declared name.
pub const DCLNAM_LEN: usize = 8;

/// `DCLNAM`: the requester's handle for an open file.
pub fn dclnam(name: &str) -> Object {
    Object::data(param::DCLNAM, ebcdic_fixed(name, DCLNAM_LEN))
}

/// Operation list codes (`S38OPTL` byte 0) seen on the wire.
pub mod op {
    /// Position to the first record.
    pub const FIRST: u8 = 0x01;
    /// The next record.
    pub const NEXT: u8 = 0x03;
    /// The record whose key equals the one given.
    pub const KEY_EQUAL: u8 = 0x0B;
    /// Update the record last read.
    pub const UPDATE: u8 = 0x21;
}

/// Lock codes (`S38OPTL` byte 1).
pub mod lock {
    /// Read without a lock.
    pub const NONE: u8 = 0x00;
    /// Read for update.
    pub const UPDATE: u8 = 0x03;
}

/// `S38OPTL`: the operation, the lock and two option bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Optl {
    /// The operation.
    pub op: u8,
    /// The lock.
    pub lock: u8,
    /// Option bytes as sent.
    pub opts: u16,
}

impl Optl {
    /// The parameter.
    pub fn to_object(self) -> Object {
        let mut d = vec![self.op, self.lock];
        d.extend(self.opts.to_be_bytes());
        Object::data(param::S38OPTL, d)
    }

    /// From a command.
    pub fn of(cmd: &Object) -> Option<Self> {
        let b = cmd.get(param::S38OPTL)?.bytes();
        (b.len() >= 4).then(|| Self {
            op: b[0],
            lock: b[1],
            opts: u16::from_be_bytes([b[2], b[3]]),
        })
    }
}

/// One entry of `S38CTLL`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ctl {
    /// `0x01`: the record format name.
    Format(String),
    /// `0x07`: key bytes.
    Key(Vec<u8>),
    /// `0x08`: how many key fields the key covers.
    KeyFields(u32),
    /// `0x10`: how many records to transfer.
    Count(u16),
    /// Anything else, kept.
    Other(u8, Vec<u8>),
}

/// The control list: entries of an ID byte, a two-byte length and the
/// data, ended by `0xFF`.
pub fn encode_ctll(entries: &[Ctl]) -> Object {
    let mut d = Vec::new();
    for e in entries {
        let (id, data) = match e {
            Ctl::Format(f) => (0x01, ebcdic_fixed(f, 10)),
            Ctl::Key(k) => (0x07, k.clone()),
            Ctl::KeyFields(n) => (0x08, n.to_be_bytes().to_vec()),
            Ctl::Count(n) => (0x10, n.to_be_bytes().to_vec()),
            Ctl::Other(id, data) => (*id, data.clone()),
        };
        d.push(id);
        d.extend(u16::try_from(data.len()).unwrap_or(0).to_be_bytes());
        d.extend(data);
    }
    d.push(0xFF);
    Object::data(param::S38CTLL, d)
}

/// Read a control list.
pub fn decode_ctll(b: &[u8]) -> Result<Vec<Ctl>, Error> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < b.len() && b[at] != 0xFF {
        if at + 3 > b.len() {
            return Err(Error::Invalid("S38CTLL entry".into()));
        }
        let id = b[at];
        let n = usize::from(u16::from_be_bytes([b[at + 1], b[at + 2]]));
        let d = b
            .get(at + 3..at + 3 + n)
            .ok_or_else(|| Error::Invalid("S38CTLL entry length".into()))?
            .to_vec();
        out.push(match (id, d.len()) {
            (0x01, _) => Ctl::Format(text(&d)),
            (0x07, _) => Ctl::Key(d),
            (0x08, 4) => Ctl::KeyFields(u32::from_be_bytes([d[0], d[1], d[2], d[3]])),
            (0x10, 2) => Ctl::Count(u16::from_be_bytes([d[0], d[1]])),
            _ => Ctl::Other(id, d),
        });
        at += 3 + n;
    }
    Ok(out)
}

/// The bytes after each record in `S38BUF`: a flag word, the relative
/// record number, five reserved bytes, then the null field map (one
/// EBCDIC `0`/`1` per field).
pub const TRAILER: usize = 11;

/// One record in `S38BUF`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rec {
    /// The record's bytes.
    pub data: Vec<u8>,
    /// Its relative record number (0 when the requester sends it).
    pub rrn: u32,
    /// The trailer's flag word.
    pub flags: u16,
    /// The byte at trailer offset 9 (`0x40` for a record read for
    /// update).
    pub locked: bool,
    /// The null field map: true for a null field.
    pub nulls: Vec<bool>,
}

impl Rec {
    /// A record with no nulls.
    pub fn new(data: Vec<u8>, fields: usize) -> Self {
        Self {
            data,
            rrn: 0,
            flags: 0,
            locked: false,
            nulls: vec![false; fields],
        }
    }
}

/// Records as `S38BUF` data.
pub fn encode_buf(recs: &[Rec]) -> Vec<u8> {
    let mut d = Vec::new();
    for r in recs {
        d.extend_from_slice(&r.data);
        d.extend(r.flags.to_be_bytes());
        d.extend(r.rrn.to_be_bytes());
        d.extend([0, 0, 0, if r.locked { 0x40 } else { 0 }, 0]);
        d.extend(r.nulls.iter().map(|&n| if n { 0xF1 } else { 0xF0 }));
    }
    d
}

/// Records of `S38BUF` data, each `reclen` bytes with `fields` null
/// map bytes after its trailer.
pub fn decode_buf(b: &[u8], reclen: usize, fields: usize) -> Result<Vec<Rec>, Error> {
    let each = reclen + TRAILER + fields;
    if each == 0 || !b.len().is_multiple_of(each) {
        return Err(Error::Invalid(format!(
            "S38BUF of {} bytes is not records of {each}",
            b.len()
        )));
    }
    Ok(b
        .chunks(each)
        .map(|c| Rec {
            data: c[..reclen].to_vec(),
            flags: u16::from_be_bytes([c[reclen], c[reclen + 1]]),
            rrn: u32::from_be_bytes([c[reclen + 2], c[reclen + 3], c[reclen + 4], c[reclen + 5]]),
            locked: c[reclen + 9] == 0x40,
            nulls: c[reclen + TRAILER..].iter().map(|&x| x == 0xF1).collect(),
        })
        .collect())
}

/// `S38BUF`.
pub fn buf(recs: &[Rec]) -> Object {
    Object::data(param::S38BUF, encode_buf(recs))
}

/// `S38CMD` with a command string.
pub fn cmd(command: &str) -> Object {
    let mut d = Vec::new();
    let e = crate::wire::ebcdic(command);
    d.extend(e);
    Object::coll(s38::S38CMD, vec![Object::data(param::S38CMDST, d)])
}

/// `S38CLOSE` of a declared name (close type 2: permanent).
pub fn close(name: &str) -> Object {
    Object::coll(
        s38::S38CLOSE,
        vec![dclnam(name), Object::byte(param::S38CLOST, 0x02)],
    )
}

/// `S38GET`, `S38GETK`, `S38GETM`, `S38UPDAT`, `S38FEOD`: a declared
/// name, an operation list and a control list when there is one.
pub fn op_cmd(cp: u16, name: &str, optl: Optl, ctll: &[Ctl]) -> Object {
    let mut v = vec![dclnam(name), optl.to_object()];
    if !ctll.is_empty() {
        v.push(encode_ctll(ctll));
    }
    Object::coll(cp, v)
}

/// `S38PUTM` or `S38DEL`: the declared name alone.
pub fn name_cmd(cp: u16, name: &str) -> Object {
    Object::coll(cp, vec![dclnam(name)])
}

/// A message of `S38MSGRM`: ID, text and severity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    /// `SVRCOD`.
    pub severity: u16,
    /// `S38MID` (`CPF5001`).
    pub id: String,
    /// `S38MTEXT`.
    pub text: String,
}

/// The message of an `S38MSGRM`.
pub fn message_of(o: &Object) -> Message {
    let t = o.get(param::S38MTEXT).map_or_else(String::new, |x| {
        let b = x.bytes();
        if b.len() >= 2 {
            text(&b[2..])
        } else {
            String::new()
        }
    });
    Message {
        severity: o.svrcod(),
        id: o.get_chars(param::S38MID).unwrap_or_default(),
        text: t,
    }
}

/// An `S38MSGRM` (an empty ID for a bare completion).
pub fn msgrm(m: &Message) -> Object {
    let mut v = vec![Object::u16(param::SVRCOD, m.severity)];
    if !m.id.is_empty() {
        v.push(Object::chars_fixed(param::S38MID, &m.id, 7));
        let t = crate::wire::ebcdic(&m.text);
        let mut d = u16::try_from(t.len()).unwrap_or(0).to_be_bytes().to_vec();
        d.extend(t);
        v.push(Object::data(param::S38MTEXT, d));
    }
    Object::coll(crate::cp::rm::S38MSGRM, v)
}

/// The `S38MSGRM` that ends the data: `CPF5001`, severity 4, message
/// type `F0`, in the order IBM i sends them.
pub fn eof_msgrm() -> Object {
    Object::coll(
        crate::cp::rm::S38MSGRM,
        vec![
            Object::chars_fixed(param::S38MID, "CPF5001", 7),
            Object::u16(param::SVRCOD, crate::cp::svrcod::WARNING),
            Object::chars(param::S38MTYPE, "F0"),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_list_round_trip() {
        let c = vec![
            Ctl::Format("QSDDMTR".into()),
            Ctl::Other(0x0F, vec![0, 0]),
            Ctl::KeyFields(1),
            Ctl::Key(vec![0xF0, 0xF2]),
        ];
        let o = encode_ctll(&c);
        assert_eq!(decode_ctll(o.bytes()).unwrap(), c);
    }

    #[test]
    fn buffer_round_trip() {
        let mut r = Rec::new(vec![1, 2, 3], 2);
        r.rrn = 7;
        r.flags = 1;
        r.nulls[1] = true;
        let b = encode_buf(&[r.clone(), r.clone()]);
        assert_eq!(decode_buf(&b, 3, 2).unwrap(), vec![r.clone(), r]);
    }
}
