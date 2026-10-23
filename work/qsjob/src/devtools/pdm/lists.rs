// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The three lists: what they show, the options typed on them, the
//! command line and the keys; Find String from option 25 and from
//! `FNDSTRPDM`.

// Throughout: `s` the session, `l` the list, `a` the answer.
#![allow(clippy::many_single_char_names)]
use std::collections::BTreeMap;

use mi::Job;
use pdm::find::Query;
use pdm::option::{Outcome, fill_w, outcome, quoted, with_parameters};
use pdm::screen::list::{self, ListScreen, Row, View, key_set_count, option_set_count, page_size};
use pdm::screen::prompt;
use pdm::subset::{Subset, date, position, repeat};
use pdm::subst::{Context, substitute};
use pdm::{Item, ListKind};
use stg::Client;

use super::panels::{Flow, defaults, exec, print, work_options};
use super::ui::{Answer, Key, show};
use super::{Session, data, find};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;

/// `WRKOBJPDM`'s values (`*PRV` taken from the last session).
pub struct ObjSpec {
    /// Library.
    pub lib: String,
    /// Object name or generic name.
    pub obj: String,
    /// Object type.
    pub ty: String,
    /// Attribute.
    pub attr: String,
}

/// `WRKMBRPDM`'s values.
pub struct MbrSpec {
    /// Source file.
    pub file: String,
    /// Its library.
    pub lib: String,
    /// Member name or generic name.
    pub mbr: String,
    /// Member type.
    pub ty: String,
}

/// What a list shows.
enum Source {
    /// `WRKLIBPDM`'s list type.
    Libs(String),
    /// A library's objects.
    Objs(String),
    /// A file's members.
    Mbrs(String, String),
}

/// One list on the display.
struct List {
    kind: ListKind,
    source: Source,
    subset: Subset,
    items: Vec<Item>,
    dates: Vec<u32>,
    sizes: Vec<u64>,
    top: usize,
    typed: BTreeMap<usize, String>,
    opt_set: usize,
    key_set: usize,
    view: View,
    sort_date: bool,
    cmdline: String,
    message: Option<String>,
    header: BTreeMap<String, String>,
}

impl List {
    fn new(kind: ListKind, source: Source, subset: Subset) -> Self {
        Self {
            kind,
            source,
            subset,
            items: Vec::new(),
            dates: Vec::new(),
            sizes: Vec::new(),
            top: 0,
            typed: BTreeMap::new(),
            opt_set: 0,
            key_set: 0,
            view: View::Full,
            sort_date: false,
            cmdline: String::new(),
            message: None,
            header: BTreeMap::new(),
        }
    }

    /// Read the entries again.
    fn load(&mut self, host: &Host) {
        self.dates.clear();
        self.sizes.clear();
        match &self.source {
            Source::Libs(spec) => {
                self.items = data::libraries(host, spec)
                    .into_iter()
                    .filter(|i| self.subset.keeps(i, 0))
                    .collect();
                self.header = BTreeMap::from([(
                    "LISTTYPE".into(),
                    if spec.starts_with('*') {
                        spec.clone()
                    } else {
                        "*ALL".into()
                    },
                )]);
            }
            Source::Objs(lib) => {
                self.items = data::objects(host, lib, &self.subset);
                if self.view == View::Size {
                    let sizes = host
                        .link
                        .borrow_mut()
                        .with(|l| l.obj_sizes(lib.clone(), "*ALL".into(), "*ALL".into()));
                    self.sizes = self
                        .items
                        .iter()
                        .map(|i| {
                            sizes
                                .iter()
                                .find(|(_, n, t, _)| *n == i.name && *t == i.ty)
                                .map_or(0, |s| s.3)
                        })
                        .collect();
                }
                self.header = BTreeMap::from([("LIB".into(), lib.clone())]);
            }
            Source::Mbrs(lib, file) => {
                let mut m = data::members(host, lib, file, &self.subset);
                if self.sort_date {
                    m.sort_by_key(|x| std::cmp::Reverse(x.changed));
                }
                self.dates = m.iter().map(|x| x.changed).collect();
                self.items = m.into_iter().map(|x| x.item).collect();
                self.header =
                    BTreeMap::from([("FILE".into(), file.clone()), ("LIB".into(), lib.clone())]);
            }
        }
        if self.top >= self.items.len() {
            self.top = 0;
        }
    }

