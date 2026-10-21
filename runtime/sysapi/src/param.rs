// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Reading parameters and filling receivers: every API parameter is
//! a storage address, character fields are EBCDIC and blank padded,
//! binary fields are big-endian, and a receiver format begins with
//! "bytes returned" and "bytes available". The error code parameter
//! is `ERRC0100` (bytes provided, bytes available, message identifier,
//! data) or `ERRC0200` (a key of -1 first, then the same with the
//! data's CCSID, offset and length).

use mi::{Job, QueueRef};
use qdecimal::{Decimal, Packed, Rounding};

use crate::provider::ApiFail;

/// A blank-padded EBCDIC field.
#[must_use]
pub fn fixed(s: &str, n: usize) -> Vec<u8> {
    let mut b = ccsid::to_ebcdic(s).unwrap_or_default();
    b.resize(n, 0x40);
    b
}

/// A big-endian `i32` at an offset of a buffer, 0 when short.
#[must_use]
pub fn bin4_at(b: &[u8], off: usize) -> i32 {
    b.get(off..off + 4)
        .map_or(0, |x| i32::from_be_bytes(x.try_into().expect("4 bytes")))
}

/// The parameter addresses of one call.
#[derive(Clone, Copy)]
pub struct Args<'a>(pub &'a [u64]);

impl Args<'_> {
    /// How many were passed.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether none were passed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether parameter `i` was passed and is not `*OMIT`.
    #[must_use]
    pub fn present(&self, i: usize) -> bool {
        self.0.get(i).is_some_and(|a| *a != 0)
    }

    /// The address of parameter `i`, 0 when absent.
    #[must_use]
    pub fn addr(&self, i: usize) -> u64 {
        self.0.get(i).copied().unwrap_or(0)
    }

    /// `n` bytes of parameter `i`, or none when absent.
    #[must_use]
    pub fn raw(&self, i: usize, n: usize) -> Option<&'static mut [u8]> {
        self.present(i).then(|| mi::storage::view(self.addr(i), n))
    }

    /// A character parameter, trimmed; empty when absent.
    #[must_use]
    pub fn char(&self, i: usize, n: usize) -> String {
        self.raw(i, n)
            .map(|b| ccsid::decode_trimmed(b))
            .unwrap_or_default()
    }

    /// A character parameter, untrimmed bytes; empty when absent.
    #[must_use]
    pub fn bytes(&self, i: usize, n: usize) -> Vec<u8> {
        self.raw(i, n).map(|b| b.to_vec()).unwrap_or_default()
    }

    /// A binary(4) parameter, 0 when absent.
    #[must_use]
    pub fn bin4(&self, i: usize) -> i32 {
        self.raw(i, 4).map_or(0, |b| bin4_at(b, 0))
    }

    /// A binary(2) parameter, 0 when absent.
    #[must_use]
    pub fn bin2(&self, i: usize) -> i16 {
        self.raw(i, 2)
            .map_or(0, |b| i16::from_be_bytes([b[0], b[1]]))
    }

    /// A packed decimal parameter as an integer (scale dropped).
    #[must_use]
    pub fn packed(&self, i: usize, digits: u8, scale: u8) -> i64 {
        let Some(p) = Packed::new(digits, scale).ok() else {
            return 0;
        };
        let n = usize::from(digits) / 2 + 1;
        self.raw(i, n)
            .and_then(|b| p.decode(b).ok())
            .and_then(|d| d.rescale(0, Rounding::Truncate).ok())
            .and_then(|d| d.to_i128())
            .and_then(|v| i64::try_from(v).ok())
            .unwrap_or(0)
    }

    /// A qualified name parameter: `(name, library)` from 20 bytes.
    #[must_use]
    pub fn qual(&self, i: usize) -> (String, String) {
        match self.raw(i, 20) {
            Some(b) => (
                ccsid::decode_trimmed(&b[..10]),
                ccsid::decode_trimmed(&b[10..]),
            ),
            None => (String::new(), String::new()),
        }
    }

    /// Store a binary(4).
    pub fn put_bin4(&self, i: usize, v: i32) {
        if let Some(b) = self.raw(i, 4) {
            b.copy_from_slice(&v.to_be_bytes());
        }
    }

    /// Store a blank-padded character field.
    pub fn put_char(&self, i: usize, n: usize, s: &str) {
        if let Some(b) = self.raw(i, n) {
            b.copy_from_slice(&fixed(s, n));
        }
    }

    /// Store bytes.
    pub fn put_bytes(&self, i: usize, bytes: &[u8]) {
        if let Some(b) = self.raw(i, bytes.len()) {
            b.copy_from_slice(bytes);
        }
    }

    /// Store a packed decimal.
    pub fn put_packed(&self, i: usize, digits: u8, scale: u8, v: i64) {
        let Ok(p) = Packed::new(digits, scale) else {
            return;
        };
        let n = usize::from(digits) / 2 + 1;
        if let (Some(b), Ok(d)) = (self.raw(i, n), Decimal::new(i128::from(v), 0)) {
            let _ = d
                .rescale(scale, Rounding::Truncate)
                .and_then(|d| p.encode_into(&d, Rounding::Truncate, b));
        }
    }
}

