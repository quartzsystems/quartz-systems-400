// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Option 1, Design Screens: the member's records on Work with Display
//! Records with their options, file-level keywords and comments, the
//! subset and defaults displays, and on leaving, Save DDS - Create
//! Display File.

use std::collections::BTreeMap;

use mi::Job;
use sda::screen::{comments, entry, records};
use sda::source::{Line, Member, Record, RecordType, write};

use super::ui::{self, Key};
use super::{Flow, Gone, Session, exec, file_library, kw, member_info, must, read_member, today, valid_name, write_member};
use crate::console::host::Host;
use crate::display::surface::Surface;

/// The member being worked on.
pub struct Work {
    /// The source file.
    pub file: String,
    /// Its library, resolved.
    pub lib: String,
    /// The member.
    pub member: String,
    /// Its source type.
    pub srctype: String,
    /// Its text.
    pub text: String,
    /// Its last change, `yy-mm-dd`.
    pub date: String,
    /// The lines as read, to tell whether anything changed.
    pub read: Vec<String>,
    /// The member as it stands.
    pub m: Member,
}

impl Work {
    /// The member's lines as they would be saved now.
    pub fn lines(&self, host: &Host) -> Vec<String> {
        write(&self.m, &today(host).1)
    }

    /// Whether saving would change the member.
    pub fn changed(&self) -> bool {
        self.m.records.iter().any(Record::is_changed)
            || self.m.orig_keywords.as_ref() != Some(&self.m.keywords)
            || self.read.is_empty()
    }
}

/// Design Screens, then the member's records; `prefilled` goes straight
/// to the records when the values are good.
pub fn start(job: &mut Job, host: &Host, surface: &mut Surface, s: &mut Session, prefilled: bool) -> Flow {
    let mut message: Option<String> = None;
    let mut skip = prefilled;
    loop {
        let (file, lib, member) = if skip {
            skip = false;
            s.src.clone()
        } else {
            let mut vals = BTreeMap::new();
            vals.insert("SRCFILE".to_string(), s.src.0.clone());
            vals.insert("SRCLIB".to_string(), s.src.1.clone());
            vals.insert("SRCMBR".to_string(), s.src.2.clone());
            let screen = entry::design_screens(&|k| vals.get(k).cloned().unwrap_or_default(), message.as_deref());
            message = None;
            let Some(a) = ui::show(surface, &screen) else {
                return Flow::Gone;
            };
            match a.key() {
                Key::F(3) => return Flow::Exit,
                Key::F(12) => return Flow::Back,
                Key::F(4) => {
                    message = Some("Type the name; the list of names is not available here.".into());
                    continue;
                }
                Key::Enter => {}
                _ => {
                    message = Some("Function key not allowed.".into());
                    continue;
                }
            }
            s.src = (a.up("SRCFILE"), a.up("SRCLIB"), a.up("SRCMBR"));
            s.src.clone()
        };
        let Some(work) = open(host, &file, &lib, &member, "DSPF", &mut message) else {
            continue;
        };
        match records_loop(job, host, surface, s, work) {
            (Flow::Gone, _) => return Flow::Gone,
            (Flow::Exit, m) => {
                message = m;
                if prefilled {
                    return Flow::Exit;
                }
            }
            (Flow::Back, m) => message = m,
        }
    }
}

/// Open a member for design: the file found, the member read (a new
/// one when missing), its type checked.
pub fn open(host: &Host, file: &str, lib: &str, member: &str, want: &str, message: &mut Option<String>) -> Option<Work> {
    if file.is_empty() || !valid_name(file) {
        *message = Some("Type a source file name.".into());
        return None;
    }
    if member.is_empty() || !valid_name(member) {
        *message = Some("Type a member name.".into());
        return None;
    }
    let lib = if lib.is_empty() { "*LIBL" } else { lib };
    let Some(real) = file_library(host, lib, file) else {
        *message = Some(format!("File {file} in library {lib} not found."));
        return None;
    };
    let existing = read_member(host, &real, file, member);
    let (srctype, text, date) = member_info(host, &real, file, member).unwrap_or_default();
    let srctype = if srctype.trim().is_empty() {
        if existing.is_some() {
            *message = Some(format!("Member {member} has no source type; SDA needs {want}."));
            return None;
        }
        want.to_string()
    } else {
        srctype.trim().to_string()
    };
    let ok = match want {
        "DSPF" => srctype == "DSPF",
        _ => srctype == "MNUDDS",
    };
    if !ok {
        *message = Some(format!("Member {member} is of type {srctype}; SDA needs {want}."));
        return None;
    }
    let lines = existing.unwrap_or_default();
    let mut m = sda::parse(&lines);
    if lines.is_empty() {
        m.keywords = vec![sda::source::write::dspsiz()];
    }
    Some(Work {
        file: file.to_string(),
        lib: real,
        member: member.to_string(),
        srctype,
        text,
        date,
        read: lines,
        m,
    })
}

