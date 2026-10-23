// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The menu, the Specify panels, Change Defaults, the user-defined
//! options and the small entry displays, and running a command from a
//! command line or an option.

// Throughout: `s` the session, `l` the list, `a` the answer.
#![allow(clippy::many_single_char_names)]
use std::collections::BTreeMap;

use mi::Job;
use pdm::screen::list::{OPTIONS_PAGE, OptionsScreen, user_options};
use pdm::screen::{menu, prompt};
use pdm::uopt::{UserOption, check_code};
use stg::Client;

use super::lists::{self, MbrSpec, ObjSpec};
use super::ui::{Answer, Key, show};
use super::{Session, data};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;

/// How a display was left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    /// F12: back one display.
    Back,
    /// F3: leave PDM.
    Exit,
    /// The display is gone.
    Gone,
}

/// Run a command line: the first line of its output, or its error.
pub fn exec(job: &mut Job, host: &Host, text: &str) -> Option<String> {
    let out = crate::command::run(job, host, text);
    if let Some(f) = &out.escape {
        let line = f.text.lines().last().unwrap_or("").trim().to_string();
        return Some(if line.is_empty() {
            format!(
                "{} ended with {}.",
                if out.name.is_empty() {
                    "The command"
                } else {
                    &out.name
                },
                f.msgid
            )
        } else {
            line
        });
    }
    out.text
        .lines()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim().to_string())
}

/// What a command line's keys do on every PDM display that has one:
/// run the command, prompt it, retrieve the last. `true` when the key
/// was one of these.
pub fn command_keys(
    job: &mut Job,
    host: &Host,
    s: &mut Session,
    a: &Answer,
    cmdline: &mut String,
    message: &mut Option<String>,
) -> bool {
    match a.key {
        Key::F(4) if !a.cmdline.is_empty() => {
            s.last.clone_from(&a.cmdline);
            *message = exec(job, host, &format!("?{}", a.cmdline));
            cmdline.clear();
            true
        }
        Key::F(9) => {
            if s.last.is_empty() {
                *message = Some("There is no command to retrieve.".into());
            } else {
                cmdline.clone_from(&s.last);
            }
            true
        }
        Key::F(10) => {
            *message = Some("Use CALL QCMD from the menu for Command Entry.".into());
            true
        }
        _ => false,
    }
}

/// `STRPDM`: the menu until F3 or F12.
pub fn strpdm(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    s: &mut Session,
) -> Result<(), Fail> {
    let mut cmdline = String::new();
    let mut message: Option<String> = None;
    loop {
        let screen = menu::strpdm(&cmdline, message.as_deref(), s.first());
        message = None;
        let Some(a) = show(surface, &screen) else {
            return Ok(());
        };
        cmdline.clear();
        if command_keys(job, host, s, &a, &mut cmdline, &mut message) {
            continue;
        }
        let flow = match a.key {
            Key::F(3 | 12) => return Ok(()),
            Key::F(18) => defaults(surface, host, s),
            Key::F(4) => {
                message = Some("Type a command to prompt on the command line.".into());
                continue;
            }
            Key::Help => help(surface, s),
            Key::Enter => match a.cmdline.as_str() {
                "" => continue,
                "1" => specify_libs(job, host, surface, s),
                "2" => specify_objs(job, host, surface, s),
                "3" => specify_mbrs(job, host, surface, s),
                "9" => specify_optfile(job, host, surface, s),
                n if n.parse::<u32>().is_ok() => {
                    message = Some(format!("Selection {n} is not valid."));
                    continue;
                }
                cmd => {
                    s.last = cmd.to_string();
                    message = exec(job, host, cmd);
                    continue;
                }
            },
            _ => {
                message = Some("Function key not allowed.".into());
                continue;
            }
        };
        match flow {
            Flow::Exit | Flow::Gone => return Ok(()),
            Flow::Back => {}
        }
    }
}

