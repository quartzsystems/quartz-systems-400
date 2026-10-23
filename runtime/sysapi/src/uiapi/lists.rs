// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The list APIs: entries are added, inserted, updated, read and
//! removed by entry handle (`Char(4)`, binary), the list's current
//! entry moving with them; `QUISETLA` says whether the list is
//! complete (`ALL`) or more entries can follow (`MORE`) and where the
//! display starts; `QUIGETLA` reports it back (`LIST0100`).

use mi::Job;
use uim::engine::UimList;

use super::{App, SharedUimHost, fail, read_record, with_app, write_record};
use crate::param::{Args, ErrCode, Out};
use crate::provider::ApiFail;

/// A handle parameter: four bytes, big-endian.
fn handle_of(a: &Args<'_>, i: usize) -> u32 {
    u32::from_be_bytes(a.bytes(i, 4).try_into().unwrap_or([0; 4]))
}

/// The list's definition variables, or `CPF6A91`.
fn list_vars(app: &App, list: &str) -> Result<Vec<String>, ApiFail> {
    app.engine
        .group
        .list_def(&list.to_ascii_uppercase())
        .map(|d| d.vars.clone())
        .ok_or_else(|| fail("CPF6A91", Some(app), list, ""))
}

/// The list, or `CPF6A91` when the program never made it.
fn existing<'a>(app: &'a mut App, list: &str) -> Result<&'a mut UimList, ApiFail> {
    if app.engine.list_ref(list).is_none() {
        return Err(fail("CPF6A91", Some(app), list, ""));
    }
    let l = app.engine.list_mut(list);
    l.sync_handles();
    Ok(l)
}

/// The variable of the list's option column, if it has one.
fn option_var(app: &App, list: &str) -> Option<String> {
    let g = &app.engine.group;
    g.list_def(&list.to_ascii_uppercase())?
        .vars
        .iter()
        .find(|v| {
            g.variable(v)
                .and_then(|x| g.class(&x.class))
                .is_some_and(|c| c.basetype == uim::model::BaseType::Action)
        })
        .cloned()
}

/// A new entry from a record in a buffer: the list's variables, those
/// the record holds filled.
fn entry(app: &App, list: &str, rec: &str, buf: &[u8]) -> Result<Vec<(String, String)>, ApiFail> {
    let vars = list_vars(app, list)?;
    let given = read_record(app, rec, buf)?;
    Ok(vars
        .into_iter()
        .map(|v| {
            let val = given
                .iter()
                .find(|(g, _)| *g == v)
                .map(|(_, x)| x.clone())
                .unwrap_or_default();
            (v, val)
        })
        .collect())
}

/// Where an entry goes: `FRST`, `LAST`, after (`NEXT`) or before
/// (`PREV`) the entry with `handle` (0: the current entry).
fn place(app: &App, l: &UimList, list: &str, option: &str, handle: u32) -> Result<usize, ApiFail> {
    let at = |h: u32| -> Result<Option<usize>, ApiFail> {
        if h == 0 {
            return Ok(l.current);
        }
        l.by_handle(h)
            .map(Some)
            .ok_or_else(|| fail("CPF6A9C", Some(app), list, ""))
    };
    Ok(match option {
        "FRST" => 0,
        "LAST" | "" => l.rows.len(),
        "NEXT" => at(handle)?.map_or(l.rows.len(), |i| i + 1),
        "PREV" | "BEFR" => at(handle)?.unwrap_or(0),
        _ => return Err(fail("CPF6A2B", Some(app), list, "")),
    })
}

/// `QUIADDLE(handle, buffer, buffer length, variable record, list,
/// option, entry handle in/out, errc)`: add an entry `FRST`, `LAST`,
/// `NEXT` or `PREV` to the entry handle (0 for the current one); the
/// new entry's handle comes back and it becomes current.
pub(super) fn quiaddle(j: &mut Job, args: &[u64], _host: &SharedUimHost) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 7);
    add(j, &a, errc, 5, 6);
}