/// The subset: a generic record name and a record type.
#[derive(Clone, Debug)]
struct Subset {
    name: String,
    ty: String,
}

impl Subset {
    fn keeps(&self, r: &Record) -> bool {
        let name = match self.name.strip_suffix('*') {
            Some(p) => r.name.starts_with(p),
            None => self.name == "*ALL" || self.name.is_empty() || r.name == self.name,
        };
        let w = r.ty.word();
        let ty = match self.ty.as_str() {
            "*ALL" | "" => true,
            "SFL*" => w.starts_with("SFL"),
            t => t == w,
        };
        name && ty
    }
}

/// The records list until the member is left. Returns the flow and the
/// message for the display before.
#[allow(clippy::too_many_lines, clippy::many_single_char_names)] // one display, its options and keys
fn records_loop(job: &mut Job, host: &Host, surface: &mut Surface, s: &mut Session, mut w: Work) -> (Flow, Option<String>) {
    let mut view = records::View {
        file: w.file.clone(),
        lib: w.lib.clone(),
        member: w.member.clone(),
        srctype: w.srctype.clone(),
        ..records::View::default()
    };
    let mut subset = Subset {
        name: "*ALL".into(),
        ty: "*ALL".into(),
    };
    let mut message: Option<String> = None;
    let (date_today, _) = today(host);
    loop {
        let lines = w.lines(host);
        let bad = sda::source::errors(&lines);
        let shown: Vec<usize> = (0..w.m.records.len()).filter(|i| subset.keeps(&w.m.records[*i])).collect();
        let rows: Vec<records::Row> = shown
            .iter()
            .map(|&i| {
                let r = &w.m.records[i];
                records::Row {
                    order: u32::try_from(i + 1).unwrap_or(0) * 10,
                    name: r.name.clone(),
                    ty: r.ty.word().to_string(),
                    related: r.related.clone().unwrap_or_default(),
                    date: if r.is_changed() { date_today.clone() } else { w.date.clone() },
                    error: bad.contains(&r.name),
                    text: r
                        .keyword("TEXT")
                        .and_then(|k| k.quoted(0))
                        .unwrap_or_default()
                        .to_string(),
                }
            })
            .collect();
        if view.top >= rows.len() {
            view.top = 0;
        }
        let screen = records::records(&view, &rows, ("", ""), message.as_deref());
        message = None;
        let Some(a) = ui::show(surface, &screen) else {
            return (Flow::Gone, None);
        };
        match a.key() {
            Key::F(3) => match save(job, host, surface, &mut w) {
                Saved::Gone => return (Flow::Gone, None),
                Saved::Back(m) => message = m,
                Saved::Done(m) => return (Flow::Exit, m),
            },
            Key::F(12) => {
                if !w.changed() {
                    return (Flow::Back, None);
                }
                match save(job, host, surface, &mut w) {
                    Saved::Gone => return (Flow::Gone, None),
                    Saved::Back(m) => message = m,
                    Saved::Done(m) => return (Flow::Back, m),
                }
            }
            Key::F(5) => {}
            Key::F(11) => view.text = !view.text,
            Key::F(24) => view.keys2 = !view.keys2,
            Key::F(13) => {
                if defaults(surface, s) == Flow::Gone {
                    return (Flow::Gone, None);
                }
            }
            Key::F(14) => {
                if kw::file(surface, &mut w.m, &w.member.clone()) == Flow::Gone {
                    return (Flow::Gone, None);
                }
            }
            Key::F(15) => {
                let name = w.member.clone();
                let mut head = std::mem::take(&mut w.m.head);
                let flow = edit_comments(surface, "File comments", &name, &mut head, true);
                w.m.head = head;
                if flow == Flow::Gone {
                    return (Flow::Gone, None);
                }
            }
            Key::F(17) => match subset_display(surface, &w, &subset) {
                None => return (Flow::Gone, None),
                Some(n) => subset = n,
            },
            Key::PageDown => {
                if view.top + records::PAGE < rows.len() {
                    view.top += records::PAGE;
                } else {
                    message = Some("You are at the bottom of the list.".into());
                }
            }
            Key::PageUp => {
                if view.top == 0 {
                    message = Some("You are at the top of the list.".into());
                } else {
                    view.top = view.top.saturating_sub(records::PAGE);
                }
            }
            Key::Enter => {
                // Order numbers first, then the options.
                reorder(&mut w.m, &shown, &a);
                let add = a.up("ADDOPT");
                if !add.is_empty() {
                    message = add_line(job, host, surface, s, &mut w, &add, &a.up("ADDNAME"));
                    if message.as_deref() == Some("") {
                        return (Flow::Gone, None);
                    }
                    continue;
                }
                for (row, opt) in &a.options {
                    let Some(&ri) = shown.get(*row) else { continue };
                    let Some(name) = w.m.records.get(ri).map(|r| r.name.clone()) else {
                        continue;
                    };
                    let Some(ri) = w.m.record(&name) else { continue };
                    let r = option(job, host, surface, s, &mut w, ri, opt);
                    match r {
                        Err(()) => return (Flow::Gone, None),
                        Ok(Some(m)) => message = Some(m),
                        Ok(None) => {}
                    }
                }
            }
            _ => message = Some("Function key not allowed.".into()),
        }
    }
}