    fn rows(&self) -> Vec<Row> {
        self.items
            .iter()
            .enumerate()
            .map(|(i, it)| {
                let d = self.dates.get(i).copied().unwrap_or(0);
                Row {
                    name: it.name.clone(),
                    ty: it.ty.clone(),
                    third: if self.kind == ListKind::Object {
                        it.attr.clone()
                    } else {
                        String::new()
                    },
                    date: if d == 0 {
                        String::new()
                    } else {
                        format!(
                            "{:02}/{:02}/{:02}",
                            d / 100 % 100,
                            d % 100,
                            d / 10_000 % 100
                        )
                    },
                    size: self.sizes.get(i).copied().unwrap_or(0),
                    text: it.text.clone(),
                }
            })
            .collect()
    }
}

/// `*CURLIB` and `*LIBL` as the library they stand for.
fn real_lib(host: &Host, lib: &str) -> String {
    match lib {
        "*CURLIB" => data::curlib(host).unwrap_or_else(|| "QGPL".into()),
        other => other.to_string(),
    }
}

/// `WRKLIBPDM` and option 1.
pub fn libraries(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    s: &mut Session,
    spec: &str,
) -> Result<Flow, Fail> {
    let spec = if spec == "*PRV" {
        s.defaults.previous("LIBLIST", "*LIBL")
    } else {
        spec.to_string()
    };
    s.defaults.remember("LIBLIST", &spec);
    let mut l = List::new(ListKind::Library, Source::Libs(spec), Subset::default());
    work(job, host, surface, s, &mut l)
}

/// `WRKOBJPDM`, option 2 and 12 on a library.
pub fn objects(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    s: &mut Session,
    spec: ObjSpec,
) -> Result<Flow, Fail> {
    let prv = |v: String, key: &str, d: &str| {
        if v == "*PRV" {
            s.defaults.previous(key, d)
        } else {
            v
        }
    };
    let lib = real_lib(host, &prv(spec.lib, "OBJLIB", "*CURLIB"));
    let subset = Subset {
        name: prv(spec.obj, "OBJ", "*ALL"),
        ty: prv(spec.ty, "OBJTYPE", "*ALL"),
        attr: prv(spec.attr, "OBJATR", "*ALL"),
        ..Subset::default()
    };
    check_lib(host, &lib)?;
    s.defaults.remember("OBJLIB", &lib);
    s.defaults.remember("OBJ", &subset.name);
    s.defaults.remember("OBJTYPE", &subset.ty);
    s.defaults.remember("OBJATR", &subset.attr);
    let mut l = List::new(ListKind::Object, Source::Objs(lib), subset);
    work(job, host, surface, s, &mut l)
}

fn check_lib(host: &Host, lib: &str) -> Result<(), Fail> {
    let found = host
        .link
        .borrow_mut()
        .with(|c| c.object("QSYS".into(), lib.into(), "*LIB".into()));
    if found.is_none() {
        return Err(Fail::new("CPF2110", format!("Library {lib} not found.")));
    }
    Ok(())
}

