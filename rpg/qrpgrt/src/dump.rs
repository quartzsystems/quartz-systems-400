// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DUMP`: the formatted dump, a spooled file `QPPGMDMP` in IBM's
//! layout: the program status area, the indicators and, with a debug
//! view, every variable by name with its value and its bytes in
//! hexadecimal. The compiler describes the variables
//! (`rpgc::codegen::dump`); the open data path sections IBM prints for
//! files are not printed.

#![allow(clippy::match_same_arms, clippy::similar_names, clippy::too_many_lines, clippy::struct_excessive_bools, clippy::unnecessary_wraps, clippy::unused_self)]

use std::fmt::Write as _;

use codegen::abi::Env;

/// The last line of a page before the next one starts.
const OVERFLOW: u32 = 60;

/// Bytes of hexadecimal per line.
const HEX_CHUNK: usize = 40;

/// A value no wider than this has its hexadecimal on the same line.
const SAME_LINE: usize = 15;

/// The dump's lines, page by page.
struct Out {
    title: String,
    /// `(page, line, text)`.
    rows: Vec<(u32, u32, String)>,
    page: u32,
    line: u32,
}

impl Out {
    fn new(title: String) -> Self {
        let mut o = Self { title, rows: Vec::new(), page: 0, line: 0 };
        o.new_page();
        o
    }

    fn new_page(&mut self) {
        self.page += 1;
        let head = format!("Page{:>5}", self.page);
        self.rows.push((self.page, 1, format!("{head:>132}")));
        self.rows.push((self.page, 2, format!(" {}", self.title)));
        self.line = 2;
    }

    /// A line after `skip` blank ones.
    fn put(&mut self, skip: u32, text: impl Into<String>) {
        self.line += skip + 1;
        if self.line > OVERFLOW {
            self.new_page();
            self.line += 1;
        }
        self.rows.push((self.page, self.line, text.into()));
    }

    /// A label and its value, as the status area prints them.
    fn field(&mut self, skip: u32, label: &str, value: &str) {
        if value.is_empty() {
            self.put(skip, format!(" {label:.<39}:"));
        } else {
            self.put(skip, format!(" {label:.<39}:   {value}"));
        }
    }

    /// The records with first-character forms control.
    fn records(&self) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        let (mut page, mut line) = (0, 0);
        for (p, l, text) in &self.rows {
            let cc = if *p == page {
                match l - line {
                    1 => ' ',
                    2 => '0',
                    _ => '-',
                }
            } else {
                '1'
            };
            // A gap of more than three lines: blank lines first.
            if *p == page && l - line > 3 {
                for _ in 0..(l - line - 3) {
                    out.push(crate::value::ebcdic(" "));
                }
            }
            page = *p;
            line = *l;
            out.push(crate::value::ebcdic(&format!("{cc}{text}")));
        }
        out
    }
}

/// Labels written with dot leaders the way the status area spells them.
fn label(text: &str) -> String {
    // `Name . . . .`: the text, then dots and blanks alternating to 39.
    let mut s = text.to_string();
    if s.len().is_multiple_of(2) {
        s.push(' ');
    }
    while s.len() < 39 {
        s.push_str(if s.len() % 2 == 1 { "." } else { " " });
    }
    s.truncate(39);
    s
}

/// One variable of the description.
#[derive(Clone, Debug, Default)]
struct Var {
    depth: usize,
    name: String,
    attr: String,
    dim: u32,
    code: String,
    slot: i64,
    stride: usize,
}

/// A section: the globals (`None`) or a subprocedure's.
type Section = (Option<String>, Vec<Var>);