/// Move records whose order number was changed.
fn reorder(m: &mut Member, shown: &[usize], a: &ui::Answer) {
    let mut keyed: Vec<(u32, usize)> = (0..m.records.len())
        .map(|i| (u32::try_from(i + 1).unwrap_or(0) * 10, i))
        .collect();
    let mut moved = false;
    for (row, &ri) in shown.iter().enumerate() {
        let Some(v) = a.cells.get(&(row, "ORDER".to_string())) else {
            continue;
        };
        let Ok(n) = v.trim().parse::<u32>() else { continue };
        if n != keyed[ri].0 {
            keyed[ri].0 = n;
            moved = true;
        }
    }
    if !moved {
        return;
    }
    keyed.sort_by_key(|(n, i)| (*n, *i));
    let old = std::mem::take(&mut m.records);
    let mut slots: Vec<Option<Record>> = old.into_iter().map(Some).collect();
    for (_, i) in keyed {
        if let Some(mut r) = slots[i].take() {
            r.changed = true;
            m.records.push(r);
        }
    }
}

#[allow(clippy::many_single_char_names)] // job, host, s, w, a
/// The add line's option: 1 adds a record. The message; an empty one
/// when the display is gone.
fn add_line(job: &mut Job, host: &Host, surface: &mut Surface, s: &mut Session, w: &mut Work, opt: &str, name: &str) -> Option<String> {
    if opt != "1" {
        return Some(format!("Option {opt} needs a record from the list."));
    }
    let mut vals = BTreeMap::new();
    vals.insert("NAME".to_string(), name.to_string());
    vals.insert("TYPE".to_string(), "RECORD".to_string());
    let mut message: Option<String> = None;
    loop {
        let head = (w.file.as_str(), w.lib.as_str(), w.member.as_str(), w.srctype.as_str());
        let screen = entry::add_record(head, &|k| vals.get(k).cloned().unwrap_or_default(), message.as_deref());
        let Some(a) = ui::show(surface, &screen) else {
            return Some(String::new());
        };
        match a.key() {
            Key::F(3 | 12) => return None,
            Key::F(5) => {
                vals.insert("NAME".into(), name.to_string());
                vals.insert("TYPE".into(), "RECORD".into());
                continue;
            }
            Key::Enter => {}
            _ => {
                message = Some("Function key not allowed.".into());
                continue;
            }
        }
        let n = a.up("NAME");
        let t = a.up("TYPE");
        vals.insert("NAME".into(), n.clone());
        vals.insert("TYPE".into(), t.clone());
        if !valid_name(&n) {
            message = Some("Type a valid record name.".into());
            continue;
        }
        if w.m.record(&n).is_some() {
            message = Some(format!("Record {n} is already in the member."));
            continue;
        }
        let Some(ty) = RecordType::from_word(&t) else {
            message = Some(format!("Type {t} is not one of the record types shown."));
            continue;
        };
        let mut r = Record::new(&n, ty);
        r.keywords = sda::source::member::type_keywords(ty, &w.m);
        w.m.records.push(r);
        let ri = w.m.records.len() - 1;
        // A new record goes straight to its image, as SDA does.
        if super::image::edit(job, host, surface, s, &mut w.m, ri) == Flow::Gone {
            return Some(String::new());
        }
        return Some(format!("Record {n} added; it is kept when you save the member."));
    }
}