/// `WRKMBRPDM`, option 3 and 12 on a file.
pub fn members(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    s: &mut Session,
    spec: MbrSpec,
) -> Result<Flow, Fail> {
    let file = if spec.file == "*PRV" {
        s.defaults.previous("FILE", "")
    } else {
        spec.file
    };
    if file.is_empty() {
        return Err(Fail::new("CPD0071", "Type the name of a source file."));
    }
    let lib = if spec.lib == "*PRV" {
        s.defaults.previous("FILELIB", "*LIBL")
    } else {
        spec.lib
    };
    let lib = data::file_library(host, &lib, &file).ok_or_else(|| {
        Fail::new(
            "CPF9812",
            format!("File {file} in library {lib} not found."),
        )
    })?;
    let prv = |v: String, key: &str| {
        if v == "*PRV" {
            s.defaults.previous(key, "*ALL")
        } else {
            v
        }
    };
    let subset = Subset {
        name: prv(spec.mbr, "MBR"),
        ty: prv(spec.ty, "MBRTYPE"),
        ..Subset::default()
    };
    s.defaults.remember("FILE", &file);
    s.defaults.remember("FILELIB", &lib);
    s.defaults.remember("MBR", &subset.name);
    s.defaults.remember("MBRTYPE", &subset.ty);
    let mut l = List::new(ListKind::Member, Source::Mbrs(lib, file), subset);
    work(job, host, surface, s, &mut l)
}

/// A list until F3 or F12.
#[allow(clippy::too_many_lines)] // one arm per key
fn work(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    s: &mut Session,
    l: &mut List,
) -> Result<Flow, Fail> {
    let system = crate::display::dds::system_name(host);
    l.load(host);
    loop {
        let rows = l.rows();
        let screen = list::render(&ListScreen {
            kind: l.kind,
            system: &system,
            header: &l.header,
            option_set: l.opt_set,
            key_set: l.key_set,
            view: l.view,
            rows: &rows,
            top: l.top,
            options: &l.typed,
            cmdline: &l.cmdline,
            message: l.message.as_deref(),
            copyright: s.first(),
        });
        l.message = None;
        let Some(a) = show(surface, &screen) else {
            return Ok(Flow::Gone);
        };
        l.cmdline.clear();
        l.typed.clone_from(&a.options);
        if header_changed(host, l, &a) {
            continue;
        }
        let page = page_size(l.kind, l.view);
        match a.key {
            Key::F(3) => return Ok(Flow::Exit),
            Key::F(12) => return Ok(Flow::Back),
            Key::F(5) => {
                l.typed.clear();
                l.load(host);
            }
            Key::F(4) if !l.typed.is_empty() => {
                if let Some(f) = options(job, host, surface, s, l, &a, true) {
                    return Ok(f);
                }
                l.load(host);
            }
            Key::F(4) if !a.cmdline.is_empty() => {
                s.last.clone_from(&a.cmdline);
                l.message = exec(job, host, &format!("?{}", a.cmdline));
            }
            Key::F(4) => l.message = Some("Type an option or a command to prompt.".into()),
            Key::F(6) => {
                let cmd = match &l.source {
                    Source::Libs(_) => "?CRTLIB".to_string(),
                    Source::Objs(_) => "GO CMDCRT".to_string(),
                    Source::Mbrs(lib, file) => format!("?STRSEU SRCFILE({lib}/{file})"),
                };
                l.message = exec(job, host, &cmd);
                l.load(host);
            }
            Key::F(9) => {
                if s.last.is_empty() {
                    l.message = Some("There is no command to retrieve.".into());
                } else {
                    l.cmdline.clone_from(&s.last);
                }
            }
            Key::F(10) => {
                l.message = Some("Use CALL QCMD from the menu for Command Entry.".into());
            }
            Key::F(11) => {
                l.view = if l.view == View::Names {
                    View::Full
                } else {
                    View::Names
                };
                l.top = 0;
            }
            Key::F(13) => {
                let typed: Vec<(usize, String)> = l.typed.clone().into_iter().collect();
                l.typed = repeat(&typed, l.items.len()).into_iter().collect();
            }
            Key::F(14) if l.kind != ListKind::Library => {
                let alt = if l.kind == ListKind::Object {
                    View::Size
                } else {
                    View::Date
                };
                l.view = if l.view == alt { View::Full } else { alt };
                l.load(host);
            }
            Key::F(15) if l.kind == ListKind::Member => {
                l.sort_date = !l.sort_date;
                l.load(host);
            }
            Key::F(16) => match work_options(job, host, surface, s) {
                Flow::Gone => return Ok(Flow::Gone),
                Flow::Exit => return Ok(Flow::Exit),
                Flow::Back => {}
            },
            Key::F(17) => match subset(surface, l) {
                Flow::Gone => return Ok(Flow::Gone),
                Flow::Exit => return Ok(Flow::Exit),
                Flow::Back => l.load(host),
            },
            Key::F(18) => {
                if defaults(surface, host, s) == Flow::Gone {
                    return Ok(Flow::Gone);
                }
            }
            Key::F(21) => {
                let lines: Vec<String> = rows
                    .iter()
                    .map(|r| format!("{:<10}  {:<10}  {:<10}  {}", r.name, r.ty, r.third, r.text))
                    .collect();
                l.message = Some(print(host, &lines));
            }
            Key::F(23) => l.opt_set = (l.opt_set + 1) % option_set_count(l.kind),
            Key::F(24) => l.key_set = (l.key_set + 1) % key_set_count(l.kind),
            Key::PageDown => {
                if l.top + page < l.items.len() {
                    l.top += page;
                } else {
                    l.message = Some("Already at the bottom of the list.".into());
                }
            }
            Key::PageUp => {
                if l.top == 0 {
                    l.message = Some("Already at the top of the list.".into());
                }
                l.top = l.top.saturating_sub(page);
            }
            Key::Enter => {
                let to = a.up("POSTO");
                if !to.is_empty() {
                    let ty = a.up("POSTYPE");
                    let at = l.items.iter().position(|i| {
                        i.name >= to
                            && (ty.is_empty()
                                || i.ty.trim_start_matches('*') >= ty.trim_start_matches('*'))
                    });
                    let names = l.items.iter().map(|i| i.name.as_str());
                    match at.or_else(|| position(names, &to)) {
                        Some(i) => l.top = i,
                        None => l.message = Some(format!("Nothing at or after {to}.")),
                    }
                    continue;
                }
                if !l.typed.is_empty() {
                    if let Some(f) = options(job, host, surface, s, l, &a, false) {
                        return Ok(f);
                    }
                    l.load(host);
                } else if !a.cmdline.is_empty() {
                    s.last.clone_from(&a.cmdline);
                    l.message = exec(job, host, &a.cmdline);
                    l.load(host);
                } else if s.defaults.exit_on_enter {
                    return Ok(Flow::Back);
                }
            }
            _ => l.message = Some("Function key not allowed.".into()),
        }
    }
}