/// The menu's help.
fn help(surface: &mut Surface, _s: &mut Session) -> Flow {
    let lines: Vec<String> = [
        "PDM lists libraries, the objects in a library and the members of a",
        "source file, and runs options and commands on what the lists show.",
        "",
        "Type an option number or a command on the command line and press",
        "Enter. F4 with a command shows its prompt; F9 brings back the last",
        "command run here.",
        "",
        "#1. Work with libraries",
        "Asks which libraries to list, then shows them with their options.",
        "",
        "#2. Work with objects",
        "Asks for a library and the objects to list, then shows them.",
        "",
        "#3. Work with members",
        "Asks for a source file and the members to list, then shows them.",
        "",
        "#9. Work with user-defined options",
        "Asks for an option file, then lists its options to create, change,",
        "copy, delete or display.",
        "",
        "#F18=Change defaults",
        "Sets where compiled objects go, whether compiles run in batch and",
        "which option file the lists use.",
    ]
    .iter()
    .map(|l| (*l).to_string())
    .collect();
    let mut page = 0;
    loop {
        let Some(a) = show(surface, &menu::help("PDM Main Menu - Help", &lines, page)) else {
            return Flow::Gone;
        };
        match a.key {
            Key::PageDown if (page + 1) * 17 < lines.len() => page += 1,
            Key::PageUp => page = page.saturating_sub(1),
            Key::F(10) => page = 0,
            Key::F(3 | 12) | Key::Enter => return Flow::Back,
            _ => {}
        }
    }
}

/// A display built from field values and a message.
type Build<'a> = &'a dyn Fn(&dyn Fn(&str) -> String, Option<&str>) -> pdm::screen::Screen;

/// A Specify panel until Enter (the answer) or another way out.
fn specify(
    surface: &mut Surface,
    vals: &mut BTreeMap<String, String>,
    start: &BTreeMap<String, String>,
    message: &mut Option<String>,
    build: Build<'_>,
) -> Result<Answer, Flow> {
    loop {
        let v = |k: &str| vals.get(k).cloned().unwrap_or_default();
        let screen = build(&v, message.as_deref());
        *message = None;
        let Some(a) = show(surface, &screen) else {
            return Err(Flow::Gone);
        };
        for (k, x) in &a.vars {
            vals.insert(k.clone(), x.trim().to_ascii_uppercase());
        }
        match a.key {
            Key::F(3) => return Err(Flow::Exit),
            Key::F(12) => return Err(Flow::Back),
            Key::F(5) => vals.clone_from(start),
            Key::Enter => return Ok(a),
            Key::F(4) => {
                *message = Some("Type a name; there is no list to choose from here.".into());
            }
            _ => *message = Some("Function key not allowed.".into()),
        }
    }
}

fn or(v: &str, d: &str) -> String {
    if v.trim().is_empty() {
        d.to_string()
    } else {
        v.trim().to_ascii_uppercase()
    }
}

/// A list's way back: F12 shows the Specify panel again.
fn after(r: Result<Flow, Fail>, message: &mut Option<String>) -> Option<Flow> {
    match r {
        Ok(Flow::Back) => None,
        Ok(f) => Some(f),
        Err(f) => {
            *message = Some(f.text);
            None
        }
    }
}

/// Option 1: Specify Libraries to Work With, then the list.
fn specify_libs(job: &mut Job, host: &Host, surface: &mut Surface, s: &mut Session) -> Flow {
    let start = BTreeMap::from([
        ("LIB".to_string(), s.defaults.previous("LIBLIST", "*LIBL")),
        ("ASPNUM".to_string(), "*ALL".to_string()),
        ("ASPDEV".to_string(), "*".to_string()),
    ]);
    let mut vals = start.clone();
    let mut message = None;
    loop {
        let a = match specify(surface, &mut vals, &start, &mut message, &|v, m| {
            prompt::specify_libs(v, m)
        }) {
            Ok(a) => a,
            Err(f) => return f,
        };
        let lib = or(&a.up("LIB"), "*LIBL");
        if let Some(f) = after(lists::libraries(job, host, surface, s, &lib), &mut message) {
            return f;
        }
    }
}

/// Option 2: Specify Objects to Work With, then the list.
fn specify_objs(job: &mut Job, host: &Host, surface: &mut Surface, s: &mut Session) -> Flow {
    let start = BTreeMap::from([
        ("LIB".to_string(), s.defaults.previous("OBJLIB", "*CURLIB")),
        ("OBJ".to_string(), "*ALL".to_string()),
        ("OBJTYPE".to_string(), "*ALL".to_string()),
        ("OBJATR".to_string(), "*ALL".to_string()),
    ]);
    let mut vals = start.clone();
    let mut message = None;
    loop {
        let a = match specify(surface, &mut vals, &start, &mut message, &|v, m| {
            prompt::specify_objs(v, m)
        }) {
            Ok(a) => a,
            Err(f) => return f,
        };
        let spec = ObjSpec {
            lib: or(&a.up("LIB"), "*CURLIB"),
            obj: or(&a.up("OBJ"), "*ALL"),
            ty: or(&a.up("OBJTYPE"), "*ALL"),
            attr: or(&a.up("OBJATR"), "*ALL"),
        };
        if let Some(f) = after(lists::objects(job, host, surface, s, spec), &mut message) {
            return f;
        }
    }
}

