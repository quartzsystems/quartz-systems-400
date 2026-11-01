// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The display's mode follows the display file, not the device: a
//! 24x80 file goes out with Clear Unit and the error line on row 24
//! even on a 27x132 device; a 27x132 file on a device that shows it
//! goes out with Clear Unit Alternate and row 27; one job switches
//! between them; a 27x132 file on a 24x80 device stays 24x80.

use dspf::model::DisplayFile;
use dspf::rt::{Env, Runtime};
use dspf::{CompileOpts, NoRefs, Source, compile_dspf};
use tn5250::ds::cmd;
use tn5250::ds::parse::{Command, Order, parse};
use tn5250::ds::wdsf::{Capabilities, Model};
use tn5250::record::Record;
use tn5250::screen::{Read, ScreenModel};
use tn5250::term::Terminal;

/// A DDS line from (column, text) pieces, columns 1-based.
fn spec(pieces: &[(usize, &str)]) -> String {
    let mut line = String::new();
    for (col, text) in pieces {
        while line.len() < col - 1 {
            line.push(' ');
        }
        line.push_str(text);
    }
    line
}

fn compiled(text: &str) -> DisplayFile {
    let (src, diags) = Source::from_text(text);
    assert!(diags.is_empty(), "{diags:?}");
    let c = compile_dspf(&src, &NoRefs, &CompileOpts::default());
    c.definition.unwrap_or_else(|| panic!("{}", c.listing))
}

/// A file defined for 27x132 only: text at row 1 column 100, an input
/// field, text on row 27.
fn wide_file() -> DisplayFile {
    let text = [
        spec(&[(6, "A"), (45, "DSPSIZ(27 132 *DS4)")]),
        spec(&[(6, "A"), (17, "R"), (19, "WIDE")]),
        spec(&[(6, "A"), (39, "  1"), (42, "100"), (45, "'Wide title'")]),
        spec(&[(6, "A"), (19, "NAME"), (33, "10"), (35, "A"), (38, "B"), (39, "  5"), (42, "  2")]),
        spec(&[(6, "A"), (39, " 27"), (42, "  2"), (45, "'Bottom row'")]),
    ]
    .join("\n");
    compiled(&text)
}

/// The sign-on's output buffer.
fn sign_on_out(df: &DisplayFile) -> Vec<u8> {
    let rec = df.record("SIGNON").expect("format");
    let values = [("QSYSNAME", "SYSNAME"), ("SBSNAME", "QINTER"), ("DEVNAME", "QPADEV0001")];
    let mut out = Vec::new();
    for bf in &rec.layout.output.fields {
        let f = &rec.fields[bf.field];
        let v = values
            .iter()
            .find(|(n, _)| f.name.as_deref() == Some(*n))
            .map_or("", |(_, v)| v);
        let mut b = ccsid::to_ebcdic(&format!("{v:<w$}", w = bf.len as usize)).unwrap();
        b.truncate(bf.len as usize);
        out.extend(b);
    }
    out
}

/// A terminal of a model.
fn terminal(model: Model) -> Terminal {
    Terminal::new(Capabilities {
        model,
        gui: false,
        color: true,
    })
}

/// What a render sent: the clear command (when there was one) and the
/// start of header's error row.
fn sent(data: &[u8]) -> (Option<u8>, Option<u8>) {
    let mut clear = None;
    let mut err = None;
    for c in parse(data).unwrap() {
        match c {
            Command::ClearUnit => clear = Some(cmd::CLEAR_UNIT),
            Command::ClearUnitAlt(_) => clear = Some(cmd::CLEAR_UNIT_ALT),
            Command::Wtd { orders, .. } => {
                for o in orders {
                    if let Order::Soh(b) = o {
                        err = b.get(3).copied();
                    }
                }
            }
            _ => {}
        }
    }
    (clear, err)
}

/// Render, check the clear and error row, apply to the terminal.
fn show(m: &mut ScreenModel, t: &mut Terminal, clear: u8, err: u8) {
    let o = m.render(Read::MdtFields);
    assert_eq!(sent(&o.data), (Some(clear), Some(err)));
    t.apply(&Record::new(o.opcode, o.data)).unwrap();
}