/// A header field typed over: the list it names. `true` when the list
/// changed.
fn header_changed(host: &Host, l: &mut List, a: &Answer) -> bool {
    let changed = |k: &str| {
        let v = a.up(k);
        (!v.is_empty() && Some(&v) != l.header.get(k)).then_some(v)
    };
    match &l.source {
        Source::Libs(_) => {
            if let Some(v) = changed("LISTTYPE") {
                l.source = Source::Libs(v);
                l.load(host);
                return true;
            }
        }
        Source::Objs(_) => {
            if let Some(v) = changed("LIB") {
                if let Err(f) = check_lib(host, &v) {
                    l.message = Some(f.text);
                    return true;
                }
                l.source = Source::Objs(v);
                l.top = 0;
                l.load(host);
                return true;
            }
        }
        Source::Mbrs(lib, file) => {
            let nf = changed("FILE");
            let nl = changed("LIB");
            if nf.is_some() || nl.is_some() {
                let f = nf.unwrap_or_else(|| file.clone());
                let lb = nl.unwrap_or_else(|| lib.clone());
                match data::file_library(host, &lb, &f) {
                    Some(found) => {
                        l.source = Source::Mbrs(found, f);
                        l.top = 0;
                        l.load(host);
                    }
                    None => l.message = Some(format!("File {f} in library {lb} not found.")),
                }
                return true;
            }
        }
    }
    false
}

