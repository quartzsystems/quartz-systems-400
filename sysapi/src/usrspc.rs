// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! User space and user index APIs: `QUSCRTUS`, `QUSPTRUS`, `QUSRTVUS`,
//! `QUSCHGUS`, `QUSDLTUS`, `QUSCUSAT`, `QUSRUSAT`, `QUSCRTUI`,
//! `QUSDLTUI`, `QUSADDUI`, `QUSRTVUI`, `QUSRMVUI`, `QUSRUIAT`. User
//! spaces live in the job's mapping (`mi::usrspc`); user indexes go
//! through the provider.

use mi::Job;

use crate::param::{Args, ErrCode, Out, bin4_at};
use crate::provider::{ApiFail, IdxAttr, Provider};

/// The APIs of this module.
pub const APIS: &[(&str, crate::ApiFn)] = &[
    ("QUSCRTUS", quscrtus),
    ("QUSPTRUS", qusptrus),
    ("QUSRTVUS", qusrtvus),
    ("QUSCHGUS", quschgus),
    ("QUSDLTUS", qusdltus),
    ("QUSCUSAT", quscusat),
    ("QUSRUSAT", qusrusat),
    ("QUSCRTUI", quscrtui),
    ("QUSDLTUI", qusdltui),
    ("QUSADDUI", qusaddui),
    ("QUSRTVUI", qusrtvui),
    ("QUSRMVUI", qusrmvui),
    ("QUSRUIAT", qusruiat),
];

/// The library a user space is in, by name and `*LIBL`.
fn locate(j: &mut Job, name: &str, lib: &str) -> Result<String, ApiFail> {
    let lib = if lib.is_empty() { "*LIBL" } else { lib };
    j.usrspcs
        .library_of(name, lib)
        .ok_or_else(|| ApiFail::not_found(name, lib))
}

/// Map a user space and return its library.
fn mapped(j: &mut Job, name: &str, lib: &str) -> Result<String, ApiFail> {
    let lib = locate(j, name, lib)?;
    j.map_user_space(name, &lib)
        .map_err(|_| ApiFail::not_found(name, &lib))?;
    Ok(lib)
}

/// `QUSCRTUS(name, attribute, size, initial value, authority, text,
/// replace, errc, domain, transfer size, alignment)`.
fn quscrtus(j: &mut Job, args: &[u64], _p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 7);
    let (name, lib) = a.qual(0);
    let size = usize::try_from(a.bin4(2)).unwrap_or(0);
    let init = a.bytes(3, 1).first().copied().unwrap_or(0);
    let text = a.char(5, 50);
    let replace = a.char(6, 10) == "*YES";
    let lib = if lib.is_empty() {
        "*CURLIB".into()
    } else {
        lib
    };
    if replace && let Some(found) = j.usrspcs.library_of(&name, &lib) {
        j.usrspcs.delete(&name, &found, &mut j.spaces);
    }
    match j.usrspcs.create(&name, &lib, size, init, &text) {
        Ok(()) => errc.ok(),
        Err(id) => errc.fail(j, &ApiFail::id(id).text(&name, 10).text(&lib, 10)),
    }
}

/// `QUSPTRUS(name, pointer out, errc)`.
fn qusptrus(j: &mut Job, args: &[u64], _p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 2);
    let (name, lib) = a.qual(0);
    let lib = match locate(j, &name, &lib) {
        Ok(l) => l,
        Err(f) => {
            errc.fail(j, &f);
            return;
        }
    };
    match j.map_user_space(&name, &lib) {
        Ok(p) => {
            a.put_bytes(1, &p.to_bytes());
            errc.ok();
        }
        Err(_) => errc.fail(j, &ApiFail::not_found(&name, &lib)),
    }
}

