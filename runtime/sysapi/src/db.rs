// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Database description APIs: `QUSLMBR`, `QUSRMBRD`, `QUSLFLD`,
//! `QUSLRCD`, `QDBRTVFD`, `QDBLDBR`, over the provider's file
//! descriptions.

use mi::Job;

use crate::param::{Args, ErrCode, Out, fixed};
use crate::provider::{ApiFail, FieldInfo, Provider};

/// The APIs of this module.
pub const APIS: &[(&str, crate::ApiFn)] = &[
    ("QUSLMBR", quslmbr),
    ("QUSRMBRD", qusrmbrd),
    ("QUSLFLD", quslfld),
    ("QUSLRCD", quslrcd),
    ("QDBRTVFD", qdbrtvfd),
    ("QDBLDBR", qdbldbr),
];

fn lib_or_libl(l: String) -> String {
    if l.is_empty() { "*LIBL".into() } else { l }
}

/// Whether a member name selects a member.
fn member_selected(pattern: &str, name: &str, first: bool) -> bool {
    match pattern {
        "*ALL" | "" => true,
        "*FIRST" => first,
        p => match p.strip_suffix('*') {
            Some(prefix) => name.starts_with(prefix),
            None => p == name,
        },
    }
}

/// `QUSLMBR(user space, format, file, member, override, errc)`:
/// `MBRL0100` (name) or `MBRL0200` (name, source type, dates, text).
fn quslmbr(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 5);
    let format = a.char(1, 8);
    let size = match format.as_str() {
        "MBRL0100" => 10,
        "MBRL0200" => 100,
        _ => {
            errc.fail(j, &ApiFail::id("CPF3C21").text(&format, 8));
            return;
        }
    };
    let (file, lib) = a.qual(2);
    let lib = lib_or_libl(lib);
    let Some(f) = p.file(&lib, &file) else {
        errc.fail(j, &ApiFail::id("CPF9812").text(&file, 10).text(&lib, 10));
        return;
    };
    let pattern = a.char(3, 10);
    let entries: Vec<Vec<u8>> = p
        .members(&lib, &file)
        .iter()
        .enumerate()
        .filter(|(i, m)| member_selected(&pattern, &m.name, *i == 0))
        .map(|(_, m)| {
            let mut e = Out::new();
            e.char(0, 10, &m.name);
            if size > 10 {
                e.char(10, 10, &m.srctype);
                e.char(20, 13, &m.created);
                e.char(33, 13, &m.changed);
                e.char(46, 50, &m.text);
                e.bin4(96, 37);
            }
            e.buf
        })
        .collect();
    let input = [
        a.bytes(0, 20),
        fixed(&format, 8),
        a.bytes(2, 20),
        fixed(&pattern, 10),
        a.bytes(4, 1),
    ]
    .concat();
    let mut header = Out::new();
    header.char(0, 10, &f.name);
    header.char(10, 10, &f.lib);
    header.char(20, 10, &f.attr);
    match crate::obj::write_list(
        j,
        &a,
        0,
        &format,
        "QUSLMBR",
        &input,
        &header.buf,
        &entries,
        size,
    ) {
        Ok(()) => errc.ok(),
        Err(f) => errc.fail(j, &f),
    }
}

/// `QUSRMBRD(receiver, len, format, file, member, override, errc)`:
/// `MBRD0100` and `MBRD0200`.
fn qusrmbrd(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 6);
    let format = a.char(2, 8);
    if !matches!(format.as_str(), "MBRD0100" | "MBRD0200") {
        errc.fail(j, &ApiFail::id("CPF3C21").text(&format, 8));
        return;
    }
    let (file, lib) = a.qual(3);
    let lib = lib_or_libl(lib);
    let Some(f) = p.file(&lib, &file) else {
        errc.fail(j, &ApiFail::id("CPF9812").text(&file, 10).text(&lib, 10));
        return;
    };
    let wanted = a.char(4, 10);
    let members = p.members(&lib, &file);
    let Some(m) = members
        .iter()
        .enumerate()
        .find(|(i, m)| member_selected(&wanted, &m.name, *i == 0) && wanted != "*ALL")
        .map(|(_, m)| m)
    else {
        errc.fail(j, &ApiFail::id("CPF3C26").text(&wanted, 10));
        return;
    };
    let mut out = Out::new();
    out.char(8, 10, &f.name);
    out.char(18, 10, &f.lib);
    out.char(28, 10, &m.name);
    out.char(38, 10, &f.attr);
    out.char(48, 10, &m.srctype);
    out.char(58, 13, &m.created);
    out.char(71, 13, &m.changed);
    out.char(84, 50, &m.text);
    out.ch(134, if f.source { '1' } else { '0' });
    out.ch(135, '0');
    out.ch(136, if f.attr == "LF" { '1' } else { '0' });
    out.ch(137, '0');
    out.blank(138, 2);
    if format == "MBRD0200" {
        out.bin4(140, i32::try_from(m.records).unwrap_or(0));
        out.bin4(144, i32::try_from(m.deleted).unwrap_or(0));
        out.bin4(148, i32::try_from(m.records * f.rec_len).unwrap_or(0));
        out.bin4(152, 0);
        out.bin4(156, i32::try_from(f.based_on.len()).unwrap_or(0));
        out.char(160, 13, &m.changed);
    }
    out.deliver(&a, 0, 1);
    errc.ok();
}

