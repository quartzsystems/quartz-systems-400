// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `od [-v] [-A base] [-j skip] [-N count] [-t type]... [-bcdosx]
//! [file...]`: a dump, sixteen bytes a line, repeated lines as `*`.

use qsh::sys::UtilCtx;

use crate::common::args::getopt;
use crate::common::io::{Out, inputs, read_input};
use crate::common::{complain, complain_sys, usage};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Named,
    Char,
    Signed,
    Unsigned,
    Octal,
    Hex,
    Float,
}

#[derive(Clone, Copy)]
struct Ty {
    kind: Kind,
    size: usize,
}

impl Ty {
    /// The width of one formatted unit.
    fn width(self) -> usize {
        match (self.kind, self.size) {
            (Kind::Named | Kind::Char, _) => 3,
            (Kind::Octal, 1) => 3,
            (Kind::Octal, 2) => 6,
            (Kind::Octal, 4) => 11,
            (Kind::Octal, _) => 22,
            (Kind::Hex, n) => 2 * n,
            (Kind::Signed, 1) => 4,
            (Kind::Signed, 2) => 6,
            (Kind::Signed, 4) => 11,
            (Kind::Signed, _) => 20,
            (Kind::Unsigned, 1) => 3,
            (Kind::Unsigned, 2) => 5,
            (Kind::Unsigned, 4) => 10,
            (Kind::Unsigned, _) => 20,
            (Kind::Float, 4) => 14,
            (Kind::Float, _) => 23,
        }
    }
}

fn parse_types(spec: &str) -> Option<Vec<Ty>> {
    let mut out = Vec::new();
    let b = spec.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let kind = match b[i] {
            b'a' => Kind::Named,
            b'c' => Kind::Char,
            b'd' => Kind::Signed,
            b'u' => Kind::Unsigned,
            b'o' => Kind::Octal,
            b'x' => Kind::Hex,
            b'f' => Kind::Float,
            _ => return None,
        };
        i += 1;
        let mut size = match kind {
            Kind::Named | Kind::Char => 1,
            Kind::Float => 8,
            _ => 4,
        };
        if matches!(kind, Kind::Signed | Kind::Unsigned | Kind::Octal | Kind::Hex) && i < b.len()
        {
            match b[i] {
                b'C' => {
                    size = 1;
                    i += 1;
                }
                b'S' => {
                    size = 2;
                    i += 1;
                }
                b'I' => {
                    size = 4;
                    i += 1;
                }
                b'L' => {
                    size = 8;
                    i += 1;
                }
                b'1' | b'2' | b'4' | b'8' => {
                    size = usize::from(b[i] - b'0');
                    i += 1;
                }
                _ => {}
            }
        } else if kind == Kind::Float && i < b.len() {
            match b[i] {
                b'F' | b'4' => {
                    size = 4;
                    i += 1;
                }
                b'D' | b'L' | b'8' => {
                    size = 8;
                    i += 1;
                }
                _ => {}
            }
        }
        out.push(Ty { kind, size });
    }
    Some(out)
}

const NAMES: [&str; 33] = [
    "nul", "soh", "stx", "etx", "eot", "enq", "ack", "bel", "bs", "ht", "nl", "vt", "ff", "cr",
    "so", "si", "dle", "dc1", "dc2", "dc3", "dc4", "nak", "syn", "etb", "can", "em", "sub", "esc",
    "fs", "gs", "rs", "us", "sp",
];

fn unit(ty: Ty, bytes: &[u8]) -> String {
    let mut padded = [0u8; 8];
    padded[..bytes.len()].copy_from_slice(bytes);
    let v = u64::from_be_bytes(padded) >> (8 * (8 - ty.size.min(8)));
    match ty.kind {
        Kind::Named => {
            let b = bytes[0] & 0x7f;
            if (b as usize) < NAMES.len() {
                NAMES[b as usize].to_string()
            } else if b == 0x7f {
                "del".to_string()
            } else {
                (b as char).to_string()
            }
        }
        Kind::Char => match bytes[0] {
            0 => "\\0".to_string(),
            7 => "\\a".to_string(),
            8 => "\\b".to_string(),
            12 => "\\f".to_string(),
            b'\n' => "\\n".to_string(),
            b'\r' => "\\r".to_string(),
            b'\t' => "\\t".to_string(),
            11 => "\\v".to_string(),
            b if (0x20..0x7f).contains(&b) => (b as char).to_string(),
            b => format!("{b:03o}"),
        },
        Kind::Octal => format!("{v:0width$o}", width = ty.width()),
        Kind::Hex => format!("{v:0width$x}", width = ty.width()),
        Kind::Unsigned => v.to_string(),
        Kind::Signed => match ty.size {
            1 => (v as u8 as i8).to_string(),
            2 => (v as u16 as i16).to_string(),
            4 => (v as u32 as i32).to_string(),
            _ => (v as i64).to_string(),
        },
        Kind::Float => {
            let s = if ty.size == 4 {
                format!("{:.6e}", f32::from_be_bytes(padded[..4].try_into().unwrap_or([0; 4])))
            } else {
                format!("{:.15e}", f64::from_be_bytes(padded))
            };
            // Rust writes `1.5e3`; C writes `1.5e+03`.
            match s.split_once('e') {
                Some((m, e)) => {
                    let (sign, digits) = match e.strip_prefix('-') {
                        Some(d) => ('-', d),
                        None => ('+', e),
                    };
                    format!("{m}e{sign}{digits:0>2}")
                }
                None => s,
            }
        }
    }
}