/// `QUSRTVUS(name, start, length, receiver, errc)`: `start` 1-based.
fn qusrtvus(j: &mut Job, args: &[u64], _p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 4);
    let (name, lib) = a.qual(0);
    let start = usize::try_from(a.bin4(1)).unwrap_or(0);
    let len = usize::try_from(a.bin4(2)).unwrap_or(0);
    let lib = match mapped(j, &name, &lib) {
        Ok(l) => l,
        Err(f) => {
            errc.fail(j, &f);
            return;
        }
    };
    let data = j.usrspcs.mapped(&name, &lib).unwrap_or(&[]);
    if start == 0 || start - 1 + len > data.len() {
        errc.fail(j, &ApiFail::id("CPF3C14").bin4(a.bin4(2)));
        return;
    }
    let slice = data[start - 1..start - 1 + len].to_vec();
    a.put_bytes(3, &slice);
    errc.ok();
}

/// `QUSCHGUS(name, start, length, data, force, errc)`: the space grows
/// when the change passes its end.
fn quschgus(j: &mut Job, args: &[u64], _p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 5);
    let (name, lib) = a.qual(0);
    let start = usize::try_from(a.bin4(1)).unwrap_or(0);
    let len = usize::try_from(a.bin4(2)).unwrap_or(0);
    let data = a.bytes(3, len);
    let lib = match mapped(j, &name, &lib) {
        Ok(l) => l,
        Err(f) => {
            errc.fail(j, &f);
            return;
        }
    };
    if start == 0 {
        errc.fail(j, &ApiFail::id("CPF3C14").bin4(a.bin4(1)));
        return;
    }
    let Some(space) = j.usrspcs.mapped_mut(&name, &lib) else {
        errc.fail(j, &ApiFail::not_found(&name, &lib));
        return;
    };
    let end = start - 1 + len;
    if space.len() < end {
        // Growing moves the bytes, so the mapping is redone from the
        // object: pointers handed out before die.
        let mut copy = space.clone();
        copy.resize(end, 0);
        copy[start - 1..end].copy_from_slice(&data);
        j.usrspcs.unmap(&name, &lib, &mut j.spaces);
        let _ = j.usrspcs.delete(&name, &lib, &mut j.spaces);
        if let Err(id) = j.usrspcs.create(&name, &lib, copy.len(), 0, "") {
            errc.fail(j, &ApiFail::id(id).text(&name, 10).text(&lib, 10));
            return;
        }
        if j.map_user_space(&name, &lib).is_ok()
            && let Some(s) = j.usrspcs.mapped_mut(&name, &lib)
        {
            s.copy_from_slice(&copy);
        }
    } else {
        space[start - 1..end].copy_from_slice(&data);
    }
    j.sync_user_spaces();
    errc.ok();
}

/// `QUSDLTUS(name, errc)`.
fn qusdltus(j: &mut Job, args: &[u64], _p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 1);
    let (name, lib) = a.qual(0);
    let lib = match locate(j, &name, &lib) {
        Ok(l) => l,
        Err(f) => {
            errc.fail(j, &f);
            return;
        }
    };
    if j.usrspcs.delete(&name, &lib, &mut j.spaces) {
        errc.ok();
    } else {
        errc.fail(j, &ApiFail::not_found(&name, &lib));
    }
}

/// `QUSCUSAT(returned lib, name, attributes, errc)`: key 1 sets the
/// size; keys 2 (initial value) and 3 (auto-extend) are accepted.
fn quscusat(j: &mut Job, args: &[u64], _p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 3);
    let (name, lib) = a.qual(1);
    let lib = match mapped(j, &name, &lib) {
        Ok(l) => l,
        Err(f) => {
            errc.fail(j, &f);
            return;
        }
    };
    let head = a.bytes(2, 4);
    let n = usize::try_from(bin4_at(&head, 0)).unwrap_or(0);
    let recs = a.bytes(2, 4 + n * 12);
    let mut at = 4;
    for _ in 0..n {
        let key = bin4_at(&recs, at);
        let len = usize::try_from(bin4_at(&recs, at + 4)).unwrap_or(0);
        let data = recs.get(at + 8..at + 8 + len).unwrap_or(&[]).to_vec();
        match key {
            1 => {
                let size = usize::try_from(bin4_at(&data, 0)).unwrap_or(0);
                let copy = j
                    .usrspcs
                    .mapped(&name, &lib)
                    .map(<[u8]>::to_vec)
                    .unwrap_or_default();
                let mut copy = copy;
                copy.resize(size.max(1), 0);
                j.usrspcs.unmap(&name, &lib, &mut j.spaces);
                let _ = j.usrspcs.delete(&name, &lib, &mut j.spaces);
                if let Err(id) = j.usrspcs.create(&name, &lib, copy.len(), 0, "") {
                    errc.fail(j, &ApiFail::id(id).text(&name, 10).text(&lib, 10));
                    return;
                }
                if j.map_user_space(&name, &lib).is_ok()
                    && let Some(s) = j.usrspcs.mapped_mut(&name, &lib)
                {
                    s.copy_from_slice(&copy);
                }
                j.sync_user_spaces();
            }
            2 | 3 => {}
            k => {
                errc.fail(j, &ApiFail::id("CPF3C4D").bin4(k));
                return;
            }
        }
        at += 8 + len;
    }
    a.put_char(0, 10, &lib);
    errc.ok();
}

