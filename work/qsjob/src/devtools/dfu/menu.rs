// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `STRDFU`: the menu, its entry panels, and creating, changing and
//! deleting DFU programs. The entry panels remember what was typed for
//! the session, as IBM's do.

// Throughout: `v` the panels' values.
#![allow(clippy::many_single_char_names)]
use std::collections::BTreeMap;

use dfu::Program;
use dfu::screen::prompt::{self, Panel};
use dfu::screen::{define, menu};
use mi::Job;

use super::ui::{Key, show};
use super::{data, temp};
use crate::console::host::Host;
use crate::display::surface::Surface;

/// The entry panels' values, by field.
#[derive(Clone, Debug, Default)]
pub struct Values(BTreeMap<String, String>);

impl Values {
    /// Set a value.
    pub fn set(&mut self, k: &str, v: String) {
        self.0.insert(k.to_string(), v);
    }

    fn get(&self, k: &str) -> String {
        self.0.get(k).cloned().unwrap_or_default()
    }
}

/// How a display was left.
enum Left {
    /// Enter: go on.
    Enter,
    /// F12: back one display.
    Back,
    /// F3, or the display is gone: leave DFU.
    Exit,
}

/// Run a command line: its first line of output or its error.
fn exec(job: &mut Job, host: &Host, text: &str) -> Option<String> {
    let out = crate::command::run(job, host, text);
    if let Some(f) = &out.escape {
        let line = f.text.lines().last().unwrap_or("").trim().to_string();
        return Some(if line.is_empty() {
            format!("The command ended with {}.", f.msgid)
        } else {
            line
        });
    }
    out.text
        .lines()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim().to_string())
}

/// The menu until F3 or F12; `option` starts at one of its options.
pub fn strdfu(job: &mut Job, host: &Host, surface: &mut Surface, option: &str, mut v: Values) {
    let mut message: Option<String> = None;
    let mut cmdline = String::new();
    let mut last = String::new();
    let mut first = true;
    let mut pending = (option != "*SELECT").then(|| option.to_string());
    loop {
        let pick = if let Some(p) = pending.take() {
            p
        } else {
            let screen = menu::strdfu(&cmdline, message.as_deref(), first);
            first = false;
            message = None;
            cmdline.clear();
            let Some(a) = show(surface, &screen) else {
                return;
            };
            match a.key {
                Key::F(3 | 12) => return,
                Key::F(9) => {
                    cmdline.clone_from(&last);
                    continue;
                }
                Key::F(4) if !a.cmdline.is_empty() => {
                    last.clone_from(&a.cmdline);
                    message = exec(job, host, &format!("?{}", a.cmdline));
                    continue;
                }
                Key::Enter => a.cmdline.clone(),
                _ => {
                    message = Some("That function key cannot be used here.".into());
                    continue;
                }
            }
        };
        let left = match pick.as_str() {
            "" => continue,
            "1" => run_menu(job, host, surface, &mut v, &mut message),
            "2" => create(host, surface, &mut v, &mut message, false),
            "3" => create(host, surface, &mut v, &mut message, true),
            "4" => delete(host, surface, &mut v, &mut message),
            "5" => temporary(host, surface, &mut v, &mut message),
            n if n.parse::<u32>().is_ok() => {
                message = Some(format!("Selection {n} is not on the menu."));
                continue;
            }
            cmd => {
                last = cmd.to_string();
                message = exec(job, host, cmd);
                continue;
            }
        };
        if matches!(left, Left::Exit) {
            return;
        }
    }
}

/// An entry panel until Enter, F12 or F3, the values kept.
fn entry(surface: &mut Surface, p: &Panel, v: &mut Values, message: Option<&str>) -> Left {
    let screen = prompt::panel(p, &|k| v.get(k), message);
    let Some(a) = show(surface, &screen) else {
        return Left::Exit;
    };
    for en in p.entries {
        v.set(en.key, a.up(en.key));
    }
    match a.key {
        Key::Enter => Left::Enter,
        Key::F(4 | 12) => Left::Back,
        _ => Left::Exit,
    }
}

fn blank_to(v: &str, dft: &str) -> String {
    if v.trim().is_empty() {
        dft.to_string()
    } else {
        v.to_string()
    }
}

/// Run a DFU Program: change or display a file with one.
fn run_menu(job: &mut Job, host: &Host, surface: &mut Surface, v: &mut Values, msg: &mut Option<String>) -> Left {
    let mut message: Option<String> = None;
    loop {
        let screen = menu::run("", message.as_deref());
        message = None;
        let Some(a) = show(surface, &screen) else {
            return Left::Exit;
        };
        let display = match a.key {
            Key::F(3) => return Left::Exit,
            Key::F(12) => return Left::Back,
            Key::Enter => match a.cmdline.as_str() {
                "1" => false,
                "2" => true,
                "" => continue,
                cmd => {
                    message = exec(job, host, cmd);
                    continue;
                }
            },
            _ => continue,
        };
        let p = if display {
            &prompt::RUN_DISPLAY
        } else {
            &prompt::RUN_CHANGE
        };
        let mut m: Option<String> = None;
        loop {
            match entry(surface, p, v, m.take().as_deref()) {
                Left::Exit => return Left::Exit,
                Left::Back => break,
                Left::Enter => {}
            }
            match run_program(host, surface, v, display) {
                Ok(()) => {
                    *msg = None;
                    break;
                }
                Err(e) => m = Some(e),
            }
        }
    }
}