/// Option 3: Specify Members to Work With, then the list.
fn specify_mbrs(job: &mut Job, host: &Host, surface: &mut Surface, s: &mut Session) -> Flow {
    let start = BTreeMap::from([
        ("FILE".to_string(), s.defaults.previous("FILE", "")),
        ("LIB".to_string(), s.defaults.previous("FILELIB", "*LIBL")),
        ("MBR".to_string(), "*ALL".to_string()),
        ("MBRTYPE".to_string(), "*ALL".to_string()),
    ]);
    let mut vals = start.clone();
    let mut message = None;
    loop {
        let a = match specify(surface, &mut vals, &start, &mut message, &|v, m| {
            prompt::specify_mbrs(v, m)
        }) {
            Ok(a) => a,
            Err(f) => return f,
        };
        let file = a.up("FILE");
        if file.is_empty() {
            message = Some("Type the name of a source file.".into());
            continue;
        }
        let spec = MbrSpec {
            file,
            lib: or(&a.up("LIB"), "*LIBL"),
            mbr: or(&a.up("MBR"), "*ALL"),
            ty: or(&a.up("MBRTYPE"), "*ALL"),
        };
        if let Some(f) = after(lists::members(job, host, surface, s, spec), &mut message) {
            return f;
        }
    }
}

/// Option 9: Specify Option File to Work With, then the options.
fn specify_optfile(job: &mut Job, host: &Host, surface: &mut Surface, s: &mut Session) -> Flow {
    let start = BTreeMap::from([
        ("OPTFILE".to_string(), s.defaults.option_file.clone()),
        ("OPTLIB".to_string(), s.defaults.option_lib.clone()),
        ("OPTMBR".to_string(), s.defaults.option_member.clone()),
    ]);
    let mut vals = start.clone();
    let mut message = None;
    loop {
        let a = match specify(surface, &mut vals, &start, &mut message, &|v, m| {
            prompt::specify_optfile(v, m)
        }) {
            Ok(a) => a,
            Err(f) => return f,
        };
        let mut d = s.defaults.clone();
        let set = d
            .set("OPTFILE", &or(&a.up("OPTFILE"), "QAUOOPT"))
            .and_then(|()| d.set("OPTLIB", &or(&a.up("OPTLIB"), "QGPL")))
            .and_then(|()| d.set("OPTMBR", &or(&a.up("OPTMBR"), "QAUOOPT")));
        if let Err(e) = set {
            message = Some(e.to_string());
            continue;
        }
        s.defaults = d;
        s.options = data::user_options(host, &s.defaults);
        match work_options(job, host, surface, s) {
            Flow::Back => {}
            f => return f,
        }
    }
}

/// F18: Change Defaults over two pages; Enter keeps the values.
pub fn defaults(surface: &mut Surface, host: &Host, s: &mut Session) -> Flow {
    let mut vals: BTreeMap<String, String> = pdm::defaults::FIELDS
        .iter()
        .map(|f| ((*f).to_string(), s.defaults.get(f)))
        .collect();
    let mut page = 0;
    let mut message: Option<String> = None;
    loop {
        let v = |k: &str| vals.get(k).cloned().unwrap_or_default();
        let screen = prompt::defaults(&v, page, message.as_deref());
        message = None;
        let Some(a) = show(surface, &screen) else {
            return Flow::Gone;
        };
        for (k, x) in &a.vars {
            vals.insert(k.clone(), x.trim().to_ascii_uppercase());
        }
        match a.key {
            Key::F(3) => return Flow::Exit,
            Key::F(12) => return Flow::Back,
            Key::F(5) => {
                vals = pdm::defaults::FIELDS
                    .iter()
                    .map(|f| ((*f).to_string(), s.defaults.get(f)))
                    .collect();
            }
            Key::PageDown if page == 0 => page = 1,
            Key::PageUp if page == 1 => page = 0,
            Key::PageDown | Key::PageUp => {
                message = Some("There are no more values to show.".into());
            }
            Key::Enter => {
                let mut d = s.defaults.clone();
                let bad = vals.iter().find_map(|(k, x)| d.set(k, x).err());
                if let Some(e) = bad {
                    message = Some(e.to_string());
                } else {
                    {
                        let file_changed = d.option_file != s.defaults.option_file
                            || d.option_lib != s.defaults.option_lib
                            || d.option_member != s.defaults.option_member;
                        s.defaults = d;
                        if file_changed {
                            s.options = data::user_options(host, &s.defaults);
                        }
                        return Flow::Back;
                    }
                }
            }
            _ => message = Some("Function key not allowed.".into()),
        }
    }
}