/// The description the compiler wrote: the header fields and the
/// sections.
fn parse(desc: &str) -> (Vec<String>, Vec<Section>) {
    let mut head = Vec::new();
    let mut secs: Vec<Section> = vec![(None, Vec::new())];
    for rec in desc.split(';') {
        let f: Vec<&str> = rec.split('|').collect();
        match f.first().copied() {
            Some("H") => head = f[1..].iter().map(|s| (*s).to_string()).collect(),
            Some("P") => secs.push((f.get(1).map(|s| (*s).to_string()), Vec::new())),
            Some("V") if f.len() >= 8 => {
                let v = Var {
                    depth: f[1].parse().unwrap_or(0),
                    name: f[2].to_string(),
                    attr: f[3].to_string(),
                    dim: f[4].parse().unwrap_or(0),
                    code: f[5].to_string(),
                    slot: f[6].parse().unwrap_or(-1),
                    stride: f[7].parse().unwrap_or(0),
                };
                if let Some(s) = secs.last_mut() {
                    s.1.push(v);
                }
            }
            _ => {}
        }
    }
    (head, secs)
}

/// The parts of a type code.
fn parts(code: &str) -> (char, u32, u32) {
    let mut it = code.split(':');
    let k = it.next().and_then(|s| s.chars().next()).unwrap_or('X');
    let a = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let b = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (k, a, b)
}

/// The widest value a type can show.
fn max_width(code: &str) -> usize {
    let (k, a, _) = parts(code);
    let a = a as usize;
    match k {
        'C' | 'V' | 'U' | 'W' | 'G' | 'D' | 'T' | 'S' => a + 2,
        'N' => 3,
        'P' | 'Z' | 'B' => a + 2,
        'I' => match a {
            3 => 4,
            5 => 6,
            10 => 11,
            _ => 20,
        },
        'J' => a,
        'F' => 21,
        _ => 2 * a + 3,
    }
}

/// A decimal value's digits as the dump shows them: every digit, the
/// point always, a minus sign in front.
fn decimal_text(digits: &[u8], neg: bool, scale: usize) -> String {
    let n = digits.len();
    let int: String = digits[..n.saturating_sub(scale)].iter().map(|d| char::from(b'0' + d)).collect();
    let frac: String = digits[n.saturating_sub(scale)..].iter().map(|d| char::from(b'0' + d)).collect();
    format!("{}{int}.{frac}", if neg { "-" } else { "" })
}

/// A float in the dump's form: twelve decimals (six for 4 bytes), a
/// three-digit exponent, right-aligned in 21.
fn float_text(v: f64, short: bool) -> String {
    let places = if short { 6 } else { 12 };
    let s = format!("{:.*E}", places, v.abs());
    let (m, e) = s.split_once('E').unwrap_or((&s, "0"));
    let e: i32 = e.parse().unwrap_or(0);
    let t = format!("{}{m}E{}{:03}", if v < 0.0 { "-" } else { "" }, if e < 0 { '-' } else { '+' }, e.abs());
    format!("{t:>21}")
}

