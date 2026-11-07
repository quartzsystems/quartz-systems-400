// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Streaming mixed data: splits, shift states across converters, flushes.

use super::mixed::{SAMPLES, hex};
use crate::conv::*;

/// The samples in one CCSID back to back, with redundant shifts and an
/// unterminated double-byte run at the end.
fn input(c: u16) -> Vec<u8> {
    let mut v = Vec::new();
    for (_, b930, b939, _) in SAMPLES {
        v.extend(hex(if c == 939 { b939 } else { b930 }));
        v.push(0x25);
    }
    v.extend(hex("0E0E45620F0FC10E4566"));
    v
}

fn run(from: u16, to: u16, parts: &[&[u8]]) -> Vec<u8> {
    let mut c = Converter::new(from, to).unwrap();
    let mut out = Vec::new();
    for p in parts {
        c.feed(p, &mut out);
    }
    c.finish(&mut out);
    out
}

fn cases() -> Vec<(u16, u16, Vec<u8>)> {
    let text: String = SAMPLES.iter().flat_map(|s| [s.0, "\n"]).collect();
    vec![
        (930, 1208, input(930)),
        (930, 939, input(930)),
        (930, 5026, input(930)),
        (939, 1200, input(939)),
        (930, 65535, input(930)),
        (1208, 930, text.clone().into_bytes()),
        (1208, 939, text.into_bytes()),
        (300, 1208, hex(SAMPLES[2].3.unwrap())),
        (1208, 300, SAMPLES[2].0.as_bytes().to_vec()),
        (300, 930, hex("4557457545FEFE")),
    ]
}

#[test]
fn every_split_gives_the_same_result() {
    for (from, to, data) in cases() {
        let whole = run(from, to, &[&data]);
        for i in 0..=data.len() {
            assert_eq!(
                run(from, to, &[&data[..i], &data[i..]]),
                whole,
                "{from}->{to} at {i}"
            );
            for j in i..=data.len() {
                assert_eq!(
                    run(from, to, &[&data[..i], &data[i..j], &data[j..]]),
                    whole,
                    "{from}->{to} at {i},{j}"
                );
            }
        }
        let bytes: Vec<&[u8]> = data.iter().map(std::slice::from_ref).collect();
        assert_eq!(run(from, to, &bytes), whole, "{from}->{to} by byte");
    }
}

#[test]
fn a_new_converter_resumes_from_the_shift_state() {
    for (from, to) in [(930, 1208), (930, 939), (930, 5026), (300, 1208)] {
        let data = if from == 300 {
            hex(SAMPLES[2].3.unwrap())
        } else {
            input(930)
        };
        let whole = to_string(from, &data).unwrap();
        for i in 0..=data.len() {
            let mut a = Converter::new(from, to).unwrap();
            let mut out = Vec::new();
            a.feed(&data[..i], &mut out);
            a.flush_shift(&mut out);
            let rest = &data[i - a.pending()..];
            let mut b = Converter::new(from, to)
                .unwrap()
                .with_shift(a.input_shift());
            b.feed(rest, &mut out);
            b.finish(&mut out);
            assert_eq!(to_string(to, &out).unwrap(), whole, "{from}->{to} at {i}");
        }
    }
}

#[test]
fn input_shift_follows_the_bytes() {
    let mut c = Converter::new(930, 1208).unwrap();
    let mut out = Vec::new();
    assert_eq!(c.input_shift(), Shift::Single);
    c.feed(&[SO], &mut out);
    assert_eq!((c.input_shift(), c.pending()), (Shift::Double, 0));
    c.feed(&[0x45], &mut out);
    assert_eq!((c.input_shift(), c.pending()), (Shift::Double, 1));
    c.feed(&[0x62, SI], &mut out);
    assert_eq!((c.input_shift(), c.pending()), (Shift::Single, 0));
    assert_eq!(out, "日".as_bytes());
    // Starting in double-byte state.
    let mut c = Converter::new(930, 1208).unwrap().with_shift(Shift::Double);
    let mut out = Vec::new();
    c.feed(&hex("45620FC1"), &mut out);
    assert_eq!(out, "日A".as_bytes());
    // A double-byte source is always in double-byte state; others single.
    let c = Converter::new(300, 1208).unwrap().with_shift(Shift::Single);
    assert_eq!(c.input_shift(), Shift::Double);
    let c = Converter::new(37, 1208).unwrap().with_shift(Shift::Double);
    assert_eq!(c.input_shift(), Shift::Single);
}

#[test]
fn flush_shift_closes_the_output_run() {
    let mut c = Converter::new(1208, 930).unwrap();
    let mut out = Vec::new();
    c.feed("日本".as_bytes(), &mut out);
    assert_eq!(out, hex("0E45624566"));
    c.flush_shift(&mut out);
    c.flush_shift(&mut out);
    assert_eq!(out, hex("0E456245660F"));
    out.clear();
    c.feed("語".as_bytes(), &mut out);
    c.feed(&"は".as_bytes()[..2], &mut out);
    c.flush_shift(&mut out);
    assert_eq!(c.pending(), 2);
    c.feed(&"は".as_bytes()[2..], &mut out);
    c.finish(&mut out);
    assert_eq!(out, hex("0E48E70F0E449D0F"));
    // Nothing to close in single-byte state or for a non-mixed target.
    let mut out = Vec::new();
    c.flush_shift(&mut out);
    let mut d = Converter::new(1208, 300).unwrap();
    d.feed("日".as_bytes(), &mut out);
    d.flush_shift(&mut out);
    assert_eq!(out, hex("4562"));
}

#[test]
fn finish_substitutes_half_a_pair() {
    assert_eq!(
        convert(930, 1208, &hex("0E45")).unwrap(),
        "\u{FFFD}".as_bytes()
    );
    assert_eq!(convert(939, 930, &hex("0E45")).unwrap(), hex("0EFEFE0F"));
    assert_eq!(convert(300, 37, &hex("456245")).unwrap(), hex("3F3F"));
    // A pass-through copies it and closes the run.
    assert_eq!(convert(930, 5026, &hex("0E45")).unwrap(), hex("0E450F"));
    let mut c = Converter::new(930, 5026).unwrap();
    let mut out = Vec::new();
    c.feed(&hex("C10E4562"), &mut out);
    c.flush_shift(&mut out);
    c.feed(&hex("4566"), &mut out);
    c.finish(&mut out);
    assert_eq!(out, hex("C10E45620F0E45660F"));
}