/// F17: the subset panel of the list.
fn subset(surface: &mut Surface, l: &mut List) -> Flow {
    let sub = l.subset.clone();
    let fmt = |d: Option<u32>, dft: &str| {
        d.map_or_else(
            || dft.to_string(),
            |d| {
                format!(
                    "{:02}/{:02}/{:02}",
                    d / 100 % 100,
                    d % 100,
                    d / 10_000 % 100
                )
            },
        )
    };
    let mut vals: BTreeMap<String, String> = BTreeMap::from([
        ("OBJ".into(), sub.name.clone()),
        ("MBR".into(), sub.name.clone()),
        ("OBJTYPE".into(), sub.ty.clone()),
        ("MBRTYPE".into(), sub.ty.clone()),
        ("OBJATR".into(), sub.attr.clone()),
        ("SIZEFROM".into(), "0".into()),
        ("SIZETO".into(), "9999999999".into()),
        ("TEXT".into(), sub.text.clone()),
        ("FROMDATE".into(), fmt(sub.from_date, "01/01/00")),
        ("TODATE".into(), fmt(sub.to_date, "12/31/99")),
    ]);
    let mut message: Option<String> = None;
    loop {
        let screen = {
            let v = |k: &str| vals.get(k).cloned().unwrap_or_default();
            match l.kind {
                ListKind::Member => prompt::subset_mbrs(&v, message.as_deref()),
                _ => prompt::subset_objs(&v, message.as_deref()),
            }
        };
        message = None;
        let Some(a) = show(surface, &screen) else {
            return Flow::Gone;
        };
        for (k, x) in &a.vars {
            vals.insert(k.clone(), x.trim().to_string());
        }
        match a.key {
            Key::F(3) => return Flow::Exit,
            Key::F(12) => return Flow::Back,
            Key::F(5) => vals.clear(),
            Key::Enter => {
                let up = |k: &str| {
                    let v = vals
                        .get(k)
                        .map(|x| x.trim().to_ascii_uppercase())
                        .unwrap_or_default();
                    if v.is_empty() { "*ALL".to_string() } else { v }
                };
                let mut n = Subset {
                    text: vals.get("TEXT").cloned().unwrap_or_default(),
                    ..Subset::default()
                };
                if l.kind == ListKind::Member {
                    n.name = up("MBR");
                    n.ty = up("MBRTYPE");
                    let v = |k: &str| vals.get(k).cloned().unwrap_or_default();
                    let (Ok(f), Ok(t)) = (date(&v("FROMDATE")), date(&v("TODATE"))) else {
                        message = Some("Type a date as MM/DD/YY.".into());
                        continue;
                    };
                    n.from_date = f.filter(|d| *d != 20_000_101);
                    n.to_date = t.filter(|d| *d != 19_991_231);
                } else {
                    n.name = up("OBJ");
                    n.ty = up("OBJTYPE");
                    n.attr = up("OBJATR");
                }
                if n.text.trim().is_empty() {
                    n.text = "*ALL".into();
                }
                l.subset = n;
                l.top = 0;
                return Flow::Back;
            }
            _ => message = Some("Function key not allowed.".into()),
        }
    }
}

/// The context a substitution and an option need.
fn context<'a>(
    s: &'a Session,
    kind: ListKind,
    option: &'a str,
    object_lib: &'a str,
) -> Context<'a> {
    Context {
        list: kind,
        option,
        defaults: &s.defaults,
        object_lib,
        user: &s.user,
    }
}

/// Run a command an option made: with the command line's parameters,
/// prompted on F4; the message.
fn run_one(job: &mut Job, host: &Host, cmd: &str, parms: &str, prompt: bool) -> Option<String> {
    let mut c = with_parameters(cmd, parms);
    if prompt && !c.starts_with('?') {
        c.insert(0, '?');
    }
    exec(job, host, &c)
}