/// One option on a record. `Err` when the display is gone, else the
/// message.
fn option(job: &mut Job, host: &Host, surface: &mut Surface, s: &mut Session, w: &mut Work, ri: usize, opt: &str) -> Result<Option<String>, ()> {
    let name = w.m.records[ri].name.clone();
    match opt {
        "2" => {
            let mut head = std::mem::take(&mut w.m.records[ri].head);
            let flow = edit_comments(surface, "Record comments", &name, &mut head, false);
            let r = &mut w.m.records[ri];
            r.head = head;
            if flow == Flow::Gone {
                return Err(());
            }
            Ok(None)
        }
        "3" | "7" => {
            let what = if opt == "3" { "copy" } else { "new name" };
            let Some(n) = ask_name(w, surface, &name, what)? else {
                return Ok(None);
            };
            if opt == "3" {
                let mut r = w.m.records[ri].clone();
                r.name.clone_from(&n);
                r.changed = true;
                r.orig_keywords = None;
                r.head.clear();
                for e in &mut r.elements {
                    e.orig = None;
                    e.lines.clear();
                }
                // Field names are unique in a file: copies get new ones.
                let mut taken = w.m.field_names();
                for e in &mut r.elements {
                    if let sda::source::Kind::Field { name: fname, .. } = &mut e.kind {
                        let fresh = r_name(&taken);
                        taken.push(fresh.clone());
                        *fname = fresh;
                    }
                }
                w.m.records.insert(ri + 1, r);
                Ok(Some(format!("Record {name} copied to {n}.")))
            } else {
                let r = &mut w.m.records[ri];
                r.name.clone_from(&n);
                r.changed = true;
                Ok(Some(format!("Record {name} renamed to {n}.")))
            }
        }
        "4" => {
            w.m.records.remove(ri);
            Ok(Some(format!("Record {name} deleted; save the member to keep this.")))
        }
        "8" => {
            if kw::record(surface, &mut w.m.records[ri]) == Flow::Gone {
                return Err(());
            }
            Ok(None)
        }
        "12" => {
            if super::image::edit(job, host, surface, s, &mut w.m, ri) == Flow::Gone {
                return Err(());
            }
            Ok(None)
        }
        other => Ok(Some(format!("Option {other} is not valid."))),
    }
}

fn r_name(taken: &[String]) -> String {
    (1..=99_999)
        .map(|n| format!("FLD{n:03}"))
        .find(|n| !taken.contains(n))
        .unwrap_or_default()
}

/// A record name for copy or rename, on Add New Record's display.
fn ask_name(w: &Work, surface: &mut Surface, from: &str, what: &str) -> Result<Option<String>, ()> {
    let mut message = Some(format!("Type the {what} for record {from}."));
    let ty = w.m.record(from).map_or("RECORD", |i| w.m.records[i].ty.word());
    loop {
        let head = (w.file.as_str(), w.lib.as_str(), w.member.as_str(), w.srctype.as_str());
        let mut vals = BTreeMap::new();
        vals.insert("TYPE".to_string(), ty.to_string());
        let screen = entry::add_record(head, &|k| vals.get(k).cloned().unwrap_or_default(), message.as_deref());
        let Some(a) = ui::show(surface, &screen) else {
            return Err(());
        };
        if matches!(a.key(), Key::F(3 | 12)) {
            return Ok(None);
        }
        let n = a.up("NAME");
        if !valid_name(&n) {
            message = Some("Type a valid record name.".into());
        } else if w.m.record(&n).is_some() {
            message = Some(format!("Record {n} is already in the member."));
        } else {
            return Ok(Some(n));
        }
    }
}

