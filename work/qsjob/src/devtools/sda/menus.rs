// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Option 2, Design Menus: the menu's member (type `MNUDDS`, a new one
//! laid out from `sda::menu::template`), its image on the design image,
//! its options' commands (member `<menu>QQ`, type `MNUCMD`), and saving
//! them with the display file, the message file and the `*MENU`.

use std::collections::BTreeMap;

use mi::Job;
use sda::menu;
use sda::screen::{entry, menus as mscreen};

use super::design::{Work, open};
use super::ui::{self, Key};
use super::{Flow, Session, file_library, must, read_member, write_member};
use crate::console::host::Host;
use crate::display::surface::Surface;

/// Design Menus, then the menu's functions.
pub fn start(job: &mut Job, host: &Host, surface: &mut Surface, s: &mut Session, prefilled: bool) -> Flow {
    let mut message: Option<String> = None;
    let mut skip = prefilled;
    loop {
        if skip {
            skip = false;
        } else {
            let mut vals = BTreeMap::new();
            vals.insert("SRCFILE".to_string(), s.menu.0.clone());
            vals.insert("SRCLIB".to_string(), s.menu.1.clone());
            vals.insert("SRCMBR".to_string(), s.menu.2.clone());
            let screen = entry::design_menus(&|k| vals.get(k).cloned().unwrap_or_default(), message.as_deref());
            message = None;
            let Some(a) = ui::show(surface, &screen) else {
                return Flow::Gone;
            };
            match a.key() {
                Key::F(3) => return Flow::Exit,
                Key::F(12) => return Flow::Back,
                Key::Enter => {}
                _ => {
                    message = Some("Function key not allowed.".into());
                    continue;
                }
            }
            s.menu = (a.up("SRCFILE"), a.up("SRCLIB"), a.up("SRCMBR"));
        }
        let (file, lib, name) = s.menu.clone();
        let Some(mut w) = open(host, &file, &lib, &name, "MNUDDS", &mut message) else {
            continue;
        };
        if w.read.is_empty() {
            w.m = menu::template(&name);
        }
        let qq = menu::commands_member(&name);
        let mut commands = read_member(host, &w.lib, &w.file, &qq)
            .map(|l| menu::commands_parse(&l))
            .unwrap_or_default();
        match functions(job, host, surface, s, &mut w, &mut commands) {
            Flow::Gone => return Flow::Gone,
            Flow::Exit if prefilled => return Flow::Exit,
            _ => {}
        }
    }
}

/// Specify Menu Functions until the menu is left.
fn functions(job: &mut Job, host: &Host, surface: &mut Surface, s: &mut Session, w: &mut Work, commands: &mut BTreeMap<u8, String>) -> Flow {
    let mut message: Option<String> = None;
    loop {
        let vals: BTreeMap<String, String> = [("IMAGE", "Y"), ("HELP", "")]
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        let screen = mscreen::functions(&w.member, &|k| vals.get(k).cloned().unwrap_or_default(), message.as_deref());
        message = None;
        let Some(a) = ui::show(surface, &screen) else {
            return Flow::Gone;
        };
        match a.key() {
            Key::F(3) => return save(job, host, surface, w, commands),
            Key::F(12) => return Flow::Back,
            Key::Enter => {}
            _ => {
                message = Some("Function key not allowed.".into());
                continue;
            }
        }
        if a.up("HELP") == "Y" {
            message = Some(format!(
                "Write the menu's help in a panel group named {}; SDA keeps no help member.",
                w.member
            ));
        }
        if a.up("IMAGE") != "Y" {
            continue;
        }
        let Some(ri) = w.m.record(&w.member) else {
            message = Some(format!("Member {} has no record {}.", w.member, w.member));
            continue;
        };
        if super::image::edit(job, host, surface, s, &mut w.m, ri) == Flow::Gone {
            return Flow::Gone;
        }
        if menu::has_fields(&w.m.records[ri]) {
            message = Some("A menu shows constants only: its fields are not used.".into());
        }
        let options = menu::options(&w.m.records[ri]);
        if define_commands(surface, &w.member, &options, commands) == Flow::Gone {
            return Flow::Gone;
        }
    }
}

