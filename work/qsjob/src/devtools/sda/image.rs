// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The design image of one record: what the operator types is worked
//! out by `sda::image::place` and applied to a working copy, kept on
//! F12 or option 1 of Exit SDA Work Screen. F4 Work with Fields, F9
//! records shown with it, F10 database fields (placed with `&n`, F11
//! hides or shows their list), F14 the ruler, F17 print, F20 the fields the image does not show, F21
//! the additional records on or off.

use std::collections::BTreeMap;

use mi::Job;
use stg::Client;
use sda::image::{View, element_text, place, render};
use sda::screen::{entry, fields as fscreen};
use sda::source::{Element, Kind, Member, Record, Usage};

use super::ui::{self, Key};
use super::{Flow, Gone, Session, fields, kw};
use crate::console::host::Host;
use crate::display::surface::Surface;

/// A database field waiting to be placed with `&n`.
#[derive(Clone, Debug)]
struct DbField {
    name: String,
    usage: Usage,
    numeric: bool,
    len: u32,
    dec: u8,
}

/// Edit record `ri` of the member on the design image.
#[allow(clippy::too_many_lines)] // the image's keys, in one place
pub fn edit(job: &mut Job, host: &Host, surface: &mut Surface, s: &mut Session, m: &mut Member, ri: usize) -> Flow {
    let _ = job;
    let mut work = m.records[ri].clone();
    let mut view = View::default();
    let mut with: Vec<String> = Vec::new();
    let mut show_with = true;
    let mut db: Vec<DbField> = Vec::new();
    let mut show_db = true;
    let mut message = Some(format!("Designing record {}: Help lists the keys.", work.name));
    let mut cursor = (2, 2);
    loop {
        let others: Vec<&Record> = if show_with {
            with.iter()
                .filter_map(|n| m.record(n))
                .map(|i| &m.records[i])
                .filter(|r| r.name != work.name)
                .collect()
        } else {
            Vec::new()
        };
        let mut rows = render(&work, &others, &view);
        let bottom = message.take().or_else(|| {
            (show_db && !db.is_empty()).then(|| {
                db.iter()
                    .enumerate()
                    .map(|(i, f)| format!("&{} {}", i + 1, f.name))
                    .collect::<Vec<_>>()
                    .join("  ")
            })
        });
        if let Some(b) = bottom {
            rows[23] = format!(" {b:79}").chars().take(80).collect();
        }
        let Some((key, typed, cur)) = ui::show_image(surface, &rows, cursor) else {
            return Flow::Gone;
        };
        cursor = cur;
        if key == Key::F(12) {
            m.records[ri] = work;
            return Flow::Back;
        }
        if key == Key::F(3) {
            match exit_display(surface) {
                None => return Flow::Gone,
                Some(1) => {
                    m.records[ri] = work;
                    return Flow::Back;
                }
                Some(2) => return Flow::Back,
                Some(_) => continue,
            }
        }
        // What was typed counts with every other key.
        let taken: Vec<String> = m
            .records
            .iter()
            .filter(|r| r.name != work.name)
            .flat_map(|r| r.elements.iter().filter_map(|e| e.name().map(str::to_string)))
            .collect();
        let typed = place_db(&rows, typed, &mut db, &mut work, &taken);
        let asked = match place::parse(&work, &rows, &typed) {
            Ok(edits) => place::apply(&mut work, &edits, &taken),
            Err(e) => {
                message = Some(e.to_string());
                continue;
            }
        };
        if let Some(k) = asked
            && kw::element(surface, &mut work.elements[k]) == Flow::Gone
        {
            return Flow::Gone;
        }
        match key {
            Key::Enter => {}
            Key::F(4) => {
                if fields::work_with(surface, &mut work, &taken) == Flow::Gone {
                    return Flow::Gone;
                }
            }
            Key::F(9) => match additional(surface, m, &work.name, &mut with) {
                Err(Gone) => return Flow::Gone,
                Ok(msg) => message = msg,
            },
            Key::F(10) => match db_files(host, surface, &mut db) {
                Err(Gone) => return Flow::Gone,
                Ok(msg) => message = msg,
            },
            Key::F(14) => {
                view.ruler = if view.ruler.is_some() { None } else { Some(cursor) };
            }
            Key::F(17) => message = Some(print(host, &work, &rows, s.flddef)),
            Key::F(20) => {
                let names = sda::image::hidden_names(&work);
                message = Some(if names.is_empty() {
                    "The record has no hidden, message or program-to-system fields.".into()
                } else {
                    format!("Not shown: {}", names.join(" "))
                });
            }
            Key::F(11) => show_db = !show_db,
            Key::F(21) => show_with = !show_with,
            Key::F(1) | Key::Help => {
                message = Some(
                    "F3 end, F4 fields, F9 records, F10 database, F11 list, F12 keep, F14 ruler, F17 print, F20 hidden, F21"
                        .into(),
                );
            }
            _ => message = Some("Function key not allowed.".into()),
        }
    }
}