/// The comment lines of a head (the `%%` stamps aside), edited; with
/// `file`, the head's own comments, else those after its `R` line.
pub fn edit_comments(surface: &mut Surface, what: &str, name: &str, head: &mut Vec<Line>, file: bool) -> Flow {
    let is_note = |l: &Line| l.is_comment() && l.get(8, 9) != "%%";
    let current: Vec<String> = head.iter().filter(|l| is_note(l)).map(Line::text).collect();
    let Ok(lines) = comments_display(surface, what, name, &current) else {
        return Flow::Gone;
    };
    let Some(lines) = lines else {
        return Flow::Back;
    };
    let new: Vec<Line> = lines.iter().map(|l| Line::new(l)).collect();
    let at = if file {
        head.iter().position(|l| !l.is_comment()).unwrap_or(head.len())
    } else {
        head.iter().position(|l| l.name_type() == 'R').map_or(0, |i| i + 1)
    };
    let mut out: Vec<Line> = Vec::new();
    for (i, l) in head.iter().enumerate() {
        if i == at {
            out.extend(new.iter().cloned());
        }
        if !is_note(l) {
            out.push(l.clone());
        }
    }
    if at >= head.len() {
        out.extend(new);
    }
    *head = out;
    Flow::Back
}

/// The comments editor: `Ok(Some(lines))` when saved, `Ok(None)` when
/// left without.
pub fn comments_display(surface: &mut Surface, what: &str, name: &str, lines: &[String]) -> Result<Option<Vec<String>>, Gone> {
    let mut lines: Vec<String> = lines.to_vec();
    let mut message: Option<String> = None;
    loop {
        let screen = comments::editor(what, name, &lines, message.as_deref());
        message = None;
        let a = ui::show(surface, &screen).ok_or(Gone)?;
        let shown = lines.len().clamp(1, comments::LINES);
        let mut typed: Vec<String> = (0..shown)
            .map(|i| a.cells.get(&(i, "TEXT".to_string())).cloned().unwrap_or_default())
            .collect();
        // Every line is a comment: `A*` in columns 6-7.
        for t in &mut typed {
            let mut l = Line::new(t);
            if l.blank(1, 80) {
                continue;
            }
            l.set(6, 7, "A*");
            *t = l.text();
        }
        typed.retain(|t| !t.trim().is_empty());
        match a.key() {
            Key::Enter => {
                lines = typed;
                if lines.len() >= comments::LINES - 1 {
                    message = Some("The display holds no more lines.".into());
                } else {
                    lines.push(format!("{:5}A*", ""));
                }
                lines.retain(|l| l.trim() != "A*");
            }
            Key::F(3) => {
                let mut vals = BTreeMap::new();
                vals.insert("SAVE".to_string(), "N".to_string());
                let screen = entry::comments_exit(&|k| vals.get(k).cloned().unwrap_or_default());
                let e = ui::show(surface, &screen).ok_or(Gone)?;
                match e.key() {
                    Key::F(12) => {}
                    _ if e.up("SAVE") == "Y" => return Ok(Some(typed)),
                    _ => return Ok(None),
                }
            }
            Key::F(12) => return Ok(None),
            _ => message = Some("Function key not allowed.".into()),
        }
    }
}

/// Subset Records; `None` when the display is gone.
fn subset_display(surface: &mut Surface, w: &Work, cur: &Subset) -> Option<Subset> {
    let mut vals = BTreeMap::new();
    vals.insert("NAME".to_string(), cur.name.clone());
    vals.insert("TYPE".to_string(), cur.ty.clone());
    let mut message: Option<String> = None;
    loop {
        let head = (w.file.as_str(), w.lib.as_str(), w.member.as_str(), w.srctype.as_str());
        let screen = entry::subset(head, &|k| vals.get(k).cloned().unwrap_or_default(), message.as_deref());
        let a = ui::show(surface, &screen)?;
        match a.key() {
            Key::F(3 | 12) => return Some(cur.clone()),
            Key::F(5) => continue,
            Key::Enter => {}
            _ => {
                message = Some("Function key not allowed.".into());
                continue;
            }
        }
        let ty = a.up("TYPE");
        let ok = matches!(ty.as_str(), "*ALL" | "SFL*" | "SFLCTL" | "GRDRCD") || RecordType::from_word(&ty).is_some();
        if !ok {
            vals.insert("TYPE".into(), ty);
            message = Some("Type a record type the display lists.".into());
            continue;
        }
        return Some(Subset {
            name: if a.up("NAME").is_empty() { "*ALL".into() } else { a.up("NAME") },
            ty,
        });
    }
}

