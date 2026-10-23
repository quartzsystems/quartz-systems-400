// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Random bytes against every parser: nothing panics. A session job
//! that fed a hostile client's records into these must stay up.

use tn5250::ds::parse::parse;
use tn5250::ds::wdsf::{Capabilities, Model};
use tn5250::record::Record;
use tn5250::screen::{Read, ScreenModel};
use tn5250::telnet::Parser;
use tn5250::telnet::client::{Client, Config};
use tn5250::telnet::server::Server;
use tn5250::term::{Key, Terminal};

/// A small deterministic generator, so a failure reproduces.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    fn byte(&mut self) -> u8 {
        (self.next() & 0xFF) as u8
    }

    fn bytes(&mut self, max: usize) -> Vec<u8> {
        let n = (self.next() as usize) % (max + 1);
        (0..n).map(|_| self.byte()).collect()
    }

    /// Bytes biased toward the codes the parsers act on.
    fn stream(&mut self, max: usize) -> Vec<u8> {
        let n = (self.next() as usize) % (max + 1);
        (0..n)
            .map(|_| match self.next() % 8 {
                0 => 0x04,
                1 => [0x11, 0x40, 0x52, 0x1D, 0x01, 0x02, 0x03, 0x10, 0x13, 0x14, 0x23, 0xF3][(self.next() % 12) as usize],
                2 => (self.next() % 28) as u8,
                3 => 0x20 + (self.next() % 32) as u8,
                _ => self.byte(),
            })
            .collect()
    }
}

#[test]
fn outbound_parser_never_panics() {
    let mut g = Lcg(1);
    for _ in 0..20_000 {
        let data = g.stream(200);
        let _ = parse(&data);
    }
}

#[test]
fn terminal_survives_random_records_and_keys() {
    let mut g = Lcg(2);
    let mut t = Terminal::new(Capabilities {
        model: Model::M3477Fc,
        gui: true,
        color: true,
    });
    for _ in 0..5_000 {
        let rec = Record {
            flags: (g.next() & 0xFFFF) as u16,
            opcode: g.byte() % 14,
            data: g.stream(300),
        };
        let _ = t.apply(&rec);
        for _ in 0..(g.next() % 6) {
            let key = match g.next() % 12 {
                0 => Key::Enter,
                1 => Key::F((g.next() % 24) as u8 + 1),
                2 => Key::Tab,
                3 => Key::FieldExit,
                4 => Key::EraseInput,
                5 => Key::Dup,
                6 => Key::Reset,
                7 => Key::PageDown,
                8 => Key::Insert,
                9 => Key::Delete,
                10 => Key::NewLine,
                _ => Key::Char((b'A' + g.byte() % 26) as char),
            };
            let _ = t.key(key);
        }
        let _ = t.take_outbox();
        let _ = t.text_rows();
        let _ = t.attr_rows();
    }
}

#[test]
fn screen_model_accepts_random_inbound() {
    let mut g = Lcg(3);
    let mut m = ScreenModel::new(false);
    m.put(1, 2, "Title", 0x22);
    m.field(5, 10, 20, 0x24, "", Default::default(), &[]);
    m.field(7, 1, 300, 0x24, "", Default::default(), &[]);
    let _ = m.render(Read::MdtFields);
    for _ in 0..20_000 {
        let data = g.bytes(120);
        let _ = m.accept(&data);
        let _ = m.render(Read::MdtFields);
    }
}

#[test]
fn record_decoder_never_panics() {
    let mut g = Lcg(4);
    for _ in 0..50_000 {
        let _ = Record::decode(&g.bytes(40));
    }
}

#[test]
fn negotiators_take_garbage() {
    let mut g = Lcg(5);
    for round in 0..300 {
        let mut s = Server::new([round as u8; 8]);
        let mut c = Client::new(Config::default());
        let mut p = Parser::new();
        for _ in 0..20 {
            let bytes = g.bytes(64);
            let _ = s.feed(&bytes);
            let _ = c.feed(&bytes);
            let _ = p.feed(&bytes);
        }
    }
}
