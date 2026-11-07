// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Printer records (RFC 2877, section 10): the host's print records,
//! the null print record that ends a print job, the client's responses,
//! and `deliver`, which sends one file a record at a time and takes it
//! as printed only when the client has answered every record, the null
//! one last, without error.

use std::fmt::Write as _;

use crate::record::GDS;

/// Data flow type of a print record (server to client).
pub const FLOW_PRINT: u16 = 0x0101;
/// Data flow type of a response record (client to server).
pub const FLOW_RESPONSE: u16 = 0x0102;
/// The length of a print record's pass-through header (bytes 6 to 15).
pub const PRINT_LL: u8 = 10;
/// The length of a whole null print record.
pub const NULL_LEN: usize = 17;

/// Flag bits of byte 7.
pub mod flags {
    /// Error indicator.
    pub const ERROR: u8 = 0x80;
    /// Intervention required.
    pub const INTERVENTION: u8 = 0x40;
    /// Printer now ready.
    pub const READY: u8 = 0x20;
    /// First of chain.
    pub const FIRST: u8 = 0x10;
    /// Last of chain.
    pub const LAST: u8 = 0x08;
}

/// Printer operation codes (byte 9).
pub mod opcode {
    /// Print (host), print complete (client).
    pub const PRINT: u8 = 0x01;
    /// Clear print buffers.
    pub const CLEAR: u8 = 0x02;
}

/// A print record carrying `data` with chain flags `chain`.
#[must_use]
pub fn record(chain: u8, data: &[u8]) -> Vec<u8> {
    let len = 6 + usize::from(PRINT_LL) + data.len();
    let mut out = Vec::with_capacity(len);
    out.extend((len as u16).to_be_bytes());
    out.extend(GDS.to_be_bytes());
    out.extend(FLOW_PRINT.to_be_bytes());
    out.push(PRINT_LL);
    out.push(chain);
    out.push(0);
    out.push(opcode::PRINT);
    out.extend([0u8; 6]);
    out.extend_from_slice(data);
    out
}

/// The null print record: last of chain, one null byte, the end of
/// the print job.
#[must_use]
pub fn null_record() -> Vec<u8> {
    record(flags::LAST, &[0])
}

/// The records of one print job: `scs` in pieces of at most `chunk`
/// bytes, the first marked first of chain, then the null record.
#[must_use]
pub fn job_records(scs: &[u8], chunk: usize) -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = scs
        .chunks(chunk.max(1))
        .enumerate()
        .map(|(i, c)| record(if i == 0 { flags::FIRST } else { 0 }, c))
        .collect();
    out.push(null_record());
    out
}

/// A client's response record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    /// The data flow type.
    pub flow: u16,
    /// Byte 7.
    pub flags: u8,
    /// The operation code.
    pub opcode: u8,
    /// The diagnostic bytes after the operation code.
    pub diag: Vec<u8>,
}

impl Response {
    /// The response in `bytes` (one record, without `IAC EOR`).
    pub fn decode(bytes: &[u8]) -> Result<Self, crate::Error> {
        if bytes.len() < 10 {
            return Err(crate::Error::Truncated { at: bytes.len() });
        }
        let kind = u16::from_be_bytes([bytes[2], bytes[3]]);
        if kind != GDS {
            return Err(crate::Error::RecordType(kind));
        }
        let ll = usize::from(bytes[6]);
        if ll < 4 || bytes.len() < 6 + ll {
            return Err(crate::Error::Truncated { at: bytes.len() });
        }
        Ok(Self {
            flow: u16::from_be_bytes([bytes[4], bytes[5]]),
            flags: bytes[7],
            opcode: bytes[9],
            diag: bytes[10..6 + ll].to_vec(),
        })
    }

    /// A good print complete: no error or intervention flag.
    #[must_use]
    pub fn is_good(&self) -> bool {
        self.flags & (flags::ERROR | flags::INTERVENTION) == 0 && self.opcode == opcode::PRINT
    }

    /// What the diagnostic bytes mean, for the operator.
    #[must_use]
    pub fn reason(&self) -> &'static str {
        match self.diag.as_slice() {
            [0x08, 0x11, 0x02, 0x00, ..] => "the print job was cancelled",
            [0x08, 0x11, 0x02, 0x28, ..] => "the print command is not valid",
            [0x08, 0x11, 0x02, 0x29, ..] => "a print parameter is not valid",
            [0xC9, 0x00, 0x03, 0x02, ..] => "the printer is not ready",
            _ if self.flags & flags::INTERVENTION != 0 => "the printer needs attention",
            _ if self.flags & flags::ERROR == 0 => "the response is not a print complete",
            _ => "the printer reported an error",
        }
    }
}

/// Why a file was not printed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// The client answered with an error: its response.
    Refused(Response),
    /// The session went away or never answered.
    Lost(String),
}

impl Failure {
    /// The diagnostic bytes as hex, empty when lost.
    #[must_use]
    pub fn diag_hex(&self) -> String {
        match self {
            Self::Refused(r) => r.diag.iter().fold(String::new(), |mut s, b| {
                let _ = write!(s, "{b:02X}");
                s
            }),
            Self::Lost(_) => String::new(),
        }
    }

    /// The text for the operator.
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            Self::Refused(r) if r.diag.is_empty() => r.reason().to_string(),
            Self::Refused(r) => format!("{} ({})", r.reason(), self.diag_hex()),
            Self::Lost(e) => e.clone(),
        }
    }
}

/// The printer session as `deliver` sees it.
pub trait Device {
    /// Send one record.
    fn send(&mut self, record: Vec<u8>) -> Result<(), String>;
    /// The next record from the client.
    fn recv(&mut self) -> Result<Vec<u8>, String>;
}