/// The value column of a scalar.
fn value_text(code: &str, b: &[u8]) -> String {
    let (k, a, s) = parts(code);
    match k {
        'C' | 'D' | 'T' | 'S' => format!("'{}'", ccsid::from_ebcdic(b)),
        'G' => format!("'{}'", ccsid::from_ebcdic(b)),
        'V' | 'W' => {
            let p = if s == 4 { 4 } else { 2 };
            let len = if p == 4 {
                b.get(..4).map_or(0, |x| u32::from_be_bytes([x[0], x[1], x[2], x[3]]) as usize)
            } else {
                b.get(..2).map_or(0, |x| usize::from(u16::from_be_bytes([x[0], x[1]])))
            };
            let unit = if k == 'W' { 2 } else { 1 };
            let data = b.get(p..p + (len * unit).min(b.len().saturating_sub(p))).unwrap_or(&[]);
            if k == 'W' { format!("'{}'", utf16(data)) } else { format!("'{}'", ccsid::from_ebcdic(data)) }
        }
        'U' => format!("'{}'", utf16(b)),
        'N' => if b.first() == Some(&0xF1) { "'1'".into() } else { "'0'".into() },
        'P' => {
            let mut digits = Vec::new();
            for (i, x) in b.iter().enumerate() {
                digits.push(x >> 4);
                if i + 1 < b.len() {
                    digits.push(x & 0x0F);
                }
            }
            let neg = b.last().is_some_and(|x| matches!(x & 0x0F, 0x0B | 0x0D));
            let skip = digits.len().saturating_sub(a as usize);
            decimal_text(&digits[skip..], neg, s as usize)
        }
        'Z' => {
            let digits: Vec<u8> = b.iter().map(|x| x & 0x0F).collect();
            let neg = b.last().is_some_and(|x| matches!(x >> 4, 0x0B | 0x0D));
            decimal_text(&digits, neg, s as usize)
        }
        'B' => {
            let v = signed(b);
            let digits: Vec<u8> = format!("{:0>w$}", v.unsigned_abs(), w = a as usize).bytes().map(|d| d - b'0').collect();
            decimal_text(&digits, v < 0, s as usize)
        }
        'I' => signed(b).to_string(),
        'J' => unsigned(b).to_string(),
        'F' => {
            let v = if b.len() == 4 {
                f64::from(f32::from_be_bytes(b.try_into().unwrap_or([0; 4])))
            } else {
                f64::from_be_bytes(b.try_into().unwrap_or([0; 8]))
            };
            float_text(v, b.len() == 4)
        }
        '*' | '&' => {
            let p = mi::FatPtr::from_bytes(&b.try_into().unwrap_or([0; 16]));
            let kind = if k == '&' { "PRP" } else { "SPP" };
            if p.is_null() { format!("{kind}:*NULL") } else { format!("{kind}:{:016X}", p.offset) }
        }
        _ => String::new(),
    }
}

fn utf16(b: &[u8]) -> String {
    let units: Vec<u16> = b.chunks(2).map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect();
    String::from_utf16_lossy(&units)
}

fn signed(b: &[u8]) -> i64 {
    match b.len() {
        1 => i64::from(i8::from_be_bytes([b[0]])),
        2 => i64::from(i16::from_be_bytes([b[0], b[1]])),
        4 => i64::from(i32::from_be_bytes(b.try_into().unwrap_or([0; 4]))),
        8 => i64::from_be_bytes(b.try_into().unwrap_or([0; 8])),
        _ => 0,
    }
}

fn unsigned(b: &[u8]) -> u64 {
    b.iter().fold(0u64, |acc, x| acc << 8 | u64::from(*x))
}

fn hex(b: &[u8]) -> String {
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02X}");
        s
    })
}

/// A value's line and its hexadecimal: on the same line when the type
/// is narrow, else `VALUE IN HEX` lines of forty bytes.
fn value_lines(o: &mut Out, first: &str, code: &str, b: &[u8]) {
    let v = value_text(code, b);
    let pointer = matches!(code, "*" | "&");
    if pointer {
        o.put(0, format!("{first}{v}"));
        return;
    }
    let chunks: Vec<&[u8]> = if b.is_empty() { vec![b] } else { b.chunks(HEX_CHUNK).collect() };
    if max_width(code) <= SAME_LINE && chunks.len() == 1 {
        o.put(0, format!("{first}{v:<17}'{}'X", hex(b)));
        return;
    }
    o.put(0, format!("{first}{v}"));
    for (i, c) in chunks.iter().enumerate() {
        let lead = if i == 0 { format!("{:23}{:<21}", "", "VALUE IN HEX") } else { format!("{:25}{:<19}", "", i * HEX_CHUNK + 1) };
        o.put(0, format!("{lead}'{}'X", hex(c)));
    }
}

