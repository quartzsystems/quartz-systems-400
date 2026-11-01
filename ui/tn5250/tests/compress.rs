// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The compressed output against the plain one: both forms of the same
//! screens, first renders and updates, decode to the same buffer, the
//! same format table and the same cursor.

use tn5250::ds::field::Ffw;
use tn5250::ds::parse::{Command, Order, parse};
use tn5250::ds::wdsf::{Capabilities, Model};
use tn5250::ds::{NULL, attr};
use tn5250::record::Record;
use tn5250::screen::{Encoding, Read, ScreenModel};
use tn5250::term::Terminal;

fn terminal(wide: bool) -> Terminal {
    Terminal::new(Capabilities {
        model: if wide { Model::M3477Fc } else { Model::M3179 },
        gui: false,
        color: true,
    })
}

/// Two models and two terminals kept in step: one compressed, one plain.
struct Pair {
    zip: (ScreenModel, Terminal),
    plain: (ScreenModel, Terminal),
    /// The last two records: compressed, plain.
    last: (Vec<u8>, Vec<u8>),
}

impl Pair {
    fn new(m: ScreenModel) -> Self {
        let wide = m.cols() == 132;
        Self {
            zip: (m.clone(), terminal(wide)),
            plain: (m, terminal(wide)),
            last: (Vec::new(), Vec::new()),
        }
    }

    /// The same change to both models.
    fn change(&mut self, f: impl Fn(&mut ScreenModel)) {
        f(&mut self.zip.0);
        f(&mut self.plain.0);
    }

    /// Render both, apply each to its terminal and compare.
    fn render(&mut self) {
        let zipped = self
            .zip
            .0
            .render_with(Read::MdtFields, Encoding::Compressed);
        let plain = self.plain.0.render_with(Read::MdtFields, Encoding::Plain);
        self.zip
            .1
            .apply(&Record::new(zipped.opcode, zipped.data.clone()))
            .unwrap();
        self.plain
            .1
            .apply(&Record::new(plain.opcode, plain.data.clone()))
            .unwrap();
        let (z, p) = (&self.zip.1, &self.plain.1);
        if let Some(i) = (0..z.cells().len()).find(|&i| z.cells()[i] != p.cells()[i]) {
            let w = i.saturating_sub(10)..(i + 10).min(z.cells().len());
            panic!(
                "cell {i}\ncompressed {:02x?}\nplain      {:02x?}\n{:02x?}\n{:02x?}",
                &z.cells()[w.clone()],
                &p.cells()[w],
                zipped.data,
                plain.data
            );
        }
        assert_eq!(z.cells(), self.zip.0.cells(), "the model's cells");
        assert_eq!(z.fields(), p.fields());
        assert_eq!(z.cursor(), p.cursor());
        assert_eq!(z.key_switches(), p.key_switches());
        assert_eq!(z.is_locked(), p.is_locked());
        assert!(
            zipped.data.len() <= plain.data.len(),
            "compressed {} > plain {}\n{:02x?}\n{:02x?}",
            zipped.data.len(),
            plain.data.len(),
            zipped.data,
            plain.data
        );
        self.last = (zipped.data, plain.data);
    }

    /// The orders of the last compressed record.
    fn orders(&self) -> Vec<Order> {
        parse(&self.last.0)
            .unwrap()
            .into_iter()
            .flat_map(|c| match c {
                Command::Wtd { orders, .. } => orders,
                _ => Vec::new(),
            })
            .collect()
    }

    fn ras(&self) -> Vec<(u8, u8, u8)> {
        self.orders()
            .into_iter()
            .filter_map(|o| match o {
                Order::Ra(r, c, b) => Some((r, c, b)),
                _ => None,
            })
            .collect()
    }
}

fn ebcdic(s: &str) -> Vec<u8> {
    ccsid::to_ebcdic(s).unwrap()
}