/// The options typed on a list, in order. `Some` when the operator
/// left PDM from a display an option showed.
#[allow(clippy::too_many_lines)] // one arm per outcome
fn options(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    s: &mut Session,
    l: &mut List,
    a: &Answer,
    prompt: bool,
) -> Option<Flow> {
    let typed = std::mem::take(&mut l.typed);
    let parms = a.cmdline.clone();
    let curlib = data::curlib(host);
    let mut deletes: Vec<(Item, Vec<String>)> = Vec::new();
    let mut finds: Vec<Item> = Vec::new();
    let mut last: Option<String> = None;
    for (i, opt) in &typed {
        let Some(item) = l.items.get(*i).cloned() else {
            continue;
        };
        let objlib = s.defaults.object_lib(&item.library, curlib.as_deref());
        let numeric = opt.parse::<u8>().is_ok();
        if !numeric || pdm::option::find(l.kind, opt).is_none() {
            match pdm::uopt::lookup(&s.options, opt).cloned() {
                Some(u) => {
                    let ctx = context(s, l.kind, opt, &objlib);
                    let cmd = substitute(&u.command, &item, &ctx);
                    last = run_one(job, host, &cmd, &parms, prompt).or(last);
                }
                None => last = Some(format!("Option {opt} is not valid here.")),
            }
            continue;
        }
        let ctx = context(s, l.kind, opt, &objlib);
        match outcome(&item, &ctx) {
            Ok(Outcome::Run(cmds)) => {
                for c in cmds {
                    last = run_one(job, host, &c, &parms, prompt).or(last);
                }
            }
            Ok(Outcome::Confirm(cmds)) => deletes.push((item, cmds)),
            Ok(Outcome::Rename(t)) => {
                let what = match l.kind {
                    ListKind::Member => "Rename Members",
                    ListKind::Object => "Rename Objects",
                    ListKind::Library => "Rename Libraries",
                };
                match ask(surface, what, &item, false) {
                    Err(f) => return Some(f),
                    Ok(Some(n)) => last = exec(job, host, &fill_w(&t, &n.to_ascii_uppercase())),
                    Ok(None) => {}
                }
            }
            Ok(Outcome::ChangeText(t)) => match ask(surface, "Change Text", &item, true) {
                Err(f) => return Some(f),
                Ok(Some(n)) => last = exec(job, host, &fill_w(&t, &quoted(&n))),
                Ok(None) => {}
            },
            Ok(Outcome::WorkWith(ListKind::Object)) => {
                let spec = ObjSpec {
                    lib: item.name.clone(),
                    obj: "*ALL".into(),
                    ty: "*ALL".into(),
                    attr: "*ALL".into(),
                };
                match objects(job, host, surface, s, spec) {
                    Ok(Flow::Back) => {}
                    Ok(f) => return Some(f),
                    Err(e) => last = Some(e.text),
                }
            }
            Ok(Outcome::WorkWith(_)) => {
                let spec = MbrSpec {
                    file: item.name.clone(),
                    lib: item.library.clone(),
                    mbr: "*ALL".into(),
                    ty: "*ALL".into(),
                };
                match members(job, host, surface, s, spec) {
                    Ok(Flow::Back) => {}
                    Ok(f) => return Some(f),
                    Err(e) => last = Some(e.text),
                }
            }
            Ok(Outcome::Find) => finds.push(item),
            Err(e) => last = Some(e.to_string()),
        }
    }
    if !deletes.is_empty() {
        let what = match l.kind {
            ListKind::Member => "Members",
            ListKind::Object => "Objects",
            ListKind::Library => "Libraries",
        };
        let rows: Vec<(String, String, String)> = deletes
            .iter()
            .map(|(i, _)| (i.name.clone(), i.ty.clone(), i.text.clone()))
            .collect();
        match show(surface, &prompt::confirm(what, &rows, None)).map(|a| a.key) {
            None => return Some(Flow::Gone),
            Some(Key::Enter) => {
                for (_, cmds) in &deletes {
                    for c in cmds {
                        last = exec(job, host, c).or(last);
                    }
                }
            }
            Some(_) => l.typed = typed.into_iter().filter(|(_, o)| o == "4").collect(),
        }
    }
    if !finds.is_empty() {
        match find_panel(job, host, surface, s, l.kind, &finds) {
            Ok(m) => last = m.or(last),
            Err(f) => return Some(f),
        }
    }
    l.message = last;
    None
}