/// A receiver variable being filled.
#[derive(Default)]
pub struct Out {
    /// The bytes built so far.
    pub buf: Vec<u8>,
}

impl Out {
    /// An empty receiver image.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Grow with blanks to `n` bytes.
    pub fn pad_to(&mut self, n: usize) {
        if self.buf.len() < n {
            self.buf.resize(n, 0x40);
        }
    }

    /// Grow with zeros to `n` bytes.
    pub fn zero_to(&mut self, n: usize) {
        if self.buf.len() < n {
            self.buf.resize(n, 0);
        }
    }

    /// Bytes at an offset.
    pub fn bytes(&mut self, off: usize, b: &[u8]) {
        self.zero_to(off + b.len());
        self.buf[off..off + b.len()].copy_from_slice(b);
    }

    /// A blank-padded character field.
    pub fn char(&mut self, off: usize, n: usize, s: &str) {
        self.bytes(off, &fixed(s, n));
    }

    /// Blanks.
    pub fn blank(&mut self, off: usize, n: usize) {
        self.bytes(off, &vec![0x40; n]);
    }

    /// A binary(4).
    pub fn bin4(&mut self, off: usize, v: i32) {
        self.bytes(off, &v.to_be_bytes());
    }

    /// A binary(2).
    pub fn bin2(&mut self, off: usize, v: i16) {
        self.bytes(off, &v.to_be_bytes());
    }

    /// One byte.
    pub fn u8(&mut self, off: usize, v: u8) {
        self.bytes(off, &[v]);
    }

    /// One character.
    pub fn ch(&mut self, off: usize, c: char) {
        self.u8(off, ccsid::byte_of(c).unwrap_or(0x40));
    }

    /// Length so far.
    #[must_use]
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// Whether nothing was written.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Deliver into the receiver at parameter `recv` of the length at
    /// parameter `len`, with bytes returned and bytes available in
    /// the first eight bytes.
    pub fn deliver(mut self, a: &Args<'_>, recv: usize, len: usize) {
        let room = usize::try_from(a.bin4(len)).unwrap_or(0);
        let avail = i32::try_from(self.buf.len()).unwrap_or(i32::MAX);
        let ret = i32::try_from(room.min(self.buf.len())).unwrap_or(i32::MAX);
        self.zero_to(8);
        self.buf[0..4].copy_from_slice(&ret.to_be_bytes());
        self.buf[4..8].copy_from_slice(&avail.to_be_bytes());
        self.deliver_raw(a, recv, room);
    }

    /// Deliver into the receiver at parameter `recv` with `room`
    /// bytes, no header.
    pub fn deliver_raw(self, a: &Args<'_>, recv: usize, room: usize) {
        let n = room.min(self.buf.len());
        if n > 0 {
            a.put_bytes(recv, &self.buf[..n]);
        }
    }
}

/// The error code parameter.
#[derive(Clone, Copy)]
pub struct ErrCode {
    addr: u64,
}

impl ErrCode {
    /// No error code parameter: failures are escapes.
    #[must_use]
    pub fn none() -> Self {
        Self { addr: 0 }
    }

    /// Parameter `i` of a call.
    #[must_use]
    pub fn at(a: &Args<'_>, i: usize) -> Self {
        Self { addr: a.addr(i) }
    }

    /// Whether the caller gave one with room for an error.
    #[must_use]
    pub fn provided(&self) -> usize {
        if self.addr == 0 {
            return 0;
        }
        let head = mi::storage::view(self.addr, 8);
        if bin4_at(head, 0) == -1 {
            usize::try_from(bin4_at(head, 4)).unwrap_or(0)
        } else {
            usize::try_from(bin4_at(head, 0)).unwrap_or(0)
        }
    }

    /// Success: bytes available 0.
    pub fn ok(&self) {
        if self.addr == 0 {
            return;
        }
        let head = mi::storage::view(self.addr, 12);
        if bin4_at(head, 0) == -1 {
            head[8..12].copy_from_slice(&0i32.to_be_bytes());
        } else {
            head[4..8].copy_from_slice(&0i32.to_be_bytes());
        }
    }