/// The `FLDL0100` entry of a field.
fn fldl0100(f: &FieldInfo) -> Vec<u8> {
    let mut e = Out::new();
    e.char(0, 10, &f.name);
    e.ch(10, f.ty);
    e.ch(11, 'B');
    e.bin4(12, i32::try_from(f.pos).unwrap_or(0));
    e.bin4(16, i32::try_from(f.pos).unwrap_or(0));
    e.bin4(20, i32::try_from(f.len).unwrap_or(0));
    e.bin4(24, i32::try_from(f.digits).unwrap_or(0));
    e.bin4(28, i32::try_from(f.decimals).unwrap_or(0));
    e.char(32, 50, &f.text);
    e.blank(82, 2);
    e.bin4(84, 0);
    e.blank(88, 64);
    e.char(152, 20, &f.headings[0]);
    e.char(172, 20, &f.headings[1]);
    e.char(192, 20, &f.headings[2]);
    e.char(212, 10, &f.name);
    e.char(222, 30, &f.name);
    e.bin4(252, i32::try_from(f.name.len()).unwrap_or(0));
    e.blank(256, 4);
    e.blank(260, 1);
    e.ch(261, '0');
    e.bin4(262, if f.ty == 'A' { 37 } else { 65_535 });
    e.pad_to(266);
    e.buf
}

/// `QUSLFLD(user space, format, file, record format, override, errc)`:
/// `FLDL0100`.
fn quslfld(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 5);
    let format = a.char(1, 8);
    if format != "FLDL0100" {
        errc.fail(j, &ApiFail::id("CPF3C21").text(&format, 8));
        return;
    }
    let (file, lib) = a.qual(2);
    let lib = lib_or_libl(lib);
    let Some(f) = p.file(&lib, &file) else {
        errc.fail(j, &ApiFail::id("CPF9812").text(&file, 10).text(&lib, 10));
        return;
    };
    let wanted = a.char(3, 10);
    let formats = p.formats(&lib, &file);
    let Some(rf) = formats
        .iter()
        .enumerate()
        .find(|(i, r)| wanted == "*FIRST" || wanted.is_empty() && *i == 0 || r.name == wanted)
        .map(|(_, r)| r)
    else {
        errc.fail(j, &ApiFail::id("CPF5C06").text(&wanted, 10));
        return;
    };
    let entries: Vec<Vec<u8>> = rf.fields.iter().map(fldl0100).collect();
    let input = [
        a.bytes(0, 20),
        fixed(&format, 8),
        a.bytes(2, 20),
        fixed(&wanted, 10),
        a.bytes(4, 1),
    ]
    .concat();
    let mut header = Out::new();
    header.char(0, 10, &f.name);
    header.char(10, 10, &f.lib);
    header.char(20, 10, &f.attr);
    header.char(30, 10, &rf.name);
    header.bin4(40, i32::try_from(rf.rec_len).unwrap_or(0));
    header.char(44, 50, &rf.text);
    match crate::obj::write_list(
        j,
        &a,
        0,
        &format,
        "QUSLFLD",
        &input,
        &header.buf,
        &entries,
        266,
    ) {
        Ok(()) => errc.ok(),
        Err(f) => errc.fail(j, &f),
    }
}