/// Run `od`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "od";
    let p = match getopt(&cx.argv[1..], "vA:j:N:t:bcdosx") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    let mut types: Vec<Ty> = Vec::new();
    for (o, v) in &p.opts {
        let add = match o {
            'b' => "o1",
            'c' => "c",
            'd' => "u2",
            'o' => "o2",
            's' => "d2",
            'x' => "x2",
            't' => v.as_deref().unwrap_or(""),
            _ => continue,
        };
        match parse_types(add) {
            Some(t) => types.extend(t),
            None => return usage(cx, name, &format!("The type {add} is not valid.")),
        }
    }
    if types.is_empty() {
        types.push(Ty {
            kind: Kind::Octal,
            size: 2,
        });
    }
    let base = p.value('A').unwrap_or("o");
    if !matches!(base, "d" | "o" | "x" | "n") {
        return usage(cx, name, &format!("The address base {base} is not valid."));
    }
    let size_arg = |s: &str| -> Option<u64> {
        let (digits, mult) = match s.as_bytes().last() {
            Some(b'b') => (&s[..s.len() - 1], 512),
            Some(b'k') => (&s[..s.len() - 1], 1024),
            Some(b'm') => (&s[..s.len() - 1], 1024 * 1024),
            _ => (s, 1),
        };
        let n = if let Some(h) = digits.strip_prefix("0x").or_else(|| digits.strip_prefix("0X")) {
            u64::from_str_radix(h, 16).ok()?
        } else if digits.len() > 1 && digits.starts_with('0') {
            u64::from_str_radix(&digits[1..], 8).ok()?
        } else {
            digits.parse().ok()?
        };
        Some(n * mult)
    };
    let skip = match p.value('j') {
        None => 0,
        Some(s) => match size_arg(s) {
            Some(n) => n as usize,
            None => return usage(cx, name, &format!("The offset {s} is not valid.")),
        },
    };
    let limit = match p.value('N') {
        None => None,
        Some(s) => match size_arg(s) {
            Some(n) => Some(n as usize),
            None => return usage(cx, name, &format!("The count {s} is not valid.")),
        },
    };
    let verbose = p.has('v');
    let mut data = Vec::new();
    let mut status = 0;
    for f in inputs(&p.operands) {
        match read_input(cx, &f) {
            Ok(d) => data.extend(d),
            Err(e) => {
                complain_sys(cx, name, &f, e);
                status = 1;
            }
        }
    }
    if skip > data.len() {
        complain(cx, name, "The offset is past the end of the input.");
        return 1;
    }
    let mut data = data[skip..].to_vec();
    if let Some(n) = limit {
        data.truncate(n);
    }
    let per_byte = types
        .iter()
        .map(|t| (t.width() + 1).div_ceil(t.size))
        .max()
        .unwrap_or(1);
    let addr = |off: usize| -> String {
        match base {
            "d" => format!("{off:07}"),
            "x" => format!("{off:06x}"),
            "n" => String::new(),
            _ => format!("{off:07o}"),
        }
    };
    let mut out = Out::stdout(cx);
    let mut off = 0;
    let mut prev: Option<Vec<u8>> = None;
    let mut starred = false;
    while off < data.len() {
        let end = (off + 16).min(data.len());
        let block = &data[off..end];
        if !verbose && block.len() == 16 && prev.as_deref() == Some(block) {
            if !starred {
                out.text("*\n");
                starred = true;
            }
            off = end;
            continue;
        }
        starred = false;
        prev = Some(block.to_vec());
        for (k, ty) in types.iter().enumerate() {
            let mut line = if k == 0 {
                addr(off + skip)
            } else {
                " ".repeat(addr(off + skip).len())
            };
            let field = per_byte * ty.size - 1;
            let mut i = 0;
            while i < block.len() {
                let e = (i + ty.size).min(block.len());
                let u = unit(*ty, &block[i..e]);
                line.push(' ');
                line.push_str(&format!("{u:>field$}"));
                i = e;
            }
            line.push('\n');
            if !out.write(line.as_bytes()) {
                return out.finish(cx, name, status);
            }
        }
        off = end;
    }
    if base != "n" {
        out.text(&format!("{}\n", addr(data.len() + skip)));
    }
    out.finish(cx, name, status)
}
