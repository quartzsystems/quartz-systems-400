// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Work with Fields (F4 on the image): the record's fields and
//! constants with their order, name and place; options 1 (keywords), 2
//! (comments) and 4 (delete); new hidden, message and
//! program-to-system fields on the add lines; F6 sorts by place.

use sda::screen::fields::{self as fscreen, Row};
use sda::source::{Element, Line, Record, Usage};

use super::design::comments_display;
use super::ui::{self, Key};
use super::{Flow, kw, valid_name};
use crate::display::surface::Surface;

/// Whether two displayed elements share places on the display.
fn overlaps(r: &Record, i: usize) -> bool {
    let e = &r.elements[i];
    let Some((row, col)) = e.pos.filter(|_| e.displayed()) else {
        return false;
    };
    let (a0, a1) = (u32::from(col), u32::from(col) + e.width);
    r.elements.iter().enumerate().any(|(j, o)| {
        j != i
            && o.displayed()
            && o.pos.is_some_and(|(r2, c2)| {
                r2 == row && u32::from(c2) < a1 && a0 < u32::from(c2) + o.width
            })
    })
}

/// A length typed on an add line: `10` or `7,2`.
fn length(t: &str) -> Option<(u32, Option<u8>)> {
    let t = t.trim();
    match t.split_once(',') {
        Some((l, d)) => Some((l.trim().parse().ok()?, Some(d.trim().parse().ok()?))),
        None => Some((t.parse().ok()?, None)),
    }
}

/// Work with Fields over a record until F3 or F12.
#[allow(clippy::too_many_lines)] // one display, its options and keys
pub fn work_with(surface: &mut Surface, r: &mut Record, taken: &[String]) -> Flow {
    let mut top = 0usize;
    let mut roll = 6usize;
    let mut message: Option<String> = None;
    loop {
        let rows: Vec<Row> = (0..r.elements.len())
            .map(|i| Row::of(u32::try_from(i + 1).unwrap_or(0) * 10, &r.elements[i], overlaps(r, i)))
            .collect();
        if top >= rows.len() {
            top = 0;
        }
        let screen = fscreen::work_with_fields(&r.name, &rows, top, &roll.to_string(), message.as_deref());
        message = None;
        let Some(a) = ui::show(surface, &screen) else {
            return Flow::Gone;
        };
        if let Ok(n) = a.up("ROLL").parse::<usize>()
            && n > 0
        {
            roll = n;
        }
        match a.key() {
            Key::F(3) => return Flow::Exit,
            Key::F(12) => return Flow::Back,
            Key::F(6) => {
                r.elements.sort_by_key(|e| (!e.displayed(), e.pos));
                r.changed = true;
                continue;
            }
            Key::PageDown => {
                if top + roll < rows.len() {
                    top += roll;
                }
                continue;
            }
            Key::PageUp => {
                top = top.saturating_sub(roll);
                continue;
            }
            Key::Enter => {}
            _ => {
                message = Some("Function key not allowed.".into());
                continue;
            }
        }
        // Names changed on the list.
        for i in 0..r.elements.len() {
            let Some(v) = a.cells.get(&(i, "NAME".to_string())) else {
                continue;
            };
            let v = v.trim().to_ascii_uppercase();
            let e = &mut r.elements[i];
            if let sda::source::Kind::Field { name, .. } = &mut e.kind
                && v != *name
                && !v.is_empty()
            {
                if valid_name(&v) {
                    *name = v;
                } else {
                    message = Some(format!("Name {v} is not valid."));
                }
            }
        }
        // New order numbers.
        let mut order: Vec<(u32, usize)> = (0..r.elements.len())
            .map(|i| {
                let given = a
                    .cells
                    .get(&(i, "ORDER".to_string()))
                    .and_then(|v| v.trim().parse::<u32>().ok());
                (given.unwrap_or(u32::try_from(i + 1).unwrap_or(0) * 10), i)
            })
            .collect();
        if order.windows(2).any(|w| w[0].0 > w[1].0) {
            order.sort_by_key(|(n, i)| (*n, *i));
            let mut slots: Vec<Option<Element>> = std::mem::take(&mut r.elements).into_iter().map(Some).collect();
            for (_, i) in &order {
                if let Some(e) = slots[*i].take() {
                    r.elements.push(e);
                }
            }
            r.changed = true;
        }
        // The add lines.
        for (name, usage) in [("ADDH", Usage::Hidden), ("ADDM", Usage::Message), ("ADDP", Usage::Program)] {
            let n = a.up(name);
            if n.is_empty() {
                continue;
            }
            let Some((len, dec)) = length(&a.up(&format!("{name}LEN"))) else {
                message = Some(format!("Type a length for field {n}."));
                continue;
            };
            if !valid_name(&n) || taken.contains(&n) || r.elements.iter().any(|e| e.name() == Some(&n)) {
                message = Some(format!("Field name {n} is not valid or is already used."));
                continue;
            }
            r.elements.push(Element::field(&n, usage, dec.is_some(), len, dec.unwrap_or(0), None));
            r.changed = true;
        }
        // Options, last entry first so deletes keep the indexes.
        let mut opts: Vec<(usize, String)> = a.options.iter().map(|(i, o)| (*i, o.clone())).collect();
        opts.sort_by_key(|x| std::cmp::Reverse(x.0));
        for (i, opt) in opts {
            if i >= r.elements.len() {
                continue;
            }
            match opt.as_str() {
                "1" => {
                    if kw::element(surface, &mut r.elements[i]) == Flow::Gone {
                        return Flow::Gone;
                    }
                }
                "2" => {
                    let e = &mut r.elements[i];
                    let what = e.name().unwrap_or("constant").to_string();
                    let f = e.lines.iter().position(|l| !l.is_comment()).unwrap_or(e.lines.len());
                    let current: Vec<String> = e.lines[..f].iter().map(Line::text).collect();
                    match comments_display(surface, "Field comments", &what, &current) {
                        Err(_) => return Flow::Gone,
                        Ok(Some(new)) => {
                            let rest = e.lines.split_off(f);
                            e.lines = new.iter().map(|l| Line::new(l)).collect();
                            e.lines.extend(rest);
                            r.changed = true;
                        }
                        Ok(None) => {}
                    }
                }
                "4" => {
                    r.elements.remove(i);
                    r.changed = true;
                }
                other => message = Some(format!("Option {other} is not valid.")),
            }
        }
    }
}