/// Exit SDA Work Screen: the choice, `None` when the display is gone.
fn exit_display(surface: &mut Surface) -> Option<u8> {
    let mut message: Option<String> = None;
    loop {
        let screen = entry::image_exit("1", message.as_deref());
        let a = ui::show(surface, &screen)?;
        if a.key() == Key::F(12) {
            return Some(3);
        }
        match a.up("CHOICE").as_str() {
            "1" => return Some(1),
            "2" => return Some(2),
            "3" => return Some(3),
            _ => message = Some("Type 1, 2 or 3.".into()),
        }
    }
}

/// `&n` typed on blank places: database field `n` goes there (the `&`
/// takes the attribute place). The typing with those taken out.
fn place_db(shown: &[String], mut typed: Vec<String>, db: &mut Vec<DbField>, work: &mut Record, taken: &[String]) -> Vec<String> {
    if db.is_empty() {
        return typed;
    }
    let mut placed: Vec<usize> = Vec::new();
    for (r, row) in typed.iter_mut().enumerate() {
        let old: Vec<char> = shown.get(r).map(|s| s.chars().collect()).unwrap_or_default();
        let mut chars: Vec<char> = row.chars().collect();
        let mut c = 0;
        while c < chars.len() {
            if chars[c] == '&' && old.get(c).is_none_or(|o| *o == ' ') {
                let mut k = c + 1;
                while k < chars.len() && chars[k].is_ascii_digit() {
                    k += 1;
                }
                let n: usize = chars[c + 1..k].iter().collect::<String>().parse().unwrap_or(0);
                if let Some(f) = n.checked_sub(1).and_then(|i| db.get(i)) {
                    if !taken.contains(&f.name) && !work.elements.iter().any(|e| e.name() == Some(&f.name)) {
                        let row = u16::try_from(r + 1).unwrap_or(1);
                        let col = u16::try_from(c + 2).unwrap_or(2);
                        let e = Element::field(&f.name, f.usage, f.numeric, f.len, f.dec, Some((row, col)));
                        let at = work
                            .elements
                            .iter()
                            .position(|x| x.displayed() && x.pos.is_some_and(|p| p > (row, col)))
                            .unwrap_or(work.elements.len());
                        work.elements.insert(at, e);
                        work.changed = true;
                        placed.push(n - 1);
                    }
                    for ch in &mut chars[c..k] {
                        *ch = ' ';
                    }
                }
                c = k;
            } else {
                c += 1;
            }
        }
        *row = chars.into_iter().collect();
    }
    placed.sort_unstable();
    placed.dedup();
    for i in placed.into_iter().rev() {
        db.remove(i);
    }
    typed
}

/// Select Additional Records (F9): the member's other records with a Y
/// to show them under this one. The message.
fn additional(surface: &mut Surface, m: &Member, this: &str, with: &mut Vec<String>) -> Result<Option<String>, Gone> {
    let names: Vec<(String, String)> = m
        .records
        .iter()
        .filter(|r| r.name != this)
        .map(|r| (r.name.clone(), r.ty.word().to_string()))
        .collect();
    if names.is_empty() {
        return Ok(Some("The member has no other records to show with this one.".into()));
    }
    let items: Vec<(&str, &str)> = names.iter().map(|(n, t)| (n.as_str(), t.as_str())).take(14).collect();
    let chosen = with.clone();
    let screen = fscreen::choose(
        "Select Additional Records",
        this,
        &items,
        &|k| if chosen.iter().any(|c| c == k) { "Y".into() } else { String::new() },
        None,
    );
    let a = ui::show(surface, &screen).ok_or(Gone)?;
    if matches!(a.key(), Key::F(3 | 12)) {
        return Ok(None);
    }
    *with = items
        .iter()
        .filter(|(n, _)| a.up(n) == "Y")
        .map(|(n, _)| (*n).to_string())
        .collect();
    Ok(None)
}

