// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Job log messages returned to the client, in the extended layout the
//! remote command server (`0x1106`) and the sign-on server (`0x112B`)
//! share: two CCSIDs, the severity, then length-prefixed (u32) type,
//! message ID, message file, library, text, substitution data and help.

use crate::Error;
use crate::frame::header::{be16, be32};

/// One message.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct HostMessage {
    /// CCSID of the text fields.
    pub text_ccsid: u32,
    /// CCSID of the substitution data.
    pub substitution_ccsid: u32,
    /// Severity.
    pub severity: u16,
    /// Message type, two EBCDIC digits (`02` diagnostic, `15` escape, ...).
    pub kind: Vec<u8>,
    /// Message ID (EBCDIC).
    pub id: Vec<u8>,
    /// Message file (EBCDIC, 10).
    pub file: Vec<u8>,
    /// Message file library (EBCDIC, 10).
    pub library: Vec<u8>,
    /// First level text.
    pub text: Vec<u8>,
    /// Substitution data.
    pub substitution: Vec<u8>,
    /// Second level (help) text.
    pub help: Vec<u8>,
    /// Bytes after the help text, if any.
    pub rest: Vec<u8>,
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn need(&self, n: usize) -> Result<(), Error> {
        if self.at + n > self.b.len() {
            return Err(Error::Short {
                what: "message",
                need: self.at + n,
                have: self.b.len(),
            });
        }
        Ok(())
    }
    fn u16(&mut self) -> Result<u16, Error> {
        self.need(2)?;
        let v = be16(self.b, self.at);
        self.at += 2;
        Ok(v)
    }
    fn u32(&mut self) -> Result<u32, Error> {
        self.need(4)?;
        let v = be32(self.b, self.at);
        self.at += 4;
        Ok(v)
    }
    fn field(&mut self) -> Result<Vec<u8>, Error> {
        let n = self.u32()? as usize;
        self.need(n)?;
        let v = self.b[self.at..self.at + n].to_vec();
        self.at += n;
        Ok(v)
    }
}

fn put(out: &mut Vec<u8>, f: &[u8]) {
    out.extend_from_slice(&u32::try_from(f.len()).unwrap_or(u32::MAX).to_be_bytes());
    out.extend_from_slice(f);
}

impl HostMessage {
    /// Decode a parameter's data.
    pub fn decode(b: &[u8]) -> Result<Self, Error> {
        let mut r = Reader { b, at: 0 };
        let mut m = Self {
            text_ccsid: r.u32()?,
            substitution_ccsid: r.u32()?,
            severity: r.u16()?,
            ..Self::default()
        };
        m.kind = r.field()?;
        m.id = r.field()?;
        m.file = r.field()?;
        m.library = r.field()?;
        m.text = r.field()?;
        m.substitution = r.field()?;
        m.help = r.field()?;
        m.rest = b[r.at..].to_vec();
        Ok(m)
    }

    /// The parameter data.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.text_ccsid.to_be_bytes());
        out.extend_from_slice(&self.substitution_ccsid.to_be_bytes());
        out.extend_from_slice(&self.severity.to_be_bytes());
        for f in [
            &self.kind,
            &self.id,
            &self.file,
            &self.library,
            &self.text,
            &self.substitution,
            &self.help,
        ] {
            put(&mut out, f);
        }
        out.extend_from_slice(&self.rest);
        out
    }

    /// The message ID as text.
    pub fn id_text(&self) -> String {
        ccsid::from_ebcdic(&self.id)
    }

    /// The message type as a number (`02`, `15`, ...).
    pub fn type_number(&self) -> u8 {
        self.kind
            .iter()
            .fold(0u8, |n, &d| n.wrapping_mul(10).wrapping_add(d & 0x0F))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let m = HostMessage {
            text_ccsid: 37,
            substitution_ccsid: 37,
            severity: 40,
            kind: vec![0xF1, 0xF5],
            id: ccsid::to_ebcdic("CPF9801").unwrap(),
            file: ccsid::to_ebcdic("QCPFMSG   ").unwrap(),
            library: ccsid::to_ebcdic("QSYS      ").unwrap(),
            text: ccsid::to_ebcdic("t").unwrap(),
            ..HostMessage::default()
        };
        let b = m.encode();
        let d = HostMessage::decode(&b).unwrap();
        assert_eq!(d, m);
        assert_eq!(d.id_text(), "CPF9801");
        assert_eq!(d.type_number(), 15);
    }
}