/// `QUIINSLE(handle, buffer, buffer length, variable record, list,
/// entry handle in/out, errc)`: insert an entry before the entry with
/// the handle (0 for the top); its handle comes back.
pub(super) fn quiinsle(j: &mut Job, args: &[u64], _host: &SharedUimHost) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 6);
    add(j, &a, errc, usize::MAX, 5);
}

fn add(j: &mut Job, a: &Args<'_>, errc: ErrCode, opt_at: usize, handle_at: usize) {
    let len = usize::try_from(a.bin4(2)).unwrap_or(0);
    let buf = a.bytes(1, len);
    let rec = a.char(3, 10);
    let list = a.char(4, 10).to_ascii_uppercase();
    let option = if opt_at == usize::MAX {
        "PREV".to_string()
    } else {
        a.char(opt_at, 4)
    };
    let rel = handle_of(a, handle_at);
    let r = with_app(j, &a.bytes(0, 8), |_, app| {
        let row = entry(app, &list, &rec, &buf)?;
        let mut l = std::mem::take(app.engine.list_mut(&list));
        l.sync_handles();
        let at = if opt_at == usize::MAX && rel == 0 {
            Ok(0)
        } else {
            place(app, &l, &list, &option, rel)
        };
        let r = at.map(|at| {
            let h = l.insert(at, row);
            l.current = Some(at.min(l.rows.len() - 1));
            h
        });
        *app.engine.list_mut(&list) = l;
        r
    });
    match r {
        Ok(h) => {
            a.put_bytes(handle_at, &h.to_be_bytes());
            errc.ok();
        }
        Err(f) => errc.fail(j, &f),
    }
}

/// `QUIUPDLE(handle, buffer, buffer length, variable record, list,
/// option, entry handle in/out, errc)`: update the current entry
/// (`SAME`) or the one with the handle (`HNDL`) from the record.
pub(super) fn quiupdle(j: &mut Job, args: &[u64], _host: &SharedUimHost) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 7);
    let len = usize::try_from(a.bin4(2)).unwrap_or(0);
    let buf = a.bytes(1, len);
    let rec = a.char(3, 10);
    let list = a.char(4, 10).to_ascii_uppercase();
    let option = a.char(5, 4);
    let h = handle_of(&a, 6);
    let r = with_app(j, &a.bytes(0, 8), |_, app| {
        let given = read_record(app, &rec, &buf)?;
        let snapshot = existing(app, &list)?.clone();
        let index = match option.as_str() {
            "SAME" | "" => snapshot
                .current
                .ok_or_else(|| fail("CPF6A93", Some(app), &list, ""))?,
            "HNDL" => snapshot
                .by_handle(h)
                .ok_or_else(|| fail("CPF6A9C", Some(app), &list, ""))?,
            _ => return Err(fail("CPF6A28", Some(app), &list, "")),
        };
        let l = app.engine.list_mut(&list);
        let row = &mut l.rows[index];
        for (v, val) in given {
            match row.iter_mut().find(|(x, _)| *x == v) {
                Some(cell) => cell.1 = val,
                None => row.push((v, val)),
            }
        }
        l.current = Some(index);
        Ok(l.handles[index])
    });
    match r {
        Ok(h) => {
            a.put_bytes(6, &h.to_be_bytes());
            errc.ok();
        }
        Err(f) => errc.fail(j, &f),
    }
}