    /// Failure: into the error code when it has room, else an escape
    /// message to the caller.
    pub fn fail(&self, j: &mut Job, f: &ApiFail) {
        let provided = self.provided();
        if provided == 0 {
            let _ = j.send_message(
                &f.msgid,
                file_of(&f.msgid),
                "QSYS",
                f.data.clone(),
                qir::MsgType::Escape,
                QueueRef::Same,
            );
            return;
        }
        let mut out = Out::new();
        let head = mi::storage::view(self.addr, 4);
        if bin4_at(head, 0) == -1 {
            out.bin4(0, -1);
            out.bin4(4, i32::try_from(provided).unwrap_or(i32::MAX));
            out.bin4(8, i32::try_from(32 + f.data.len()).unwrap_or(i32::MAX));
            out.char(12, 7, &f.msgid);
            out.blank(19, 1);
            out.bin4(20, 37);
            out.bin4(24, 32);
            out.bin4(28, i32::try_from(f.data.len()).unwrap_or(i32::MAX));
            out.bytes(32, &f.data);
        } else {
            out.bin4(0, i32::try_from(provided).unwrap_or(i32::MAX));
            out.bin4(4, i32::try_from(16 + f.data.len()).unwrap_or(i32::MAX));
            out.char(8, 7, &f.msgid);
            out.blank(15, 1);
            out.bytes(16, &f.data);
        }
        let n = provided.min(out.buf.len());
        mi::storage::view(self.addr, n).copy_from_slice(&out.buf[..n]);
    }
}

/// The message file of an identifier's family.
#[must_use]
pub fn file_of(msgid: &str) -> &'static str {
    match msgid.get(..3) {
        Some("CEE") => "QCEEMSG",
        Some("SQL") => "QSQLMSG",
        _ => "QCPFMSG",
    }
}

/// The queue of a call-stack entry parameter: `*` is the caller of the
/// API, `*CTLBDY` the nearest control boundary, `*EXT` the external
/// queue, a name the nearest entry whose program or procedure has it;
/// `counter` entries further up. `None` when there is no such entry.
#[must_use]
pub fn entry_queue(j: &Job, entry: &str, counter: i32) -> Option<QueueRef> {
    let top = j.act.frames.len();
    if top == 0 {
        return (entry == "*EXT").then_some(QueueRef::Ext);
    }
    let base = match entry {
        "*" | "*SAME" | "" => top - 1,
        "*EXT" => return Some(QueueRef::Ext),
        "*CTLBDY" => j.act.frames.iter().rposition(|f| f.control_boundary)?,
        "*PGMBDY" => {
            let pgm = j.act.frames[top - 1]
                .program
                .as_ref()
                .map(|p| p.key.clone());
            j.act
                .frames
                .iter()
                .rposition(|f| f.program.as_ref().map(|p| p.key.clone()) != pgm)
                .map_or(0, |i| i + 1)
        }
        name => j.act.frames.iter().rposition(|f| {
            f.program.as_ref().is_some_and(|p| {
                p.object.name == name || p.proc_name(f.code).as_deref() == Some(name)
            })
        })?,
    };
    let up = usize::try_from(counter).ok()?;
    if up > base {
        return None;
    }
    let target = base - up;
    Some(QueueRef::Up(u32::try_from(top - 1 - target).ok()?))
}

/// The code `QMHRCVPM` reports for a message type.
#[must_use]
pub fn type_code(t: qir::MsgType) -> &'static str {
    match t {
        qir::MsgType::Comp => "01",
        qir::MsgType::Diag => "02",
        qir::MsgType::Info => "04",
        qir::MsgType::Inq => "05",
        qir::MsgType::Rqs => "08",
        qir::MsgType::Notify => "14",
        qir::MsgType::Escape => "15",
        qir::MsgType::Rpy => "21",
        _ => "00",
    }
}

/// The message type an API name or code means.
#[must_use]
pub fn type_named(s: &str) -> Option<qir::MsgType> {
    match s.trim() {
        "*ANY" => Some(qir::MsgType::Any),
        "*COMP" | "01" => Some(qir::MsgType::Comp),
        "*DIAG" | "02" => Some(qir::MsgType::Diag),
        "*INFO" | "04" => Some(qir::MsgType::Info),
        "*INQ" | "05" => Some(qir::MsgType::Inq),
        "*RQS" | "08" => Some(qir::MsgType::Rqs),
        "*NOTIFY" | "14" | "16" => Some(qir::MsgType::Notify),
        "*ESCAPE" | "15" | "17" => Some(qir::MsgType::Escape),
        "*STATUS" => Some(qir::MsgType::Status),
        "*RPY" | "21" | "22" | "23" | "24" | "25" => Some(qir::MsgType::Rpy),
        "*SCOPE" => Some(qir::MsgType::Scope),
        _ => None,
    }
}