/// Write the sign-on through a runtime opened for `wide`.
fn write_sign_on(m: &mut ScreenModel, wide: bool) {
    let df = compiled(dspf::sys::QDSIGNON);
    let out = sign_on_out(&df);
    let mut rt = Runtime::new(df, wide);
    rt.write(m, &Env::default(), "SIGNON", &out, &[]).unwrap();
}

#[test]
fn sign_on_is_24x80_on_a_27x132_device() {
    // The model starts in the device's size, as the server makes it.
    let mut m = ScreenModel::new(true);
    write_sign_on(&mut m, true);
    m.error_line("Message", tn5250::ds::attr::WHITE);
    let mut t = terminal(Model::M3477Fc);
    show(&mut m, &mut t, cmd::CLEAR_UNIT, 24);
    assert_eq!((t.rows(), t.cols()), (24, 80));
    let rows = t.text_rows();
    assert_eq!(rows.len(), 24);
    assert!(rows[0].contains("Sign On"));
    assert!(rows[23].contains("Message"), "{:?}", rows[23]);
}

#[test]
fn sign_on_copyright_on_row_24_of_a_27x132_device() {
    let mut m = ScreenModel::new(true);
    write_sign_on(&mut m, true);
    let mut t = terminal(Model::M3477Fc);
    show(&mut m, &mut t, cmd::CLEAR_UNIT, 24);
    let rows = t.text_rows();
    assert!(rows[23].contains("COPYRIGHT"), "{:?}", rows[23]);
}

#[test]
fn wide_file_is_27x132_on_a_27x132_device() {
    let mut m = ScreenModel::new(false);
    let mut rt = Runtime::new(wide_file(), true);
    assert!(rt.shows_wide());
    rt.write(&mut m, &Env::default(), "WIDE", b"ABC       ", &[]).unwrap();
    let mut t = terminal(Model::M3477Fc);
    show(&mut m, &mut t, cmd::CLEAR_UNIT_ALT, 27);
    assert_eq!((t.rows(), t.cols()), (27, 132));
    let rows = t.text_rows();
    assert_eq!(&rows[0][99..109], "Wide title");
    assert!(rows[26].starts_with(" Bottom row"));
}

#[test]
fn one_job_switches_between_the_sizes() {
    let mut m = ScreenModel::new(true);
    let mut t = terminal(Model::M3477Fc);
    let mut wide = Runtime::new(wide_file(), true);

    write_sign_on(&mut m, true);
    show(&mut m, &mut t, cmd::CLEAR_UNIT, 24);
    assert_eq!(t.cols(), 80);

    wide.write(&mut m, &Env::default(), "WIDE", b"ABC       ", &[]).unwrap();
    show(&mut m, &mut t, cmd::CLEAR_UNIT_ALT, 27);
    assert_eq!(t.cols(), 132);

    // The same size again: only what changed, no clear.
    wide.write(&mut m, &Env::default(), "WIDE", b"XYZ       ", &[]).unwrap();
    let o = m.render(Read::MdtFields);
    assert_eq!(sent(&o.data).0, None);
    t.apply(&Record::new(o.opcode, o.data)).unwrap();

    write_sign_on(&mut m, true);
    show(&mut m, &mut t, cmd::CLEAR_UNIT, 24);
    assert_eq!((t.rows(), t.cols()), (24, 80));
    assert!(t.text_rows()[23].contains("COPYRIGHT"));

    wide.write(&mut m, &Env::default(), "WIDE", b"ABC       ", &[]).unwrap();
    show(&mut m, &mut t, cmd::CLEAR_UNIT_ALT, 27);
    assert_eq!((t.rows(), t.cols()), (27, 132));
}

#[test]
fn wide_file_on_a_24x80_device_stays_24x80() {
    let mut m = ScreenModel::new(false);
    let mut rt = Runtime::new(wide_file(), false);
    assert!(!rt.shows_wide());
    rt.write(&mut m, &Env::default(), "WIDE", b"ABC       ", &[]).unwrap();
    let mut t = terminal(Model::M3179);
    show(&mut m, &mut t, cmd::CLEAR_UNIT, 24);
    assert_eq!((t.rows(), t.cols()), (24, 80));
}