#[test]
fn a_full_24_by_80_screen() {
    let mut m = ScreenModel::new(false);
    for r in 1..=24 {
        let line: String = (0..79)
            .map(|c| char::from(b'A' + u8::try_from((r * 7 + c) % 26).unwrap()))
            .collect();
        m.put(
            r,
            2,
            &line,
            if r % 2 == 0 { attr::WHITE } else { attr::GREEN },
        );
    }
    let mut p = Pair::new(m);
    p.render();
    // Every cell differs from a cleared screen: nothing to skip.
    p.change(|m| m.put(12, 10, "changed", attr::RED));
    p.render();
    assert!(p.last.0.len() < 30, "{}", p.last.0.len());
}

#[test]
fn a_27_by_132_screen() {
    let mut m = ScreenModel::new(true);
    m.put(1, 60, "Wide Screen", attr::WHITE);
    m.put(14, 2, &"=".repeat(130), attr::BLUE);
    m.field(20, 40, 50, attr::GREEN_UL, "value", Ffw::default(), &[]);
    m.put(27, 2, "F3=Exit", attr::BLUE);
    m.put_bytes(26, 120, &[0x40; 13]);
    let mut p = Pair::new(m);
    p.render();
    assert!(p.last.0.starts_with(&[0x04, 0x20, 0x00]));
    assert!(p.ras().contains(&(14, 131, 0x7E)), "{:?}", p.ras());
    p.change(|m| m.put(14, 2, &"-".repeat(130), attr::BLUE));
    p.render();
}

#[test]
fn fields_with_attributes() {
    let mut m = ScreenModel::new(false);
    let bypass = Ffw {
        bypass: true,
        ..Ffw::default()
    };
    m.put(3, 2, "User", attr::GREEN);
    m.field(3, 20, 10, attr::GREEN_UL, "QSECOFR", Ffw::default(), &[]);
    m.field(4, 20, 10, attr::NONDISPLAY, "", Ffw::default(), &[]);
    m.field(
        5,
        20,
        10,
        attr::WHITE_RI,
        "xxxxxxxxxx",
        Ffw::default(),
        &[0x8101],
    );
    m.field(6, 20, 5, attr::RED_UL, "out", bypass, &[]);
    m.field(7, 1, 160, attr::TURQUOISE_UL_CS, "", Ffw::default(), &[]);
    m.field(10, 75, 12, attr::PINK_UL, "wraps", Ffw::default(), &[]);
    let mut p = Pair::new(m);
    p.render();
    assert_eq!(p.zip.1.fields().len(), 6);
    // The ten x's of the reverse field are one repeat.
    assert!(p.ras().contains(&(5, 29, 0xA7)), "{:?}", p.ras());
    p.change(|m| {
        let i = m.field_index_at(4, 20).unwrap();
        m.set_attr(i, attr::RED_RI);
        m.set_value(i, "secret");
    });
    p.render();
}

#[test]
fn runs_of_three_and_four() {
    let mut m = ScreenModel::new(false);
    m.put(2, 2, "aaa.bbbb.ccccc", attr::GREEN);
    let mut p = Pair::new(m);
    p.render();
    let ras = p.ras();
    assert!(!ras.iter().any(|r| r.2 == 0x81), "three a's: {ras:?}");
    assert!(ras.contains(&(2, 9, 0x82)), "four b's: {ras:?}");
    assert!(ras.contains(&(2, 15, 0x83)), "five c's: {ras:?}");
}

#[test]
fn runs_cross_row_ends() {
    let mut m = ScreenModel::new(false);
    m.put_bytes(3, 70, &[0x60; 30]);
    m.put_bytes(5, 79, &[0x5C; 4]);
    m.put_bytes(8, 80, &[0x4B; 3]);
    let mut p = Pair::new(m);
    p.render();
    let ras = p.ras();
    assert!(ras.contains(&(4, 19, 0x60)), "{ras:?}");
    assert!(ras.contains(&(6, 2, 0x5C)), "{ras:?}");
    assert!(!ras.iter().any(|r| r.2 == 0x4B), "{ras:?}");
}

