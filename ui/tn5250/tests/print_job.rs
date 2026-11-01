// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A one-page `QSYSPRT` file (132 by 66, as the printer file writes it)
//! sent as the writer sends it, into a printer client stub that frames
//! what it receives as the telnet connection would carry it. Every
//! record header is checked byte for byte against RFC 2877, section 10,
//! and against the captured client's replies.

use tn5250::print::{Device, Failure, deliver};

/// The writer's record size (`work/qsjob/src/work/writer.rs`).
const CHUNK: usize = 4000;

/// A client stub: what went on the wire, and its answer to each record.
struct Client {
    wire: Vec<Vec<u8>>,
    answer: Box<dyn FnMut(usize) -> Option<Vec<u8>>>,
}

impl Device for Client {
    fn send(&mut self, record: Vec<u8>) -> Result<(), String> {
        self.wire.push(tn5250::telnet::record(&record));
        Ok(())
    }

    fn recv(&mut self) -> Result<Vec<u8>, String> {
        let n = self.wire.len();
        (self.answer)(n).ok_or_else(|| "the printer session went away".into())
    }
}

/// Print complete, good (RFC 2877, Figure 5).
const COMPLETE: [u8; 10] = [0x00, 0x0A, 0x12, 0xA0, 0x01, 0x02, 0x04, 0x00, 0x00, 0x01];

/// The page: 66 lines of a 132-column report, the last one a form feed.
fn page() -> Vec<u8> {
    let mut p = spool::printer::Printer::new(spool::printer::PageFormat::default());
    for i in 0..60 {
        p.write_str(&format!("LINE {i:03} {}", "X".repeat(100)), spool::printer::Control::line());
    }
    p.take()
}

/// The record without `IAC EOR` and with doubled `IAC`s undone.
fn unframe(wire: &[u8]) -> Vec<u8> {
    assert_eq!(&wire[wire.len() - 2..], &[0xFF, 0xEF], "each record ends IAC EOR");
    let body = &wire[..wire.len() - 2];
    let mut out = Vec::with_capacity(body.len());
    let mut i = 0;
    while i < body.len() {
        out.push(body[i]);
        i += if body[i] == 0xFF { 2 } else { 1 };
    }
    out
}

#[test]
fn a_one_page_file_is_one_print_job() {
    let scs = page();
    assert!(scs.len() > CHUNK, "the page needs more than one record");
    let mut c = Client {
        wire: Vec::new(),
        answer: Box::new(|_| Some(COMPLETE.to_vec())),
    };
    let mut sent = 0;
    deliver(&mut c, &scs, CHUNK, &mut sent).unwrap();
    let records: Vec<Vec<u8>> = c.wire.iter().map(|w| unframe(w)).collect();
    assert_eq!(sent, records.len());
    let mut data = Vec::new();
    for (i, r) in records.iter().enumerate() {
        let last = i + 1 == records.len();
        let len = u16::from_be_bytes([r[0], r[1]]) as usize;
        assert_eq!(len, r.len(), "record {i}: length");
        assert_eq!(&r[2..4], &[0x12, 0xA0], "record {i}: GDS");
        assert_eq!(&r[4..6], &[0x01, 0x01], "record {i}: print record, server to client");
        assert_eq!(r[6], 0x0A, "record {i}: header length");
        let chain = match (i, last) {
            (_, true) => 0x08,
            (0, _) => 0x10,
            _ => 0x00,
        };
        assert_eq!(r[7], chain, "record {i}: chain flags");
        assert_eq!(r[8], 0x00, "record {i}: reserved");
        assert_eq!(r[9], 0x01, "record {i}: print");
        assert_eq!(&r[10..16], &[0; 6], "record {i}: zero pad");
        if last {
            assert_eq!(r.len(), 17, "the null print record");
            assert_eq!(r[16], 0x00);
        } else {
            data.extend_from_slice(&r[16..]);
        }
    }
    assert_eq!(data, scs, "the data records carry the file whole");
}

#[test]
fn the_captured_refusal_stops_at_the_first_record() {
    let mut c = Client {
        wire: Vec::new(),
        answer: Box::new(|_| {
            Some(vec![0x00, 0x0E, 0x12, 0xA0, 0x01, 0x02, 0x08, 0x80, 0x00, 0x00, 0x08, 0x11, 0x02, 0x28])
        }),
    };
    let mut sent = 0;
    let e = deliver(&mut c, &page(), CHUNK, &mut sent).unwrap_err();
    assert_eq!(c.wire.len(), 1);
    assert_eq!(sent, 0);
    assert!(matches!(e, Failure::Refused(_)));
    assert_eq!(e.diag_hex(), "08110228");
}

#[test]
fn a_disconnect_mid_file_is_not_printed() {
    let mut c = Client {
        wire: Vec::new(),
        answer: Box::new(|n| (n < 2).then(|| COMPLETE.to_vec())),
    };
    let mut sent = 0;
    let e = deliver(&mut c, &page(), CHUNK, &mut sent).unwrap_err();
    assert!(matches!(e, Failure::Lost(_)));
    assert_eq!(sent, 1);
}