/// `QUSLRCD(user space, format, file, override, errc)`: `RCDL0100`
/// (name) or `RCDL0200` (name, record length, fields, text).
fn quslrcd(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 4);
    let format = a.char(1, 8);
    let size = match format.as_str() {
        "RCDL0100" => 10,
        "RCDL0200" => 68,
        _ => {
            errc.fail(j, &ApiFail::id("CPF3C21").text(&format, 8));
            return;
        }
    };
    let (file, lib) = a.qual(2);
    let lib = lib_or_libl(lib);
    let Some(f) = p.file(&lib, &file) else {
        errc.fail(j, &ApiFail::id("CPF9812").text(&file, 10).text(&lib, 10));
        return;
    };
    let entries: Vec<Vec<u8>> = p
        .formats(&lib, &file)
        .iter()
        .map(|r| {
            let mut e = Out::new();
            e.char(0, 10, &r.name);
            if size > 10 {
                e.bin4(10, i32::try_from(r.rec_len).unwrap_or(0));
                e.bin4(14, i32::try_from(r.fields.len()).unwrap_or(0));
                e.char(18, 50, &r.text);
            }
            e.buf
        })
        .collect();
    let input = [
        a.bytes(0, 20),
        fixed(&format, 8),
        a.bytes(2, 20),
        a.bytes(3, 1),
    ]
    .concat();
    let mut header = Out::new();
    header.char(0, 10, &f.name);
    header.char(10, 10, &f.lib);
    header.char(20, 10, &f.attr);
    match crate::obj::write_list(
        j,
        &a,
        0,
        &format,
        "QUSLRCD",
        &input,
        &header.buf,
        &entries,
        size,
    ) {
        Ok(()) => errc.ok(),
        Err(f) => errc.fail(j, &f),
    }
}

/// `QDBRTVFD(receiver, len, returned file, format, file, record
/// format, override, system, format type, errc)`: `FILD0100`, the
/// QS/400 subset: bytes returned and available, file type at 8
/// (`P`/`L`), keyed at 9, source at 10, record length at 12, format
/// count at 16, member count at 20, then the based-on files from 24.
fn qdbrtvfd(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 9);
    let format = a.char(3, 8);
    if format != "FILD0100" {
        errc.fail(j, &ApiFail::id("CPF3C21").text(&format, 8));
        return;
    }
    let (file, lib) = a.qual(4);
    let lib = lib_or_libl(lib);
    let Some(f) = p.file(&lib, &file) else {
        errc.fail(j, &ApiFail::id("CPF9812").text(&file, 10).text(&lib, 10));
        return;
    };
    let mut ret = Out::new();
    ret.char(0, 10, &f.name);
    ret.char(10, 10, &f.lib);
    a.put_bytes(2, &ret.buf);
    let formats = p.formats(&lib, &file);
    let members = p.members(&lib, &file);
    let mut out = Out::new();
    out.ch(8, if f.attr == "LF" { 'L' } else { 'P' });
    out.ch(9, if f.keyed { '1' } else { '0' });
    out.ch(10, if f.source { '1' } else { '0' });
    out.blank(11, 1);
    out.bin4(12, i32::try_from(f.rec_len).unwrap_or(0));
    out.bin4(16, i32::try_from(formats.len()).unwrap_or(0));
    out.bin4(20, i32::try_from(members.len()).unwrap_or(0));
    let mut at = 24;
    out.bin4(at, i32::try_from(f.based_on.len()).unwrap_or(0));
    at += 4;
    for (bf, bl) in &f.based_on {
        out.char(at, 10, bf);
        out.char(at + 10, 10, bl);
        at += 20;
    }
    out.deliver(&a, 0, 1);
    errc.ok();
}

/// `QDBLDBR(user space, format, file, record format, member, errc)`:
/// `DBRL0100`: dependent file, library, member and kind.
fn qdbldbr(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 5);
    let format = a.char(1, 8);
    if format != "DBRL0100" {
        errc.fail(j, &ApiFail::id("CPF3C21").text(&format, 8));
        return;
    }
    let (file, lib) = a.qual(2);
    let lib = lib_or_libl(lib);
    let Some(f) = p.file(&lib, &file) else {
        errc.fail(j, &ApiFail::id("CPF9812").text(&file, 10).text(&lib, 10));
        return;
    };
    let member = a.char(4, 10);
    let entries: Vec<Vec<u8>> = p
        .dependents(&lib, &file, &member)
        .iter()
        .map(|d| {
            let mut e = Out::new();
            e.char(0, 10, &d.name);
            e.char(10, 10, &d.lib);
            e.char(20, 10, &d.member);
            e.ch(30, d.kind);
            e.pad_to(32);
            e.buf
        })
        .collect();
    let input = [
        a.bytes(0, 20),
        fixed(&format, 8),
        a.bytes(2, 20),
        a.bytes(3, 10),
        fixed(&member, 10),
    ]
    .concat();
    let mut header = Out::new();
    header.char(0, 10, &f.name);
    header.char(10, 10, &f.lib);
    match crate::obj::write_list(
        j,
        &a,
        0,
        &format,
        "QDBLDBR",
        &input,
        &header.buf,
        &entries,
        32,
    ) {
        Ok(()) => errc.ok(),
        Err(f) => errc.fail(j, &f),
    }
}