/// Define Menu Commands for the options on the image.
fn define_commands(surface: &mut Surface, name: &str, options: &[u8], commands: &mut BTreeMap<u8, String>) -> Flow {
    let mut top = 0;
    let mut message: Option<String> = None;
    loop {
        let rows: Vec<(u8, String)> = options
            .iter()
            .map(|o| (*o, commands.get(o).cloned().unwrap_or_default()))
            .collect();
        let screen = mscreen::commands(name, &rows, top, message.as_deref());
        message = None;
        let Some(a) = ui::show(surface, &screen) else {
            return Flow::Gone;
        };
        for (i, (o, _)) in rows.iter().enumerate() {
            if let Some(c) = a.cells.get(&(i, "CMD".to_string())) {
                if c.trim().is_empty() {
                    commands.remove(o);
                } else {
                    commands.insert(*o, c.trim().to_string());
                }
            }
        }
        match a.key() {
            Key::F(3 | 12) | Key::Enter => return Flow::Back,
            Key::PageDown if top + mscreen::PAGE < rows.len() => top += mscreen::PAGE,
            Key::PageUp => top = top.saturating_sub(mscreen::PAGE),
            Key::F(4) => message = Some("Type the command; prompt it from a command line.".into()),
            _ => message = Some("Function key not allowed.".into()),
        }
    }
}

/// Save Menu - Create Menu.
fn save(job: &mut Job, host: &Host, surface: &mut Surface, w: &mut Work, commands: &BTreeMap<u8, String>) -> Flow {
    let mut vals: BTreeMap<String, String> = [
        ("SAVE", "Y"),
        ("SRCFILE", w.file.as_str()),
        ("SRCLIB", w.lib.as_str()),
        ("CREATE", "Y"),
        ("LIB", w.lib.as_str()),
        ("REPLACE", "Y"),
    ]
    .iter()
    .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
    .collect();
    let mut message: Option<String> = None;
    loop {
        let screen = mscreen::save(&|k| vals.get(k).cloned().unwrap_or_default(), message.as_deref());
        let Some(a) = ui::show(surface, &screen) else {
            return Flow::Gone;
        };
        for k in ["SAVE", "SRCFILE", "SRCLIB", "CREATE", "LIB", "REPLACE"] {
            vals.insert(k.to_string(), a.up(k));
        }
        match a.key() {
            Key::F(12) => return Flow::Back,
            Key::F(3) => return Flow::Exit,
            Key::Enter => {}
            _ => {
                message = Some("Function key not allowed.".into());
                continue;
            }
        }
        let save = vals["SAVE"] == "Y";
        let create = vals["CREATE"] == "Y";
        if !save && !create {
            return Flow::Exit;
        }
        if create && !save {
            message = Some("The menu is made from the saved members: type Y to save them.".into());
            continue;
        }
        let (file, lib) = (vals["SRCFILE"].clone(), vals["SRCLIB"].clone());
        let Some(real) = file_library(host, &lib, &file) else {
            message = Some(format!("File {file} in library {lib} not found."));
            continue;
        };
        let lines = w.lines(host);
        let qq = menu::commands_member(&w.member);
        let r = write_member(host, &real, &file, &w.member, lines, "MNUDDS")
            .and_then(|()| write_member(host, &real, &file, &qq, menu::commands_source(commands), "MNUCMD"));
        if let Err(e) = r {
            message = Some(e);
            continue;
        }
        if create {
            let objlib = if vals["LIB"].is_empty() { real.clone() } else { vals["LIB"].clone() };
            if vals["REPLACE"] == "Y" {
                let _ = must(job, host, &format!("DLTMNU MENU({objlib}/{})", w.member));
                let _ = must(job, host, &format!("DLTMSGF MSGF({objlib}/{qq})"));
                let _ = must(job, host, &format!("DLTF FILE({objlib}/{})", w.member));
            }
            for line in menu::create_commands(&w.member, &file, &real, &objlib, commands) {
                if let Err(e) = must(job, host, &line) {
                    message = Some(e);
                    break;
                }
            }
            if message.is_some() {
                continue;
            }
        }
        return Flow::Exit;
    }
}