#[test]
fn a_screen_ending_in_blanks() {
    let mut m = ScreenModel::new(false);
    m.put(1, 2, "Top", attr::WHITE);
    m.put_bytes(24, 1, &[0x40; 80]);
    let mut p = Pair::new(m);
    p.render();
    assert!(p.ras().contains(&(24, 80, 0x40)), "{:?}", p.ras());
    // The blanks become nulls: written as nulls, not skipped.
    p.change(|m| m.put_bytes(24, 1, &[NULL; 80]));
    p.render();
    assert_eq!(p.zip.1.cells()[23 * 80..], [NULL; 80]);
}

#[test]
fn a_null_tail_is_one_repeat() {
    let mut m = ScreenModel::new(false);
    for r in 1..=24 {
        m.put(r, 2, &format!("row {r}"), attr::GREEN);
        m.put(r, 40, "right", attr::WHITE);
    }
    let mut p = Pair::new(m);
    p.render();
    p.change(|m| m.clear_rows(10, 24));
    p.render();
    // From the first changed cell through the unchanged nulls between
    // to the last: one address, one repeat.
    let o = p.orders();
    assert_eq!(
        o[..2],
        [Order::Sba(10, 1), Order::Ra(24, 45, NULL)],
        "{o:?}"
    );
}

#[test]
fn unchanged_cells_are_skipped() {
    let mut m = ScreenModel::new(false);
    m.put(1, 2, &"x".repeat(70), attr::GREEN);
    m.put(2, 2, "some text on row two", attr::GREEN);
    let mut p = Pair::new(m);
    p.render();
    p.change(|m| m.put_bytes(1, 40, &ebcdic("y")));
    p.render();
    // Write To Display, one address, one byte, the cursor and the read.
    assert_eq!(p.last.0.len(), 4 + 3 + 1 + 3 + 4, "{:02x?}", p.last.0);
}

#[test]
fn a_command_key_rewrites_typed_fields() {
    let mut m = ScreenModel::new(false);
    m.field(5, 10, 20, attr::GREEN_UL, "", Ffw::default(), &[]);
    m.set_no_data_keys(&[12]);
    let mut p = Pair::new(m);
    p.render();
    for t in [&mut p.zip.1, &mut p.plain.1] {
        t.type_str("typed");
        t.key(tn5250::term::Key::F(12));
    }
    let a = p.zip.1.take_outbox().remove(0);
    let b = p.plain.1.take_outbox().remove(0);
    p.zip.0.accept(&a.data).unwrap();
    p.plain.0.accept(&b.data).unwrap();
    p.render();
}

#[test]
fn random_screens_and_updates_agree() {
    let mut seed = 0x5EED_u64;
    let mut next = move |n: usize| {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        (seed >> 33) as usize % n
    };
    for round in 0..200 {
        let wide = round % 3 == 0;
        let mut p = Pair::new(ScreenModel::new(wide));
        let (rows, cols) = if wide { (27, 132) } else { (24, 80) };
        for _ in 0..6 {
            for _ in 0..=next(12) {
                let (r, c) = (1 + next(rows), 1 + next(cols));
                let ch = [0x40, 0x00, 0x81, 0x4B, 0x60][next(5)];
                let len = next(200);
                let at = [attr::GREEN, attr::WHITE, attr::BLUE_UL, attr::NONDISPLAY][next(4)];
                match next(5) {
                    0 => p.change(|m| m.put_bytes(r, c, &vec![ch; len])),
                    1 => p.change(|m| m.put(r, c, "Text  ....  x", at)),
                    2 => p.change(|m| m.field(r, c, 1 + len, at, "abcd", Ffw::default(), &[])),
                    3 => {
                        let to = r + next(5);
                        p.change(|m| m.clear_rows(r, to));
                    }
                    _ => p.change(|m| m.error_line("message", attr::WHITE)),
                }
            }
            // Raw bytes over a field's attribute cell leave a field the
            // stream cannot describe: put the attribute back.
            p.change(|m| {
                for i in 0..m.fields().len() {
                    let f = &m.fields()[i];
                    let at = (f.row - 1) * m.cols() + f.col - 1;
                    if at > 0 && !attr::is_attr(m.cells()[at - 1]) {
                        let a = f.attr;
                        m.set_attr(i, a);
                    }
                }
            });
            p.render();
        }
    }
}