/// The bytes of a variable: its storage, or zeros where it is not
/// reachable.
fn storage(addrs: u64, j: &mi::Job, v: &Var, len: usize) -> Vec<u8> {
    let Ok(slot) = usize::try_from(v.slot) else { return vec![0; len] };
    let raw: [u8; 16] = crate::value::bytes(addrs + 16 * slot as u64, 16).try_into().unwrap_or([0; 16]);
    let p = mi::FatPtr::from_bytes(&raw);
    match j.spaces.deref(p) {
        Ok(a) if a != 0 => crate::value::bytes(a, len).to_vec(),
        _ => vec![0; len],
    }
}

/// The variables of one section.
fn variables(o: &mut Out, j: &mi::Job, addrs: u64, vars: &[Var]) {
    o.put(0, format!(" {:<22}{:<21}VALUE", "NAME", "ATTRIBUTES"));
    for v in vars {
        let name = format!("{}{}", "  ".repeat(v.depth), v.name);
        let first = format!(" {name:<22}{:<21}", v.attr);
        if v.code.starts_with('X') && v.attr == "DS" {
            if v.dim > 0 {
                o.put(0, format!("{first}DIM({})", v.dim));
            } else {
                o.put(0, format!(" {name:<22}{}", v.attr));
            }
            continue;
        }
        if v.dim == 0 {
            let b = storage(addrs, j, v, v.stride);
            value_lines(o, &first, &v.code, &b);
            continue;
        }
        o.put(0, format!("{first}DIM({})", v.dim));
        let all = storage(addrs, j, v, v.stride * v.dim as usize);
        let elems: Vec<&[u8]> = all.chunks(v.stride.max(1)).collect();
        let mut i = 0;
        while i < elems.len() {
            let mut k = i;
            while k + 1 < elems.len() && elems[k + 1] == elems[i] {
                k += 1;
            }
            let which = if k == i { format!("({})", i + 1) } else { format!("({}-{})", i + 1, k + 1) };
            value_lines(o, &format!("{:25}{which:<19}", ""), &v.code, elems[i]);
            i = k + 1;
        }
    }
}

/// The status area of page 1.
fn status_area(o: &mut Out, j: &mi::Job, head: &[String]) {
    let (lib, pgm, module) = match j.act.frames.last().and_then(|f| f.program.clone().map(|p| (p, f.code))) {
        Some((p, code)) => {
            let (l, n) = p.key.split_once('/').map_or((String::new(), p.key.clone()), |(l, n)| (l.to_string(), n.to_string()));
            let m = p
                .proc_of(code)
                .and_then(|(mi, _)| p.object.modules.get(mi as usize).map(|m| m.module.name.clone()))
                .unwrap_or_default();
            (l, n, m)
        }
        None => (String::new(), String::new(), String::new()),
    };
    let module = if module.is_empty() { head.first().cloned().unwrap_or_default() } else { module };
    let module = module.to_ascii_uppercase();
    let (date, time) = now();
    o.put(1, " Program Status Area:");
    o.field(0, &label("Procedure Name"), &module);
    o.field(0, &label("Program Name"), &pgm);
    o.field(0, &label("   Library"), &lib);
    o.field(0, &label("Module Name."), &module);
    o.field(0, &label("Program Status"), "00000");
    o.field(0, &label("Previous Status "), "00000");
    o.field(0, &label("Statement in Error"), "00000000");
    o.field(0, &label("RPG Routine "), "*DETC");
    o.field(0, &label("Number of Parameters"), "");
    o.field(0, &label("Message Type"), "");
    o.field(0, &label("Additional Message Info "), "");
    o.field(0, &label("Message Data"), "");
    o.field(1, &label("Status that caused RNX9001"), "");
    o.field(0, &label("Last File Used"), "");
    o.field(0, &label("Last File Status"), "");
    o.field(0, &label("Last File Operation "), "");
    o.field(0, &label("Last File Routine "), "");
    o.field(0, &label("Last File Statement "), "");
    o.field(0, &label("Last File Record Name "), "");
    o.field(0, &label("Job Name"), &j.ident.name);
    o.field(0, &label("User Name "), &j.ident.user);
    o.field(0, &label("Job Number"), &j.ident.number);
    o.field(0, &label("Date Entered System "), &format!("{}{}{}", &date[4..6], &date[6..8], &date[0..4]));
    o.field(0, &label("   Century"), &date[0..2]);
    o.field(0, &label("Date Started"), "*N/A*");
    o.field(0, &label("Time Started"), "*N/A*");
    o.field(0, &label("Compile Date"), &format!("{}{}{}", &date[4..6], &date[6..8], &date[2..4]));
    o.field(0, &label("Compile Time"), &time);
    o.field(0, &label("Compiler Level"), "0001");
    o.field(0, &label("Source File "), head.get(1).map_or("", String::as_str));
    o.field(0, &label("   Library"), head.get(2).map_or("", String::as_str));
    o.field(0, &label("Member"), head.get(3).map_or("", String::as_str));
}

