// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The keyword displays: Select File, Record or Field Keywords, then
//! for each group marked `Y` the display attributes or colors as a Y/N
//! list or the group's keywords as text lines.

use std::collections::BTreeMap;

use sda::keywords::{self as dds_kw, Group, Level};
use sda::screen::fields::{self as fscreen, Head};
use sda::source::{Element, Member, Record};

use super::Flow;
use super::ui::{self, Key};
use crate::display::surface::Surface;

/// What a keyword display edits.
enum Target<'a> {
    /// The file level.
    File(&'a mut Member),
    /// A record.
    Record(&'a mut Record),
    /// A field or constant.
    Element(&'a mut Element),
}

impl Target<'_> {
    fn keywords(&mut self) -> &mut Vec<sda::Keyword> {
        match self {
            Target::File(m) => &mut m.keywords,
            Target::Record(r) => &mut r.keywords,
            Target::Element(e) => &mut e.keywords,
        }
    }

    fn level(&self) -> Level {
        match self {
            Target::File(_) => Level::File,
            Target::Record(_) => Level::Record,
            Target::Element(_) => Level::Field,
        }
    }

    fn what(&self) -> String {
        match self {
            Target::File(_) => "the file".into(),
            Target::Record(r) => r.name.clone(),
            Target::Element(e) => e.name().map_or_else(|| sda::image::element_text(e), str::to_string),
        }
    }
}

/// File-level keywords (F14).
pub fn file(surface: &mut Surface, m: &mut Member, member: &str) -> Flow {
    let head = Head::Member(member.to_string());
    select(surface, Target::File(m), &head, dds_kw::FILE)
}

/// Record-level keywords (option 8).
pub fn record(surface: &mut Surface, r: &mut Record) -> Flow {
    let head = Head::Record(r.name.clone());
    select(surface, Target::Record(r), &head, dds_kw::RECORD)
}

/// A field's or constant's keywords.
pub fn element(surface: &mut Surface, e: &mut Element) -> Flow {
    let head = Head::of(e);
    let groups = dds_kw::field_groups(e);
    select(surface, Target::Element(e), &head, &groups)
}

fn select(surface: &mut Surface, mut t: Target<'_>, head: &Head, groups: &[Group]) -> Flow {
    let mut message: Option<String> = None;
    loop {
        let screen = fscreen::select_keywords(head, groups, &|_| String::new(), message.as_deref());
        message = None;
        let Some(a) = ui::show(surface, &screen) else {
            return Flow::Gone;
        };
        match a.key() {
            Key::F(3) => return Flow::Exit,
            Key::F(12) => return Flow::Back,
            Key::F(4) => {
                let all: Vec<String> = t
                    .keywords()
                    .iter()
                    .filter(|k| k.name != dds_kw::CONSTANT)
                    .map(ToString::to_string)
                    .collect();
                let screen = fscreen::keyword_text("Selected Keywords", &t.what(), &all, None);
                if ui::show(surface, &screen).is_none() {
                    return Flow::Gone;
                }
                continue;
            }
            Key::Enter => {}
            _ => {
                message = Some("Function key not allowed.".into());
                continue;
            }
        }
        let chosen: Vec<&Group> = groups.iter().filter(|g| a.up(g.id) == "Y").collect();
        if chosen.is_empty() {
            return Flow::Back;
        }
        for g in chosen {
            let flow = match g.id {
                "DSPATR" | "COLOR" => choose(surface, &mut t, g),
                _ => text(surface, &mut t, g),
            };
            if flow == Flow::Gone {
                return Flow::Gone;
            }
        }
    }
}

/// Display attributes or colors as a Y/N list.
fn choose(surface: &mut Surface, t: &mut Target<'_>, g: &Group) -> Flow {
    let Target::Element(e) = t else {
        return Flow::Back;
    };
    let (title, items, current) = if g.id == "DSPATR" {
        ("Select Display Attributes", fscreen::ATTRS, e.attrs.clone())
    } else {
        ("Select Colors", fscreen::COLORS, e.colors.clone())
    };
    let what = e.name().map_or_else(|| sda::image::element_text(e), str::to_string);
    let vals: BTreeMap<String, String> = current.iter().map(|v| (v.clone(), "Y".to_string())).collect();
    let screen = fscreen::choose(title, &what, items, &|k| vals.get(k).cloned().unwrap_or_default(), None);
    let Some(a) = ui::show(surface, &screen) else {
        return Flow::Gone;
    };
    if matches!(a.key(), Key::F(3 | 12)) {
        return Flow::Back;
    }
    let picked: Vec<String> = items
        .iter()
        .filter(|(v, _)| a.up(v) == "Y")
        .map(|(v, _)| (*v).to_string())
        .collect();
    if g.id == "DSPATR" {
        e.attrs = picked;
    } else {
        e.colors = picked;
    }
    Flow::Back
}

/// A group's keywords as text lines.
fn text(surface: &mut Surface, t: &mut Target<'_>, g: &Group) -> Flow {
    let level = t.level();
    let what = t.what();
    let mut lines = dds_kw::texts(level, g.id, t.keywords());
    let mut message: Option<String> = None;
    loop {
        let screen = fscreen::keyword_text(g.label, &what, &lines, message.as_deref());
        let Some(a) = ui::show(surface, &screen) else {
            return Flow::Gone;
        };
        if matches!(a.key(), Key::F(3 | 12)) {
            return Flow::Back;
        }
        lines = (0..fscreen::KEYWORD_LINES)
            .map(|i| a.raw(&format!("KW{i}")))
            .filter(|l| !l.trim().is_empty())
            .collect();
        match dds_kw::replace(level, g.id, t.keywords(), &lines) {
            Ok(()) => {
                if let Target::Record(r) = t {
                    r.changed = true;
                }
                return Flow::Back;
            }
            Err(e) => message = Some(e),
        }
    }
}