/// Send `scs` as one print job, each record answered before the next.
/// `Ok` only when the null record, the last, is answered good. The
/// number of records answered is in `sent`.
pub fn deliver(
    dev: &mut dyn Device,
    scs: &[u8],
    chunk: usize,
    sent: &mut usize,
) -> Result<(), Failure> {
    for rec in job_records(scs, chunk) {
        dev.send(rec).map_err(Failure::Lost)?;
        let r = loop {
            let bytes = dev.recv().map_err(Failure::Lost)?;
            match Response::decode(&bytes) {
                // Only response records answer a print record.
                Ok(r) if r.flow == FLOW_RESPONSE => break r,
                _ => {}
            }
        };
        if !r.is_good() {
            return Err(Failure::Refused(r));
        }
        *sent += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A client that answers each record with `answer(n)`.
    struct Stub {
        sent: Vec<Vec<u8>>,
        answer: fn(usize) -> Option<Vec<u8>>,
    }

    impl Device for Stub {
        fn send(&mut self, record: Vec<u8>) -> Result<(), String> {
            self.sent.push(record);
            Ok(())
        }

        fn recv(&mut self) -> Result<Vec<u8>, String> {
            (self.answer)(self.sent.len()).ok_or_else(|| "the printer session went away".into())
        }
    }

    const GOOD: [u8; 10] = [0x00, 0x0A, 0x12, 0xA0, 0x01, 0x02, 0x04, 0x00, 0x00, 0x01];
    /// The capture's error reply, without `IAC EOR`.
    const REFUSED: [u8; 14] = [
        0x00, 0x0E, 0x12, 0xA0, 0x01, 0x02, 0x08, 0x80, 0x00, 0x00, 0x08, 0x11, 0x02, 0x28,
    ];

    #[test]
    fn print_record_header_is_rfc_2877() {
        let r = record(flags::FIRST | flags::LAST, &[0x34, 0xC4, 0x01]);
        assert_eq!(
            &r[..16],
            &[
                0x00, 0x13, 0x12, 0xA0, 0x01, 0x01, 0x0A, 0x18, 0x00, 0x01, 0, 0, 0, 0, 0, 0
            ]
        );
        assert_eq!(&r[16..], &[0x34, 0xC4, 0x01]);
    }

    #[test]
    fn null_record_is_17_bytes() {
        let n = null_record();
        assert_eq!(n.len(), NULL_LEN);
        assert_eq!(
            n,
            vec![
                0x00, 0x11, 0x12, 0xA0, 0x01, 0x01, 0x0A, 0x08, 0x00, 0x01, 0, 0, 0, 0, 0, 0, 0x00
            ]
        );
    }

    #[test]
    fn chain_flags() {
        let recs = job_records(&[0x40; 9000], 4000);
        let fl: Vec<u8> = recs.iter().map(|r| r[7]).collect();
        assert_eq!(fl, vec![flags::FIRST, 0, 0, flags::LAST]);
        assert_eq!(recs[2].len(), 16 + 1000);
    }

    #[test]
    fn the_captured_error_reply() {
        let r = Response::decode(&REFUSED).unwrap();
        assert_eq!(r.flow, FLOW_RESPONSE);
        assert_eq!(r.flags, flags::ERROR);
        assert_eq!(r.diag, vec![0x08, 0x11, 0x02, 0x28]);
        assert!(!r.is_good());
        assert_eq!(r.reason(), "the print command is not valid");
        let good = Response::decode(&GOOD).unwrap();
        assert!(good.is_good());
        assert!(good.diag.is_empty());
    }

    #[test]
    fn a_good_file_ends_with_the_null_record() {
        let mut s = Stub {
            sent: Vec::new(),
            answer: |_| Some(GOOD.to_vec()),
        };
        let mut n = 0;
        assert_eq!(deliver(&mut s, &[0x40; 100], 4000, &mut n), Ok(()));
        assert_eq!(n, 2);
        assert_eq!(s.sent.len(), 2);
        assert_eq!(
            &s.sent[0][..10],
            &[0x00, 0x74, 0x12, 0xA0, 0x01, 0x01, 0x0A, 0x10, 0x00, 0x01]
        );
        assert_eq!(s.sent[1], null_record());
    }

    #[test]
    fn an_error_reply_stops_the_file() {
        let mut s = Stub {
            sent: Vec::new(),
            answer: |_| Some(REFUSED.to_vec()),
        };
        let mut n = 0;
        let e = deliver(&mut s, &[0x40; 9000], 4000, &mut n).unwrap_err();
        assert_eq!(s.sent.len(), 1);
        assert_eq!(n, 0);
        assert_eq!(e.diag_hex(), "08110228");
        assert!(e.text().contains("not valid"));
    }

    #[test]
    fn a_lost_session_stops_the_file() {
        let mut s = Stub {
            sent: Vec::new(),
            answer: |n| (n < 2).then(|| GOOD.to_vec()),
        };
        let mut n = 0;
        let e = deliver(&mut s, &[0x40; 9000], 4000, &mut n).unwrap_err();
        assert!(matches!(e, Failure::Lost(_)));
        assert_eq!(n, 1);
    }

    #[test]
    fn a_lost_null_record_answer_is_not_printed() {
        let mut s = Stub {
            sent: Vec::new(),
            answer: |n| (n < 2).then(|| GOOD.to_vec()),
        };
        let mut n = 0;
        assert!(deliver(&mut s, &[0x40; 10], 4000, &mut n).is_err());
        assert_eq!(s.sent.last().unwrap(), &null_record());
    }
}