/// Today (`YYYYMMDD`) and the time (`HHMMSS`), UTC.
fn now() -> (String, String) {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    let t = secs % 86_400;
    (format!("{y:04}{m:02}{d:02}"), format!("{:02}{:02}{:02}", t / 3600, t / 60 % 60, t % 60))
}

/// The indicator groups: heading and the names, ten to a line.
const GROUPS: &[(&str, std::ops::Range<usize>)] = &[
    ("Halt Indicators:", 0..9),
    ("Command/Function Key Indicators:", 9..33),
    ("Control Level Indicators:", 33..42),
    ("Overflow Indicators:", 42..50),
    ("External Indicators:", 50..58),
];

/// The indicators: the special ones, `01`-`99`, then `LR`, `MR`, `RT`
/// and `1P`, each as `NAME 'v'`.
fn indicators(o: &mut Out, inds: &[u8]) {
    let names = DUMP_NAMES;
    let item = |i: usize| -> String {
        let n = if i < 58 { names[i].to_string() } else if i < 157 { format!("{:02}", i - 57) } else { names[i - 99].to_string() };
        let v = if inds.get(i) == Some(&0xF1) { '1' } else { '0' };
        format!("{n} '{v}'")
    };
    let line = |o: &mut Out, r: std::ops::Range<usize>| {
        let all: Vec<usize> = r.collect();
        for chunk in all.chunks(10) {
            let t: Vec<String> = chunk.iter().map(|&i| item(i)).collect();
            o.put(0, format!(" {}", t.join("    ")));
        }
    };
    for (head, r) in GROUPS {
        o.put(0, format!(" {head}"));
        line(o, r.clone());
    }
    o.put(0, " General Indicators:");
    line(o, 58..157);
    o.put(0, " Internal Indicators:");
    line(o, 157..161);
}

/// The indicator names the compiler's buffer holds before the general
/// ones, then the four internal ones (`rpgc::codegen::dump::DUMP_INDS`).
const DUMP_NAMES: &[&str] = &[
    "H1", "H2", "H3", "H4", "H5", "H6", "H7", "H8", "H9", "KA", "KB", "KC", "KD", "KE", "KF", "KG", "KH", "KI", "KJ",
    "KK", "KL", "KM", "KN", "KP", "KQ", "KR", "KS", "KT", "KU", "KV", "KW", "KX", "KY", "L1", "L2", "L3", "L4", "L5",
    "L6", "L7", "L8", "L9", "OA", "OB", "OC", "OD", "OE", "OF", "OG", "OV", "U1", "U2", "U3", "U4", "U5", "U6", "U7",
    "U8", "LR", "MR", "RT", "1P",
];