/// The user-defined options as the list shows them.
fn option_rows(s: &Session) -> Vec<(String, String)> {
    s.options
        .iter()
        .map(|o| (o.code.clone(), o.command.clone()))
        .collect()
}

/// Work with User-Defined Options: 2 change, 3 copy, 4 delete, 5
/// display, F6 create; the file is written when the list is left with
/// F3 or F12, F15 leaves without writing.
#[allow(clippy::too_many_lines)] // one arm per key and option
pub fn work_options(job: &mut Job, host: &Host, surface: &mut Surface, s: &mut Session) -> Flow {
    let saved = s.options.clone();
    let mut top = 0;
    let mut key_set = 0;
    let mut cmdline = String::new();
    let mut position = String::new();
    let mut message: Option<String> = None;
    let mut typed: BTreeMap<usize, String> = BTreeMap::new();
    let system = crate::display::dds::system_name(host);
    let write = |s: &Session| data::save_user_options(host, &s.defaults, &s.options);
    loop {
        let rows = option_rows(s);
        let d = &s.defaults;
        let screen = user_options(&OptionsScreen {
            system: &system,
            file: (&d.option_file, &d.option_lib, &d.option_member),
            position: &position,
            rows: &rows,
            top,
            options: &typed,
            key_set,
            cmdline: &cmdline,
            message: message.as_deref(),
        });
        message = None;
        let Some(a) = show(surface, &screen) else {
            return Flow::Gone;
        };
        cmdline.clear();
        typed = a.options.clone();
        position = a.up("POSTO");
        if command_keys(job, host, s, &a, &mut cmdline, &mut message) {
            continue;
        }
        match a.key {
            Key::F(3 | 12) => {
                if s.options != saved
                    && let Err(e) = write(s)
                {
                    message = Some(format!("The options were not saved: {e}"));
                    continue;
                }
                return if a.key == Key::F(3) {
                    Flow::Exit
                } else {
                    Flow::Back
                };
            }
            Key::F(15) => {
                s.options = saved;
                return Flow::Back;
            }
            Key::F(24) => key_set = 1 - key_set,
            Key::F(5) => typed.clear(),
            Key::F(18) => {
                if defaults(surface, host, s) == Flow::Gone {
                    return Flow::Gone;
                }
            }
            Key::F(13) => {
                let list: Vec<(usize, String)> = typed.clone().into_iter().collect();
                typed = pdm::subset::repeat(&list, rows.len()).into_iter().collect();
            }
            Key::F(6) => {
                if let Some(m) = edit_option(surface, s, None, true) {
                    message = Some(m);
                }
            }
            Key::PageDown => {
                if top + OPTIONS_PAGE < rows.len() {
                    top += OPTIONS_PAGE;
                } else {
                    message = Some("Already at the bottom of the list.".into());
                }
            }
            Key::PageUp => top = top.saturating_sub(OPTIONS_PAGE),
            Key::Enter => {
                if !position.is_empty() {
                    if let Some(i) =
                        pdm::subset::position(rows.iter().map(|(c, _)| c.as_str()), &position)
                    {
                        top = i;
                    }
                    position.clear();
                }
                if typed.is_empty() && !a.cmdline.is_empty() {
                    s.last.clone_from(&a.cmdline);
                    message = exec(job, host, &a.cmdline);
                    continue;
                }
                let todo = std::mem::take(&mut typed);
                let mut deletes = Vec::new();
                for (i, opt) in todo {
                    let Some(o) = s.options.get(i).cloned() else {
                        continue;
                    };
                    match opt.as_str() {
                        "2" => message = edit_option(surface, s, Some(&o), true),
                        "3" => message = edit_option(surface, s, Some(&o), false),
                        "4" => deletes.push(o),
                        "5" => {
                            let v = |k: &str| match k {
                                "CODE" => o.code.clone(),
                                "CMD" => o.command.clone(),
                                _ => String::new(),
                            };
                            let screen =
                                prompt::user_option("Display User-Defined Option", false, &v, None);
                            if show(surface, &screen).is_none() {
                                return Flow::Gone;
                            }
                        }
                        other => message = Some(format!("Option {other} is not valid here.")),
                    }
                }
                if !deletes.is_empty() {
                    let rows: Vec<(String, String, String)> = deletes
                        .iter()
                        .map(|o| (o.code.clone(), String::new(), o.command.clone()))
                        .collect();
                    let screen = prompt::confirm("User-Defined Options", &rows, None);
                    match show(surface, &screen).map(|a| a.key) {
                        None => return Flow::Gone,
                        Some(Key::Enter) => {
                            s.options
                                .retain(|o| !deletes.iter().any(|d| d.code == o.code));
                            message = Some(format!("{} options deleted.", deletes.len()));
                        }
                        Some(_) => {}
                    }
                }
            }
            _ => message = Some("Function key not allowed.".into()),
        }
    }
}