/// Rename or new text for one entry: `Ok(None)` when cancelled.
fn ask(
    surface: &mut Surface,
    heading: &str,
    item: &Item,
    text: bool,
) -> Result<Option<String>, Flow> {
    let mut message: Option<String> = None;
    let mut value = if text {
        item.text.clone()
    } else {
        String::new()
    };
    loop {
        let v = |k: &str| {
            if k == "NEW" {
                value.clone()
            } else {
                String::new()
            }
        };
        let shown = message.take();
        let screen = prompt::one_entry(heading, (&item.name, &item.ty), text, &v, shown.as_deref());
        let Some(a) = show(surface, &screen) else {
            return Err(Flow::Gone);
        };
        value = a
            .vars
            .get("NEW")
            .map(|x| x.trim().to_string())
            .unwrap_or_default();
        match a.key {
            Key::F(3) => return Err(Flow::Exit),
            Key::F(12) => return Ok(None),
            Key::Enter if !text && value.is_empty() => {
                message = Some("Type the new name.".into());
            }
            Key::Enter => return Ok(Some(value)),
            _ => message = Some("Function key not allowed.".into()),
        }
    }
}

/// Option 25: the Find String panel over the chosen members (or every
/// member of the chosen files), then the option on each member that
/// holds the string, or the printed report.
#[allow(clippy::too_many_lines)] // the panel, the search, the option
fn find_panel(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    s: &mut Session,
    kind: ListKind,
    items: &[Item],
) -> Result<Option<String>, Flow> {
    let mut vals: BTreeMap<String, String> = [
        ("STRING", ""),
        ("FROMCOL", "1"),
        ("TOCOL", "*RCDLEN"),
        ("CASE", "2"),
        ("OPTION", "5"),
        ("PROMPT", "N"),
        ("PRTLIST", "N"),
        ("PRTRCDS", "N"),
        ("NUMBER", "*ALL"),
        ("FORMAT", "*CHAR"),
        ("MARK", "Y"),
        ("OVERFLOW", "1"),
        ("BATCH", "N"),
        ("PARM", ""),
    ]
    .iter()
    .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
    .collect();
    let mut message: Option<String> = None;
    let a = loop {
        let v = |k: &str| vals.get(k).cloned().unwrap_or_default();
        let screen = prompt::find(&v, message.as_deref());
        message = None;
        let a = show(surface, &screen).ok_or(Flow::Gone)?;
        for (k, x) in &a.vars {
            vals.insert(k.clone(), x.trim_end().to_string());
        }
        match a.key {
            Key::F(3) => return Err(Flow::Exit),
            Key::F(12) => return Ok(None),
            Key::F(16) => {
                if work_options(job, host, surface, s) == Flow::Gone {
                    return Err(Flow::Gone);
                }
            }
            Key::F(18) => {
                if defaults(surface, host, s) == Flow::Gone {
                    return Err(Flow::Gone);
                }
            }
            Key::Enter if vals["STRING"].trim().is_empty() => {
                message = Some("Type the string to find.".into());
            }
            Key::Enter => break a,
            _ => message = Some("Function key not allowed.".into()),
        }
    };
    let _ = a;
    let get = |k: &str| {
        vals.get(k)
            .map(|x| x.trim().to_ascii_uppercase())
            .unwrap_or_default()
    };
    let query = Query {
        string: vals["STRING"].trim().to_string(),
        from: get("FROMCOL").parse().unwrap_or(1),
        to: get("TOCOL").parse().ok(),
        match_case: get("CASE") == "1",
    };
    // The files and members searched: on a member list the members, on
    // an object list every member of each file.
    let mut groups: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for i in items {
        let (file, mbr) = if kind == ListKind::Member {
            (i.file.clone(), i.name.clone())
        } else {
            (i.name.clone(), "*ALL".to_string())
        };
        groups
            .entry((i.library.clone(), file))
            .or_default()
            .push(mbr);
    }
    if get("BATCH") == "Y" {
        let mut last = None;
        for ((lib, file), mbrs) in &groups {
            let cmd = format!(
                "SBMJOB CMD(FNDSTRPDM STRING({}) FILE({lib}/{file}) MBR({}) PRTMBRLIST(*YES)) JOB(FNDSTRPDM)",
                quoted(&query.string),
                mbrs.join(" ")
            );
            last = exec(job, host, &cmd);
        }
        return Ok(last);
    }
    let option = get("OPTION");
    let prompt_it = get("PROMPT") == "Y";
    let mut found_total = 0;
    let mut report: Vec<String> = Vec::new();
    let mut last = None;
    for ((lib, file), mbrs) in groups {
        let search = find::Search {
            lib: lib.clone(),
            file: file.clone(),
            members: mbrs,
            query: query.clone(),
        };
        let (hits, searched) = find::run(host, &search);
        found_total += hits.len();
        let records = (get("PRTRCDS") == "Y").then(|| get("NUMBER").parse().ok());
        if get("PRTLIST") == "Y" || records.is_some() {
            report.extend(
                pdm::find::report(&lib, &file, &query, &hits, searched.len(), records)
                    .lines()
                    .map(str::to_string),
            );
        }
        if option.is_empty() || option == "*NONE" {
            continue;
        }
        for h in &hits {
            let Some(m) = searched.iter().find(|m| m.item.name == h.member) else {
                continue;
            };
            let curlib = data::curlib(host);
            let objlib = s.defaults.object_lib(&lib, curlib.as_deref());
            let ctx = context(s, ListKind::Member, &option, &objlib);
            let cmds = match outcome(&m.item, &ctx) {
                Ok(Outcome::Run(c)) => c,
                Ok(_) => continue,
                Err(_) => {
                    if let Some(u) = pdm::uopt::lookup(&s.options, &option) {
                        vec![substitute(&u.command, &m.item, &ctx)]
                    } else {
                        last = Some(format!("Option {option} is not valid here."));
                        break;
                    }
                }
            };
            for c in cmds {
                last = run_one(job, host, &c, &get("PARM"), prompt_it).or(last);
            }
        }
    }
    if !report.is_empty() {
        last = Some(print(host, &report));
    }
    Ok(Some(last.unwrap_or_else(|| {
        format!("{found_total} members contain the string.")
    })))
}