/// `QUSRUSAT(receiver, len, format, name, errc)`: `SPCA0100`.
fn qusrusat(j: &mut Job, args: &[u64], _p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 4);
    let format = a.char(2, 8);
    if format != "SPCA0100" {
        errc.fail(j, &ApiFail::id("CPF3C21").text(&format, 8));
        return;
    }
    let (name, lib) = a.qual(3);
    let lib = match mapped(j, &name, &lib) {
        Ok(l) => l,
        Err(f) => {
            errc.fail(j, &f);
            return;
        }
    };
    let size = j.usrspcs.mapped(&name, &lib).map_or(0, <[u8]>::len);
    let mut out = Out::new();
    out.bin4(8, i32::try_from(size).unwrap_or(i32::MAX));
    out.ch(12, '1');
    out.u8(13, 0);
    out.char(14, 10, &lib);
    out.deliver(&a, 0, 1);
    errc.ok();
}

/// `QUSCRTUI(name, attribute, entry length type, entry length, key
/// insertion, key length, immediate update, optimization, authority,
/// text, replace, errc)`.
fn quscrtui(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 11);
    let (name, lib) = a.qual(0);
    let attr = IdxAttr {
        fixed: a.char(2, 1) != "V",
        entry_len: u32::try_from(a.bin4(3)).unwrap_or(0),
        keyed: a.char(4, 1) == "1",
        key_len: u32::try_from(a.bin4(5)).unwrap_or(0),
        immediate: a.char(6, 1) == "1",
        sequential: a.char(7, 1) == "1",
        text: a.char(9, 50),
    };
    if attr.entry_len == 0 || attr.entry_len > 2000 {
        errc.fail(j, &ApiFail::id("CPF3C14").bin4(a.bin4(3)));
        return;
    }
    if attr.keyed && (attr.key_len == 0 || attr.key_len > attr.entry_len) {
        errc.fail(j, &ApiFail::id("CPF3C14").bin4(a.bin4(5)));
        return;
    }
    if a.char(10, 10) == "*YES" {
        let _ = p.usridx_delete(&lib, &name);
    }
    match p.usridx_create(&lib, &name, attr) {
        Ok(()) => errc.ok(),
        Err(f) => errc.fail(j, &f),
    }
}

/// `QUSDLTUI(name, errc)`.
fn qusdltui(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 1);
    let (name, lib) = a.qual(0);
    match p.usridx_delete(&lib, &name) {
        Ok(()) => errc.ok(),
        Err(f) => errc.fail(j, &f),
    }
}

/// `QUSADDUI(name, returned count, entry lengths, insert type,
/// entries, entry length, count, errc)`: insert type `1` fails on a
/// duplicate key, `2` replaces it.
fn qusaddui(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 7);
    let (name, lib) = a.qual(0);
    let (attr, _, _) = match p.usridx_attrs(&lib, &name) {
        Ok(x) => x,
        Err(f) => {
            errc.fail(j, &f);
            return;
        }
    };
    let n = usize::try_from(a.bin4(6)).unwrap_or(0);
    let replace = a.char(3, 1) == "2";
    let entries: Vec<Vec<u8>> = if attr.fixed {
        let len = usize::try_from(a.bin4(5)).unwrap_or(0);
        a.bytes(4, len * n)
            .chunks(len.max(1))
            .map(<[u8]>::to_vec)
            .collect()
    } else {
        let lens: Vec<usize> = a
            .bytes(2, 4 * n)
            .chunks(4)
            .map(|c| usize::try_from(bin4_at(c, 0)).unwrap_or(0))
            .collect();
        let total: usize = lens.iter().sum();
        let all = a.bytes(4, total);
        let mut at = 0;
        lens.iter()
            .map(|l| {
                let e = all[at..at + l].to_vec();
                at += l;
                e
            })
            .collect()
    };
    match p.usridx_add(&lib, &name, entries, replace) {
        Ok(k) => {
            a.put_bin4(1, i32::try_from(k).unwrap_or(i32::MAX));
            errc.ok();
        }
        Err(f) => errc.fail(j, &f),
    }
}