fn run_program(host: &Host, surface: &mut Surface, v: &Values, display: bool) -> Result<(), String> {
    let name = v.get("PGM");
    if name.is_empty() {
        return Err("Type the name of a DFU program.".into());
    }
    let lib = blank_to(&v.get("PGMLIB"), "*LIBL");
    let (_, p) = data::program(host, &lib, &name)?
        .ok_or_else(|| format!("DFU program {name} in library {lib} not found."))?;
    let file = blank_to(&v.get("FILE"), "*SAME");
    let (file, flib) = if file == "*SAME" {
        (p.file.clone(), p.file_lib.clone())
    } else {
        (file, blank_to(&v.get("FILELIB"), "*LIBL"))
    };
    let f = data::file(host, &flib, &file)
        .ok_or_else(|| format!("Data file {file} in library {flib} not found."))?;
    let mbr = Some(v.get("MBR")).filter(|m| !m.is_empty() && m != "*FIRST");
    temp::run(host, surface, &f, mbr, Some(&p), display)
}

/// Create (or change) a DFU program: the entry panel, then Define
/// General Information; Enter there stores the program.
fn create(host: &Host, surface: &mut Surface, v: &mut Values, msg: &mut Option<String>, change: bool) -> Left {
    let panel = if change { &prompt::CHANGE } else { &prompt::CREATE };
    let mut m: Option<String> = None;
    loop {
        match entry(surface, panel, v, m.take().as_deref()) {
            Left::Enter => {}
            other => return other,
        }
        let name = v.get("PGM");
        if name.is_empty() {
            m = Some("Type the name of the DFU program.".into());
            continue;
        }
        let lib = blank_to(&v.get("PGMLIB"), if change { "*LIBL" } else { "*CURLIB" });
        let found = match data::program(host, &lib, &name) {
            Ok(f) => f,
            Err(e) => {
                m = Some(e);
                continue;
            }
        };
        let mut p = match (found, change) {
            (Some(_), false) => {
                m = Some(format!("DFU program {name} already exists in library {lib}."));
                continue;
            }
            (None, true) => {
                m = Some(format!("DFU program {name} in library {lib} not found."));
                continue;
            }
            (Some((_, p)), true) => p,
            (None, false) => Program::new(&name, "*LIBL", ""),
        };
        let file = v.get("FILE");
        if !(file.is_empty() || file == "*SAME") {
            p.file = file;
            p.file_lib = blank_to(&v.get("FILELIB"), "*LIBL");
        }
        if p.file.is_empty() {
            m = Some("Type the name of the data file.".into());
            continue;
        }
        let Some(f) = data::file(host, &p.file_lib, &p.file) else {
            m = Some(format!("Data file {} in library {} not found.", p.file, p.file_lib));
            continue;
        };
        match general(surface, &mut p, f.keyed) {
            Left::Exit => return Left::Exit,
            Left::Back => continue,
            Left::Enter => {}
        }
        *msg = Some(match data::save_program(host, &lib, &name, &p) {
            Ok(real) => format!("DFU program {name} was stored in library {real}."),
            Err(e) => e,
        });
        return Left::Back;
    }
}

/// Define General Information until Enter (the choices taken), F12
/// (twice when something was typed) or F3.
fn general(surface: &mut Surface, p: &mut Program, keyed: bool) -> Left {
    let mut message: Option<String> = None;
    let mut warned = false;
    loop {
        let screen = define::general(p, keyed, message.take().as_deref());
        let Some(a) = show(surface, &screen) else {
            return Left::Exit;
        };
        let value = |k: &str| a.vars.get(k).map(|v| v.trim().to_string()).unwrap_or_default();
        if let Err(e) = define::apply(p, &value) {
            message = Some(e);
            continue;
        }
        match a.key {
            Key::Enter => return Left::Enter,
            Key::F(3) => return Left::Exit,
            Key::F(12) if warned => return Left::Back,
            Key::F(12) => {
                warned = true;
                message = Some("The definition is not stored yet: F12 again leaves it.".into());
            }
            Key::F(14) => {
                message = Some(format!(
                    "Over file {}/{}, display format {}.",
                    p.file_lib,
                    p.file,
                    p.format.code()
                ));
            }
            _ => message = Some("That function key cannot be used here.".into()),
        }
    }
}

/// Delete a DFU program.
fn delete(host: &Host, surface: &mut Surface, v: &mut Values, msg: &mut Option<String>) -> Left {
    let mut m: Option<String> = None;
    loop {
        match entry(surface, &prompt::DELETE, v, m.take().as_deref()) {
            Left::Enter => {}
            other => return other,
        }
        let name = v.get("PGM");
        let lib = blank_to(&v.get("PGMLIB"), "*CURLIB");
        match data::delete_program(host, &lib, &name) {
            Ok(()) => {
                *msg = Some(format!("DFU program {name} was deleted."));
                return Left::Back;
            }
            Err(e) => m = Some(e),
        }
    }
}

/// Update Data Using Temporary Program.
fn temporary(host: &Host, surface: &mut Surface, v: &mut Values, msg: &mut Option<String>) -> Left {
    let mut m: Option<String> = None;
    loop {
        match entry(surface, &prompt::TEMP, v, m.take().as_deref()) {
            Left::Enter => {}
            other => return other,
        }
        let file = v.get("FILE");
        let lib = blank_to(&v.get("FILELIB"), "*LIBL");
        let Some(f) = data::file(host, &lib, &file) else {
            m = Some(format!("Data file {file} in library {lib} not found."));
            continue;
        };
        let mbr = Some(v.get("MBR")).filter(|x| !x.is_empty() && x != "*FIRST");
        match temp::run(host, surface, &f, mbr, None, false) {
            Ok(()) => {
                *msg = None;
                return Left::Back;
            }
            Err(e) => m = Some(e),
        }
    }
}