/// Create (none), change or copy an option: the message to show.
fn edit_option(
    surface: &mut Surface,
    s: &mut Session,
    from: Option<&UserOption>,
    change: bool,
) -> Option<String> {
    let heading = match (from, change) {
        (None, _) => "Create User-Defined Option",
        (Some(_), true) => "Change User-Defined Option",
        (Some(_), false) => "Copy User-Defined Option",
    };
    let mut vals = BTreeMap::from([
        (
            "CODE".to_string(),
            if change {
                from.map(|o| o.code.clone()).unwrap_or_default()
            } else {
                String::new()
            },
        ),
        (
            "CMD".to_string(),
            from.map(|o| o.command.clone()).unwrap_or_default(),
        ),
    ]);
    let mut message: Option<String> = None;
    loop {
        let v = |k: &str| vals.get(k).cloned().unwrap_or_default();
        let shown = message.take();
        let screen = prompt::user_option(heading, true, &v, shown.as_deref());
        let a = show(surface, &screen)?;
        for (k, x) in &a.vars {
            vals.insert(k.clone(), x.trim().to_string());
        }
        match a.key {
            Key::F(3 | 12) => return None,
            Key::Enter => {
                let code = match check_code(&vals["CODE"]) {
                    Ok(c) => c,
                    Err(e) => {
                        message = Some(e.to_string());
                        continue;
                    }
                };
                let command = vals["CMD"].trim().to_string();
                if command.is_empty() {
                    message = Some("Type the command the option runs.".into());
                    continue;
                }
                let replacing = from.filter(|_| change).map(|o| o.code.clone());
                if s.options
                    .iter()
                    .any(|o| o.code == code && Some(&o.code) != replacing.as_ref())
                {
                    message = Some(format!("Option {code} already exists."));
                    continue;
                }
                if let Some(old) = replacing {
                    s.options.retain(|o| o.code != old);
                }
                s.options.push(UserOption {
                    code: code.clone(),
                    command,
                });
                s.options.sort_by(|a, b| a.code.cmp(&b.code));
                return Some(format!(
                    "Option {code} kept; it is written when you leave the list."
                ));
            }
            _ => message = Some("Function key not allowed.".into()),
        }
    }
}

/// Print lines to `QSYSPRT` as a spooled file; the message to show.
pub fn print(host: &Host, lines: &[String]) -> String {
    use spool::printer::{Control, PageFormat, Printer};
    let opened = host
        .link
        .borrow_mut()
        .with(|l| l.prt_open("QSYSPRT".into(), "*LIBL".into(), "*DFTACTGRP".into()));
    let (id, prtf) = match opened {
        Ok(o) => o,
        Err(e) => return format!("The list was not printed: {e}"),
    };
    let mut p = Printer::new(PageFormat {
        len: prtf.page_len,
        width: prtf.page_width,
        lpi: prtf.lpi,
        cpi: prtf.cpi,
        ovrflw: prtf.ovrflw,
        fold: prtf.fold,
    });
    for l in lines {
        p.write_str(l, Control::line());
    }
    let bytes = p.take();
    let put = host
        .link
        .borrow_mut()
        .with(|l| l.splf_put(id, bytes, p.pages, p.records));
    let closed = host.link.borrow_mut().with(|l| l.splf_close(id, prtf.hold));
    let r = put.and(closed);
    match r {
        Ok(()) => "The list was printed to QSYSPRT.".into(),
        Err(e) => format!("The list was not printed: {e}"),
    }
}