/// Change Defaults.
fn defaults(surface: &mut Surface, s: &mut Session) -> Flow {
    let mut message: Option<String> = None;
    loop {
        let v = if s.flddef { "Y" } else { "N" };
        let screen = entry::defaults(&|_| v.to_string(), message.as_deref());
        let Some(a) = ui::show(surface, &screen) else {
            return Flow::Gone;
        };
        match a.key() {
            Key::F(3) => return Flow::Exit,
            Key::F(12) => return Flow::Back,
            Key::Enter => match a.up("FLDDEF").as_str() {
                "Y" => {
                    s.flddef = true;
                    return Flow::Back;
                }
                "N" => {
                    s.flddef = false;
                    return Flow::Back;
                }
                _ => message = Some("Type Y or N.".into()),
            },
            _ => message = Some("Function key not allowed.".into()),
        }
    }
}

/// How Save DDS ended.
enum Saved {
    /// Back to the records.
    Back(Option<String>),
    /// The member is left, with this message.
    Done(Option<String>),
    /// The display is gone.
    Gone,
}

/// Save DDS - Create Display File.
fn save(job: &mut Job, host: &Host, surface: &mut Surface, w: &mut Work) -> Saved {
    let mut vals: BTreeMap<String, String> = [
        ("SAVE", "Y"),
        ("SRCFILE", w.file.as_str()),
        ("SRCLIB", w.lib.as_str()),
        ("SRCMBR", w.member.as_str()),
        ("TEXT", w.text.as_str()),
        ("CREATE", "Y"),
        ("PROMPT", ""),
        ("FILE", w.member.as_str()),
        ("LIB", w.lib.as_str()),
        ("REPLACE", "Y"),
        ("BATCH", "N"),
    ]
    .iter()
    .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
    .collect();
    let exists = !w.read.is_empty();
    let mut message = exists.then(|| format!("Member {} is replaced when you press Enter.", w.member));
    loop {
        let screen = entry::save(&|k| vals.get(k).cloned().unwrap_or_default(), message.as_deref());
        let Some(a) = ui::show(surface, &screen) else {
            return Saved::Gone;
        };
        for k in ["SAVE", "SRCFILE", "SRCLIB", "SRCMBR", "CREATE", "PROMPT", "FILE", "LIB", "REPLACE", "BATCH", "MORE"] {
            vals.insert(k.to_string(), a.up(k));
        }
        vals.insert("TEXT".into(), a.raw("TEXT").trim().to_string());
        match a.key() {
            Key::F(12) => return Saved::Back(Some(format!("Member {} was not saved.", w.member))),
            Key::F(3) => return Saved::Done(Some(format!("Member {} was not saved.", w.member))),
            Key::Enter => {}
            _ => {
                message = Some("Function key not allowed.".into());
                continue;
            }
        }
        let save = vals["SAVE"] == "Y";
        let create = vals["CREATE"] == "Y";
        if !save && !create {
            return Saved::Done(Some(format!("Member {} was not saved.", w.member)));
        }
        if create && !save {
            message = Some("The file is created from the saved member: type Y to save it.".into());
            continue;
        }
        let (file, lib, mbr) = (vals["SRCFILE"].clone(), vals["SRCLIB"].clone(), vals["SRCMBR"].clone());
        let Some(real) = file_library(host, &lib, &file) else {
            message = Some(format!("File {file} in library {lib} not found."));
            continue;
        };
        let lines = w.lines(host);
        if let Err(e) = write_member(host, &real, &file, &mbr, lines, &w.srctype) {
            message = Some(e);
            continue;
        }
        let text = vals["TEXT"].clone();
        if !text.is_empty() && text != w.text {
            let _ = must(job, host, &format!("CHGPFM FILE({real}/{file}) MBR({mbr}) TEXT('{}')", text.replace('\'', "''")));
        }
        let mut done = format!("Member {mbr} saved in file {real}/{file}.");
        if create {
            let obj_lib = if vals["LIB"].is_empty() { "*CURLIB".to_string() } else { vals["LIB"].clone() };
            let replace = if vals["REPLACE"] == "Y" { "*YES" } else { "*NO" };
            let crt = format!(
                "CRTDSPF FILE({obj_lib}/{}) SRCFILE({real}/{file}) SRCMBR({mbr}) REPLACE({replace})",
                vals["FILE"]
            );
            let line = if vals["PROMPT"] == "Y" {
                format!("?{crt}")
            } else if vals["BATCH"] == "Y" {
                format!("SBMJOB CMD({crt}) JOB({})", vals["FILE"])
            } else {
                crt
            };
            done = exec(job, host, &line).unwrap_or(done);
        }
        return Saved::Done(Some(done));
    }
}