/// The whole dump as records.
fn render(j: &mi::Job, ident: &str, desc: &str, addrs: u64, inds: &[u8]) -> Vec<Vec<u8>> {
    let (head, secs) = parse(desc);
    let title = if ident.trim().is_empty() { "ILE RPG FORMATTED DUMP".to_string() } else { ident.to_string() };
    let mut o = Out::new(title);
    status_area(&mut o, j, &head);
    o.new_page();
    let module = head.first().cloned().unwrap_or_default();
    o.field(0, &label("Module Name."), &module);
    o.field(0, &label("Optimization Level "), "*NONE");
    indicators(&mut o, inds);
    let has_vars = secs.iter().any(|(_, v)| !v.is_empty());
    if has_vars || secs.len() > 1 {
        let mut skip = 1;
        for (name, vars) in &secs {
            if let Some(n) = name {
                o.put(1, format!(" Local variables for subprocedure : {n}"));
                skip = 0;
            }
            if skip == 1 {
                o.line += 1;
            }
            variables(&mut o, j, addrs, vars);
            skip = 0;
        }
    } else {
        o.put(1, " Dump terminated because variable data not available.");
    }
    o.put(1, format!("{:13}* * * * *   E N D   O F   R P G   D U M P   * * * * *", ""));
    o.records()
}

/// `_QRNX_DUMP(ident, desc, addrs, inds)`: print the dump to a new
/// `QPPGMDMP` spooled file.
pub extern "sysv64" fn dump(env: *mut Env, _argc: i64, _od: i64, ident: i64, desc: i64, addrs: i64, inds: i64) {
    let j = mi::storage::job_of(env);
    let ident = varchar(ident);
    let desc = varchar(desc);
    let inds = crate::value::bytes(inds.cast_unsigned(), DUMP_NAMES.len() + 99).to_vec();
    let recs = render(j, &ident, &desc, addrs.cast_unsigned(), &inds);
    let ag = j
        .act
        .frames
        .last()
        .and_then(|f| j.act.groups().find(|g| g.id == f.ag))
        .map_or_else(|| "*DFTACTGRP".to_string(), |g| g.name.clone());
    let Some(sink) = j.spool.as_mut() else { return };
    let Ok(mut open) = sink.open("QPPGMDMP", "*LIBL", &ag) else { return };
    open.prtf.fcfc = true;
    open.prtf.page_width = open.prtf.page_width.max(132);
    let mut p = mi::io::spool::PrinterState::new(open);
    for r in &recs {
        let _ = p.write(sink.as_mut(), "", r);
    }
    let _ = p.close(sink.as_mut());
}

/// A `const varchar` parameter's text.
fn varchar(addr: i64) -> String {
    let len = crate::value::bytes(addr.cast_unsigned(), 2);
    let n = usize::from(u16::from_be_bytes([len[0], len[1]]));
    ccsid::from_ebcdic(&crate::value::bytes(addr.cast_unsigned(), 2 + n)[2..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_have_dot_leaders() {
        assert_eq!(format!(" {}:", label("Procedure Name")), " Procedure Name . . . . . . . . . . . . :");
        assert_eq!(format!(" {}:", label("Module Name.")), " Module Name. . . . . . . . . . . . . . :");
        assert_eq!(format!(" {}:", label("   Library")), "    Library . . . . . . . . . . . . . . :");
        assert_eq!(format!(" {}:", label("Previous Status ")), " Previous Status  . . . . . . . . . . . :");
        assert_eq!(format!(" {}:", label("Status that caused RNX9001")), " Status that caused RNX9001 . . . . . . :");
    }

    #[test]
    fn values_as_the_dump_shows_them() {
        assert_eq!(value_text("P:7:2", &[0x00, 0x12, 0x34, 0x5D]), "-00123.45");
        assert_eq!(value_text("P:3:0", &[0x00, 0x5F]), "005.");
        assert_eq!(value_text("Z:5:1", &[0xF0, 0xF0, 0xF1, 0xF2, 0xF3]), "0012.3");
        assert_eq!(value_text("I:10", &[0, 0, 0, 42]), "42");
        assert_eq!(value_text("F:8", &1.5f64.to_be_bytes()), "  1.500000000000E+000");
        assert_eq!(value_text("V:20:2", &[0, 2, 0x88, 0x89, 0, 0]), "'hi'");
    }
}