/// `FNDSTRPDM` at a display: the option run on each member that holds
/// the string, the report printed when asked; the summary.
pub fn found(
    job: &mut Job,
    host: &Host,
    s: &mut Session,
    search: &find::Search,
    option: &str,
    prompt_it: bool,
    print_list: bool,
) -> String {
    let (hits, searched) = find::run(host, search);
    let mut last = None;
    if option != "*NONE" {
        let curlib = data::curlib(host);
        let objlib = s.defaults.object_lib(&search.lib, curlib.as_deref());
        for h in &hits {
            let Some(m) = searched.iter().find(|m| m.item.name == h.member) else {
                continue;
            };
            let ctx = context(s, ListKind::Member, option, &objlib);
            let cmds = match outcome(&m.item, &ctx) {
                Ok(Outcome::Run(c)) => c,
                _ => match pdm::uopt::lookup(&s.options, option) {
                    Some(u) => vec![substitute(&u.command, &m.item, &ctx)],
                    None => continue,
                },
            };
            for c in cmds {
                last = run_one(job, host, &c, "", prompt_it).or(last);
            }
        }
    }
    if print_list {
        let report = pdm::find::report(
            &search.lib,
            &search.file,
            &search.query,
            &hits,
            searched.len(),
            None,
        );
        let lines: Vec<String> = report.lines().map(str::to_string).collect();
        last = Some(print(host, &lines));
    }
    let _ = last;
    format!(
        "{} of {} members searched contain the string.",
        hits.len(),
        searched.len()
    )
}