/// Find or remove: `(receiver, receiver length, entry lengths out,
/// entry lengths length, returned count out, name, rule, key, key
/// length, key2?, errc)`; rule 8 takes two keys of `key length` each.
fn find_entries(j: &mut Job, args: &[u64], p: &mut dyn Provider, remove: bool) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 9);
    let (name, lib) = a.qual(5);
    let rule = u32::try_from(a.bin4(6)).unwrap_or(0);
    let kl = usize::try_from(a.bin4(8)).unwrap_or(0);
    let keys = a.bytes(7, if rule == 8 { 2 * kl } else { kl });
    let (key, key2) = keys.split_at(kl.min(keys.len()));
    let room = usize::try_from(a.bin4(1)).unwrap_or(0);
    let lens_room = usize::try_from(a.bin4(3)).unwrap_or(0) / 4;
    let (attr, _, _) = match p.usridx_attrs(&lib, &name) {
        Ok(x) => x,
        Err(f) => {
            errc.fail(j, &f);
            return;
        }
    };
    let max = if attr.fixed {
        room / (attr.entry_len as usize).max(1)
    } else {
        lens_room
    }
    .max(1);
    match p.usridx_find(&lib, &name, rule, key, key2, max, remove) {
        Ok(found) => {
            let mut data = Out::new();
            let mut lens = Out::new();
            let mut at = 0;
            for (i, e) in found.iter().enumerate() {
                let mut e = e.clone();
                if attr.fixed {
                    e.resize(attr.entry_len as usize, 0x40);
                }
                data.bytes(at, &e);
                at += e.len();
                lens.bin4(4 * i, i32::try_from(e.len()).unwrap_or(0));
            }
            data.deliver_raw(&a, 0, room);
            lens.deliver_raw(&a, 2, lens_room * 4);
            a.put_bin4(4, i32::try_from(found.len()).unwrap_or(0));
            errc.ok();
        }
        Err(f) => errc.fail(j, &f),
    }
}

/// `QUSRTVUI`.
fn qusrtvui(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    find_entries(j, args, p, false);
}

/// `QUSRMVUI`.
fn qusrmvui(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    find_entries(j, args, p, true);
}

/// `QUSRUIAT(receiver, len, format, name, errc)`: `IDXA0100`.
fn qusruiat(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 4);
    let format = a.char(2, 8);
    if format != "IDXA0100" {
        errc.fail(j, &ApiFail::id("CPF3C21").text(&format, 8));
        return;
    }
    let (name, lib) = a.qual(3);
    match p.usridx_attrs(&lib, &name) {
        Ok((attr, count, found)) => {
            let mut out = Out::new();
            out.bin4(8, 0);
            out.ch(12, if attr.fixed { 'F' } else { 'V' });
            out.bin4(13, i32::try_from(attr.entry_len).unwrap_or(0));
            out.ch(17, if attr.keyed { '1' } else { '0' });
            out.bin4(18, i32::try_from(attr.key_len).unwrap_or(0));
            out.ch(22, if attr.immediate { '1' } else { '0' });
            out.ch(23, if attr.sequential { '1' } else { '0' });
            out.bin4(24, i32::try_from(count).unwrap_or(0));
            out.bin4(28, 0);
            out.char(32, 10, &found);
            out.char(42, 50, &attr.text);
            out.deliver(&a, 0, 1);
            errc.ok();
        }
        Err(f) => errc.fail(j, &f),
    }
}
