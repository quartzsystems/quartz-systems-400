// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Data queue and data area APIs: `QSNDDTAQ`, `QRCVDTAQ`, `QCLRDTAQ`,
//! `QMHQRDQD`, `QMHRDQM`, `QWCRDTAA`, `QXXCHGDA`, `QXXRTVDA`. The
//! data queue APIs take packed-decimal lengths, as CL passes them.

use mi::Job;

use crate::param::{Args, ErrCode, Out};
use crate::provider::{ApiFail, DataAreaType, Provider};

/// The APIs of this module.
pub const APIS: &[(&str, crate::ApiFn)] = &[
    ("QSNDDTAQ", qsnddtaq),
    ("QRCVDTAQ", qrcvdtaq),
    ("QCLRDTAQ", qclrdtaq),
    ("QMHQRDQD", qmhqrdqd),
    ("QMHRDQM", qmhrdqm),
    ("QWCRDTAA", qwcrdtaa),
    ("QXXCHGDA", qxxchgda),
    ("QXXRTVDA", qxxrtvda),
];

fn lib_or_libl(l: String) -> String {
    if l.is_empty() { "*LIBL".into() } else { l }
}

/// `QSNDDTAQ(name, library, length, data, [key length, key, async,
/// journal])`: errors are escapes.
fn qsnddtaq(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let name = a.char(0, 10);
    let lib = lib_or_libl(a.char(1, 10));
    let len = usize::try_from(a.packed(2, 5, 0)).unwrap_or(0);
    let data = a.bytes(3, len);
    let kl = usize::try_from(a.packed(4, 3, 0)).unwrap_or(0);
    let key = a.bytes(5, kl);
    let sender = format!("{}/{}/{}", j.ident.number, j.ident.user, j.ident.name);
    if let Err(f) = p.dtaq_send(&lib, &name, &key, &data, &sender) {
        ErrCode::none().fail(j, &f);
    }
}

/// `QRCVDTAQ(name, library, length out, data out, wait, [key order,
/// key length, key, sender length, sender out, remove, size, errc])`.
fn qrcvdtaq(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 12);
    let name = a.char(0, 10);
    let lib = lib_or_libl(a.char(1, 10));
    let order = a.char(5, 2);
    let kl = usize::try_from(a.packed(6, 3, 0)).unwrap_or(0);
    let key = a.bytes(7, kl);
    let remove = !a.present(10) || a.char(10, 10) != "*NO";
    let room = if a.present(11) {
        usize::try_from(a.packed(11, 5, 0)).unwrap_or(0)
    } else {
        usize::MAX
    };
    match p.dtaq_receive(&lib, &name, &order, &key, remove) {
        Ok(Some(e)) => {
            let n = e.data.len().min(room);
            a.put_packed(2, 5, 0, i64::try_from(n).unwrap_or(0));
            a.put_bytes(3, &e.data[..n]);
            if a.present(7) && kl > 0 {
                let mut k = e.key.clone();
                k.resize(kl, 0x40);
                a.put_bytes(7, &k);
            }
            if a.present(9) {
                let sl = usize::try_from(a.packed(8, 3, 0)).unwrap_or(0);
                let mut s = e.sender.clone();
                s.resize(sl, 0x40);
                a.put_bytes(9, &s);
            }
            errc.ok();
        }
        Ok(None) => {
            a.put_packed(2, 5, 0, 0);
            errc.ok();
        }
        Err(f) => errc.fail(j, &f),
    }
}

/// `QCLRDTAQ(name, library, [key order, key length, key, errc])`.
fn qclrdtaq(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 5);
    let name = a.char(0, 10);
    let lib = lib_or_libl(a.char(1, 10));
    let order = a.char(2, 2);
    let kl = usize::try_from(a.packed(3, 3, 0)).unwrap_or(0);
    let key = a.bytes(4, kl);
    match p.dtaq_clear(&lib, &name, &order, &key) {
        Ok(()) => errc.ok(),
        Err(f) => errc.fail(j, &f),
    }
}

/// `QMHQRDQD(receiver, len, format, queue, errc)`: `RDQD0100`.
fn qmhqrdqd(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 4);
    let format = a.char(2, 8);
    if format != "RDQD0100" {
        errc.fail(j, &ApiFail::id("CPF3C21").text(&format, 8));
        return;
    }
    let (name, lib) = a.qual(3);
    let lib = lib_or_libl(lib);
    match p.dtaq_describe(&lib, &name) {
        Ok(d) => {
            let mut out = Out::new();
            out.bin4(8, i32::try_from(d.max_len).unwrap_or(0));
            out.bin4(12, i32::try_from(d.key_len).unwrap_or(0));
            out.ch(16, d.sequence);
            out.ch(17, if d.sender_id { 'Y' } else { 'N' });
            out.ch(18, 'N');
            out.char(19, 50, &d.text);
            out.ch(69, '0');
            out.ch(70, 'N');
            out.blank(71, 1);
            out.bin4(72, i32::try_from(d.entries).unwrap_or(0));
            out.bin4(76, i32::try_from(d.allocated).unwrap_or(0));
            out.char(80, 10, &d.name);
            out.char(90, 10, &d.lib);
            out.deliver(&a, 0, 1);
            errc.ok();
        }
        Err(f) => errc.fail(j, &f),
    }
}