/// `QUIGETLE(handle, buffer out, buffer length, variable record, list,
/// positioning, copy option, selection criteria, selection handle,
/// extend option, entry handle out, errc)`: the `FRST`, `LAST`,
/// `NEXT`, `PREV` or `SAME` entry, or the one with the selection
/// handle (`HNDL`), among all entries (`ALL` or blank criteria) or
/// those with an option typed (`ACTION`). With copy option `Y` its
/// variables (the typed option in the option column) go into the
/// buffer.
pub(super) fn quigetle(j: &mut Job, args: &[u64], _host: &SharedUimHost) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 11);
    let len = usize::try_from(a.bin4(2)).unwrap_or(0);
    let rec = a.char(3, 10);
    let list = a.char(4, 10).to_ascii_uppercase();
    let pos = a.char(5, 4);
    let copy = a.char(6, 1);
    let criteria = a.char(7, 20).to_ascii_uppercase();
    let sel = handle_of(&a, 8);
    let r = with_app(j, &a.bytes(0, 8), |_, app| {
        if !matches!(copy.as_str(), "Y" | "N" | "") {
            return Err(fail("CPF6A4C", Some(app), &list, ""));
        }
        let only_actions = match criteria.as_str() {
            "" | "ALL" => false,
            "ACTION" => true,
            _ => return Err(fail("CPF6A2D", Some(app), &list, "")),
        };
        let l = existing(app, &list)?.clone();
        let wanted = |i: &usize| !only_actions || l.options.contains_key(i);
        let found = match pos.as_str() {
            "FRST" => (0..l.rows.len()).find(wanted),
            "LAST" => (0..l.rows.len()).rev().find(wanted),
            "NEXT" => match l.current {
                Some(c) => (c + 1..l.rows.len()).find(wanted),
                None => (0..l.rows.len()).find(wanted),
            },
            "PREV" => l.current.and_then(|c| (0..c).rev().find(wanted)),
            "SAME" => l.current.filter(wanted),
            "HNDL" => l.by_handle(sel).filter(wanted),
            _ => return Err(fail("CPF6A2C", Some(app), &list, "")),
        };
        let index = found.ok_or_else(|| fail("CPF6A98", Some(app), &list, ""))?;
        app.engine.list_mut(&list).current = Some(index);
        let opt = option_var(app, &list);
        let row = l.rows[index].clone();
        let typed = l.options.get(&index).cloned();
        let value = |v: &str| -> String {
            if opt.as_deref() == Some(v)
                && let Some(t) = &typed
            {
                return t.clone();
            }
            row.iter()
                .find(|(x, _)| x == v)
                .map(|(_, x)| x.clone())
                .unwrap_or_default()
        };
        let bytes = if copy == "N" {
            None
        } else {
            Some(write_record(app, &rec, len, &value)?)
        };
        Ok((bytes, l.handles[index]))
    });
    match r {
        Ok((bytes, h)) => {
            if let Some(b) = bytes {
                a.put_bytes(1, &b);
            }
            a.put_bytes(10, &h.to_be_bytes());
            errc.ok();
        }
        Err(f) => errc.fail(j, &f),
    }
}

/// `QUIRMVLE(handle, list, extend option, entry handle in/out, errc)`:
/// remove the entry with the handle (0: the current one); the handle
/// of the entry before it comes back (0 at the top) and that entry
/// becomes current.
pub(super) fn quirmvle(j: &mut Job, args: &[u64], _host: &SharedUimHost) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 4);
    let list = a.char(1, 10).to_ascii_uppercase();
    let h = handle_of(&a, 3);
    let r = with_app(j, &a.bytes(0, 8), |_, app| {
        let snapshot = existing(app, &list)?.clone();
        let index = if h == 0 {
            snapshot.current
        } else {
            snapshot.by_handle(h)
        }
        .ok_or_else(|| fail("CPF6A9C", Some(app), &list, ""))?;
        let l = app.engine.list_mut(&list);
        l.remove(index);
        let before = index.checked_sub(1);
        l.current = before;
        Ok(before.map_or(0, |b| l.handles[b]))
    });
    match r {
        Ok(h) => {
            a.put_bytes(3, &h.to_be_bytes());
            errc.ok();
        }
        Err(f) => errc.fail(j, &f),
    }
}

/// `QUIDLTL(handle, list, errc)`: delete a list.
pub(super) fn quidltl(j: &mut Job, args: &[u64], _host: &SharedUimHost) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 2);
    let list = a.char(1, 10).to_ascii_uppercase();
    let r = with_app(j, &a.bytes(0, 8), |_, app| {
        if app.engine.delete_list(&list) {
            Ok(())
        } else {
            Err(fail("CPF6A91", Some(app), &list, ""))
        }
    });
    match r {
        Ok(()) => errc.ok(),
        Err(f) => errc.fail(j, &f),
    }
}