/// Select Database Files (F10): the fields of the files chosen wait on
/// the message line to be placed. The message.
fn db_files(host: &Host, surface: &mut Surface, db: &mut Vec<DbField>) -> Result<Option<String>, Gone> {
    let mut vals: BTreeMap<String, String> = BTreeMap::new();
    for i in 0..4 {
        vals.insert(format!("LIB{i}"), "*LIBL".into());
    }
    let mut message: Option<String> = None;
    loop {
        let screen = entry::db_files(&|k| vals.get(k).cloned().unwrap_or_default(), message.as_deref());
        let a = ui::show(surface, &screen).ok_or(Gone)?;
        if matches!(a.key(), Key::F(3 | 12)) {
            return Ok(None);
        }
        for (k, v) in &a.vars {
            vals.insert(k.clone(), v.trim().to_ascii_uppercase());
        }
        let mut added = 0;
        message = None;
        for i in 0..4 {
            let opt = a.up(&format!("OPT{i}"));
            if opt.is_empty() {
                continue;
            }
            let usage = match opt.as_str() {
                "2" => Usage::Input,
                "3" => Usage::Output,
                "1" | "4" => Usage::Both,
                _ => {
                    message = Some(format!("Option {opt} is not valid."));
                    break;
                }
            };
            let file = a.up(&format!("FILE{i}"));
            let lib = a.up(&format!("LIB{i}"));
            let lib = if lib.is_empty() { "*LIBL".to_string() } else { lib };
            let rcd = a.up(&format!("RCD{i}"));
            let formats = host.link.borrow_mut().with(|l| l.formats(lib.clone(), file.clone()));
            let Some(f) = formats
                .iter()
                .find(|f| rcd.is_empty() || f.name == rcd)
            else {
                message = Some(format!("File {file} in library {lib} has no record {rcd}."));
                break;
            };
            for fi in &f.fields {
                let numeric = matches!(fi.ty, 'P' | 'S' | 'B' | 'F');
                db.push(DbField {
                    name: fi.name.clone(),
                    usage,
                    numeric,
                    len: if numeric { fi.digits } else { fi.len },
                    dec: if numeric { u8::try_from(fi.decimals).unwrap_or(0) } else { 0 },
                });
                added += 1;
            }
        }
        if message.is_none() {
            return Ok(Some(if added == 0 {
                "No fields were selected.".into()
            } else {
                format!("{added} fields wait on this line: type &n where field n goes.")
            }));
        }
    }
}

/// Print the image, and the fields when the defaults say so, to
/// `QSYSPRT`.
fn print(host: &Host, work: &Record, rows: &[String], flddef: bool) -> String {
    let mut lines: Vec<String> = vec![format!("Record {}", work.name), String::new()];
    let scale = "....+....1....+....2....+....3....+....4....+....5....+....6....+....7....+....8";
    lines.push(format!("     {scale}"));
    for (i, r) in rows.iter().enumerate().take(23) {
        lines.push(format!("{:>3}  {}", i + 1, r.trim_end()));
    }
    if flddef {
        lines.push(String::new());
        for e in &work.elements {
            let at = e.pos.map(|(r, c)| format!("{r:02} {c:03}")).unwrap_or_default();
            let what = match &e.kind {
                Kind::Constant { .. } => format!("constant  {}", element_text(e)),
                Kind::Field { name, usage, len, dec, .. } => {
                    format!("{name:<10} {} {len}{}", usage.letter(), dec.map(|d| format!(",{d}")).unwrap_or_default())
                }
            };
            lines.push(format!("{at:7} {what}"));
        }
    }
    super::print(host, &lines)
}