/// `QMHRDQM(receiver, len, format, queue, selection, selection
/// length, errc)`: `RDQM0100`, every entry: a count at 8, then
/// records of length, key length, key and data, each preceded by its
/// offset to the next.
fn qmhrdqm(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 6);
    let format = a.char(2, 8);
    if format != "RDQM0100" {
        errc.fail(j, &ApiFail::id("CPF3C21").text(&format, 8));
        return;
    }
    let (name, lib) = a.qual(3);
    let lib = lib_or_libl(lib);
    match p.dtaq_entries(&lib, &name) {
        Ok(entries) => {
            let mut out = Out::new();
            out.bin4(8, i32::try_from(entries.len()).unwrap_or(0));
            out.bin4(12, 16);
            let mut at = 16;
            for e in &entries {
                let rec = 16 + e.key.len() + e.data.len();
                out.bin4(at, i32::try_from(at + rec).unwrap_or(0));
                out.bin4(at + 4, i32::try_from(e.data.len()).unwrap_or(0));
                out.bin4(at + 8, i32::try_from(e.key.len()).unwrap_or(0));
                out.bin4(at + 12, i32::try_from(e.sender.len()).unwrap_or(0));
                out.bytes(at + 16, &e.key);
                out.bytes(at + 16 + e.key.len(), &e.data);
                at += rec;
            }
            out.deliver(&a, 0, 1);
            errc.ok();
        }
        Err(f) => errc.fail(j, &f),
    }
}

/// The receiver image of a data area: type, library, length, decimal
/// positions and the value from `start`.
fn dtaara_image(d: &crate::provider::DataArea, start: usize, len: usize) -> Out {
    let mut out = Out::new();
    out.char(8, 10, d.ty.name());
    out.char(18, 10, &d.lib);
    out.bin4(28, i32::try_from(d.len).unwrap_or(0));
    out.bin4(32, i32::try_from(d.decimals).unwrap_or(0));
    let v = d.value.get(start.saturating_sub(1)..).unwrap_or(&[]);
    let v = &v[..v.len().min(len)];
    out.bytes(36, v);
    out
}

/// `QWCRDTAA(receiver, len, data area, start, length, errc)`: `start`
/// -1 and length -1 read the whole value.
fn qwcrdtaa(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 5);
    let (name, lib) = a.qual(2);
    let lib = lib_or_libl(lib);
    let start = a.bin4(3);
    let len = a.bin4(4);
    match p.dtaara_read(&lib, &name) {
        Ok(d) => {
            let (s, l) = if start == -1 || d.ty != DataAreaType::Char {
                (1, d.value.len())
            } else {
                (
                    usize::try_from(start).unwrap_or(1),
                    usize::try_from(len).unwrap_or(0),
                )
            };
            if s == 0 || s - 1 + l > d.value.len() {
                errc.fail(j, &ApiFail::id("CPF1089").text(&name, 10));
                return;
            }
            dtaara_image(&d, s, l).deliver(&a, 0, 1);
            errc.ok();
        }
        Err(f) => errc.fail(j, &f),
    }
}

/// `QXXCHGDA(data area, start, length, value)`: errors are escapes.
fn qxxchgda(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let (name, lib) = a.qual(0);
    let lib = lib_or_libl(lib);
    let start = usize::try_from(a.bin4(1)).unwrap_or(0);
    let len = usize::try_from(a.bin4(2)).unwrap_or(0);
    let value = a.bytes(3, len);
    if let Err(f) = p.dtaara_write(&lib, &name, start, &value) {
        ErrCode::none().fail(j, &f);
    }
}

/// `QXXRTVDA(receiver, length, data area, start, length)`: the value
/// only; errors are escapes.
fn qxxrtvda(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let (name, lib) = a.qual(2);
    let lib = lib_or_libl(lib);
    let start = usize::try_from(a.bin4(3)).unwrap_or(1).max(1);
    let len = usize::try_from(a.bin4(4)).unwrap_or(0);
    let room = usize::try_from(a.bin4(1)).unwrap_or(0);
    match p.dtaara_read(&lib, &name) {
        Ok(d) => {
            let v = d.value.get(start - 1..).unwrap_or(&[]);
            let n = v.len().min(len).min(room);
            a.put_bytes(0, &v[..n]);
        }
        Err(f) => ErrCode::none().fail(j, &f),
    }
}