/// `QUISETLA(handle, list, list contents, display position, trim,
/// errc)`: contents `ALL` (complete), `MORE` (more entries may follow:
/// paging past the last one returns from `QUIDSPP`) or `SAME`; display
/// position `TOP`, `BOT`, `SAME` or an entry handle.
pub(super) fn quisetla(j: &mut Job, args: &[u64], _host: &SharedUimHost) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 5);
    let list = a.char(1, 10).to_ascii_uppercase();
    let contents = a.char(2, 4);
    let raw_pos = a.bytes(3, 4);
    let pos = ccsid::decode_trimmed(&raw_pos);
    let trim = a.char(4, 1);
    let r = with_app(j, &a.bytes(0, 8), |_, app| {
        list_vars(app, &list)?;
        if !matches!(trim.as_str(), "Y" | "N" | "") {
            return Err(fail("CPF6A9E", Some(app), &list, ""));
        }
        let complete = match contents.as_str() {
            "ALL" | "ALLX" => Some(true),
            "MORE" => Some(false),
            "SAME" | "" => None,
            _ => return Err(fail("CPF6A90", Some(app), &list, "")),
        };
        let snapshot = {
            let l = app.engine.list_mut(&list);
            l.sync_handles();
            l.clone()
        };
        let top = match pos.as_str() {
            "TOP" => Some(0),
            "BOT" => Some(snapshot.rows.len().saturating_sub(1)),
            "SAME" | "" => None,
            _ => {
                let h = u32::from_be_bytes(raw_pos.clone().try_into().unwrap_or([0; 4]));
                let i = snapshot
                    .by_handle(h)
                    .ok_or_else(|| fail("CPF6A9C", Some(app), &list, ""))?;
                Some(snapshot.visible().iter().position(|x| *x == i).unwrap_or(0))
            }
        };
        let l = app.engine.list_mut(&list);
        if let Some(c) = complete {
            l.complete = c;
        }
        if let Some(t) = top {
            l.top = t;
        }
        Ok(())
    });
    match r {
        Ok(()) => errc.ok(),
        Err(f) => errc.fail(j, &f),
    }
}

/// `QUIGETLA(handle, receiver, receiver length, format, list, errc)`:
/// `LIST0100`: bytes returned and available, list contents (`ALL` or
/// `MORE`), the handle of the first entry shown, the number of
/// entries, the number with an option typed, the current entry's
/// handle.
pub(super) fn quigetla(j: &mut Job, args: &[u64], _host: &SharedUimHost) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 5);
    let len = usize::try_from(a.bin4(2)).unwrap_or(0);
    let format = a.char(3, 8);
    let list = a.char(4, 10).to_ascii_uppercase();
    if len < 8 {
        errc.fail(j, &fail("CPF6A29", None, &list, ""));
        return;
    }
    if format != "LIST0100" {
        errc.fail(j, &ApiFail::id("CPF3C21").text(&format, 8));
        return;
    }
    let r = with_app(j, &a.bytes(0, 8), |_, app| {
        let l = existing(app, &list)?.clone();
        let visible = l.visible();
        let top = visible.get(l.top).map_or(0, |i| l.handles[*i]);
        let mut o = Out::new();
        o.char(8, 4, if l.complete { "ALL" } else { "MORE" });
        o.bytes(12, &top.to_be_bytes());
        o.bin4(16, i32::try_from(l.rows.len()).unwrap_or(i32::MAX));
        o.bin4(20, i32::try_from(l.options.len()).unwrap_or(i32::MAX));
        o.bytes(24, &l.current.map_or(0, |c| l.handles[c]).to_be_bytes());
        Ok(o)
    });
    match r {
        Ok(o) => {
            o.deliver(&a, 1, 2);
            errc.ok();
        }
        Err(f) => errc.fail(j, &f),
    }
}
