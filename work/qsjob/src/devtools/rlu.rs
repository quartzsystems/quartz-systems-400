// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Report Layout Utility in the job, kept small: `STRRLU` shows a
//! printer file member's DDS as a page image (`dfu::rlu`), the operator
//! types constants over it, `I`, `D` and `DR` in the prefix area insert
//! and delete lines and add a record format, and Exit RLU writes the
//! DDS back (and creates the printer file when asked). The member is
//! read and written through PDM's requests (320, 321).

use std::fmt::Write as _;

use cmd::Resolved;
use dds::{CompileOpts, NoRefs, Source};
use dfu::rlu::{Image, Line, MAX_WIDTH};
use dfu::screen::rlu::{self as screen, Design, IMAGE_COLS, IMAGE_ROWS, ImageLine, banner};
use mi::Job;
use stg::Client;

use super::dfu::ui::{Key, show};
use super::pdm::{parm, qual};
use crate::command::{Fail, Outcome};
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::panels::shows_panels;

/// The session: where the member lives and its image.
struct Session {
    lib: String,
    file: String,
    member: String,
    text: String,
    image: Image,
    browse: bool,
}

/// `STRRLU`.
pub fn run(job: &mut Job, host: &Host, r: &Resolved, out: &mut Outcome) -> Result<String, Fail> {
    let _ = out;
    let (file, lib) = qual(r, "SRCFILE");
    let file = file.filter(|f| f != "*PRV").unwrap_or_else(|| "QDDSSRC".into());
    let lib = lib.filter(|l| l != "*PRV").unwrap_or_else(|| "*LIBL".into());
    let member = parm(r, "SRCMBR")
        .filter(|m| m != "*PRV")
        .ok_or_else(|| Fail::new("CPD0071", "Name the source member to work with."))?;
    let real = host
        .link
        .borrow_mut()
        .with(|l| l.file(lib.clone(), file.clone()))
        .map(|f| f.lib)
        .ok_or_else(|| {
            Fail::new(
                "CPF9812",
                format!("Source file {file} in library {lib} not found."),
            )
        })?;
    let lines = host
        .link
        .borrow_mut()
        .with(|l| l.pdm_read(real.clone(), file.clone(), member.clone()))
        .map_err(|e| Fail::new("CPF9898", e))?
        .unwrap_or_default();
    let (src, _) = Source::from_text(&lines.join("\n"));
    let comp = dds::compile_prtf(&src, &NoRefs, &CompileOpts::default());
    let mut message = None;
    let def = comp.definition.unwrap_or_else(|| {
        if !lines.is_empty() {
            message = Some("The member's DDS has errors; only what could be read is shown.".to_string());
        }
        dds::prtf::PrtfDefinition::default()
    });
    let option = parm(r, "OPTION").unwrap_or_else(|| "2".into());
    let text = parm(r, "TEXT").filter(|t| t != "*BLANK").unwrap_or_default();
    let mut s = Session {
        lib: real,
        file,
        member,
        text,
        image: Image::new(def),
        browse: option == "5",
    };
    if option == "6" {
        return Ok(print(&s));
    }
    if !shows_panels(host, r) {
        return Err(Fail::new(
            "CPF9898",
            "STRRLU needs a display; it cannot run in this job.",
        ));
    }
    let mut surface = Surface::new(host.clone());
    edit(job, host, &mut surface, &mut s, message);
    Ok(String::new())
}

/// The image as text: `OPTION(6)`.
fn print(s: &Session) -> String {
    let mut out = format!("{}/{}({})\n", s.lib, s.file, s.member);
    for l in &s.image.lines {
        let name = s
            .image
            .def
            .formats
            .get(l.format)
            .map_or("", |f| f.name.as_str());
        let _ = writeln!(out, "{name:<10} {}", l.text);
    }
    out
}

/// The display's lines: the page banner, the image, the end banner.
fn shown(s: &Session, top: usize, from: usize) -> (Vec<ImageLine>, Vec<Option<usize>>) {
    let mut all: Vec<(ImageLine, Option<usize>)> = vec![(
        ImageLine {
            prefix: String::new(),
            text: banner("Start of Page 001"),
            editable: false,
        },
        None,
    )];
    for (i, Line { format, text }) in s.image.lines.iter().enumerate() {
        let name = s
            .image
            .def
            .formats
            .get(*format)
            .map_or(String::new(), |f| f.name.clone());
        let t: String = text.chars().skip(from - 1).take(IMAGE_COLS).collect();
        all.push((
            ImageLine {
                prefix: name,
                text: t,
                editable: !s.browse,
            },
            Some(i),
        ));
    }
    all.push((
        ImageLine {
            prefix: String::new(),
            text: banner("End of Report"),
            editable: false,
        },
        None,
    ));
    all.into_iter().skip(top).take(IMAGE_ROWS).unzip()
}

fn edit(job: &mut Job, host: &Host, surface: &mut Surface, s: &mut Session, mut message: Option<String>) {
    let mut top = 0usize;
    let mut from = 1usize;
    let mut key_set = 0usize;
    loop {
        let (lines, index) = shown(s, top, from);
        let d = Design {
            lib: s.lib.clone(),
            file: s.file.clone(),
            member: s.member.clone(),
            from,
            cmdline: String::new(),
            lines: lines.clone(),
            key_set,
            message: message.take(),
        };
        let Some(a) = show(surface, &screen::design(&d)) else {
            return;
        };
        // What was typed over the image, then the line commands.
        let mut commands = Vec::new();
        for (i, l) in lines.iter().enumerate() {
            let Some(Some(at)) = index.get(i) else { continue };
            if let Some(t) = a.vars.get(&format!("L{i}"))
                && t.trim_end() != l.text.trim_end()
            {
                s.image.type_over(*at, from, &format!("{t:<IMAGE_COLS$}"));
            }
            if let Some(p) = a.vars.get(&format!("P{i}")) {
                let p = p.trim().to_ascii_uppercase();
                if !p.is_empty() && p != l.prefix.chars().take(7).collect::<String>() {
                    commands.push((*at, p));
                }
            }
        }
        // From the bottom up, so the indexes hold.
        commands.sort_by_key(|c| std::cmp::Reverse(c.0));
        for (at, c) in commands {
            match c.as_str() {
                "I" => s.image.insert_after(at),
                "D" => s.image.delete(at),
                "DR" => add_format(s, at),
                other => message = Some(format!("Line command {other} is not one RLU knows.")),
            }
        }
        match a.key {
            Key::Enter => match a.cmdline.trim().to_ascii_uppercase().as_str() {
                "" => {}
                "SAVE" => message = Some(save(host, s)),
                "FILE" => {
                    let _ = save(host, s);
                    return;
                }
                "CANCEL" => return,
                other => message = Some(format!("RLU does not know the command {other}.")),
            },
            Key::F(3) => {
                if exit(job, host, surface, s) {
                    return;
                }
            }
            Key::F(24) => key_set += 1,
            Key::F(19) => from = from.saturating_sub(IMAGE_COLS - 1).max(1),
            Key::F(20) => from = (from + IMAGE_COLS - 1).min(MAX_WIDTH - IMAGE_COLS + 1),
            Key::PageDown => {
                if top + IMAGE_ROWS < s.image.lines.len() + 2 {
                    top += IMAGE_ROWS;
                }
            }
            Key::PageUp => top = top.saturating_sub(IMAGE_ROWS),
            _ => message = Some("That function is not in this RLU.".into()),
        }
    }
}

/// A new record format after a line: `DR` in the prefix area.
fn add_format(s: &mut Session, after: usize) {
    let n = s.image.def.formats.len() + 1;
    s.image.def.formats.push(dds::prtf::PrtFormat {
        name: format!("RCD{n:03}"),
        text: None,
        spacing: dds::prtf::Spacing {
            spaceb: 1,
            ..dds::prtf::Spacing::default()
        },
        fields: Vec::new(),
    });
    let at = (after + 1).min(s.image.lines.len());
    s.image.lines.insert(
        at,
        Line {
            format: n - 1,
            text: String::new(),
        },
    );
    s.image.changed = true;
}

/// Write the DDS back; the message.
fn save(host: &Host, s: &mut Session) -> String {
    let lines = s.image.dds();
    let r = host.link.borrow_mut().with(|l| {
        l.pdm_write(
            s.lib.clone(),
            s.file.clone(),
            s.member.clone(),
            lines,
            "PRTF".into(),
        )
    });
    match r {
        Ok(()) => {
            s.image.changed = false;
            format!("Member {} was saved in {}/{}.", s.member, s.lib, s.file)
        }
        Err(e) => e,
    }
}

/// Exit RLU; `true` to leave.
fn exit(job: &mut Job, host: &Host, surface: &mut Surface, s: &mut Session) -> bool {
    let mut v = std::collections::BTreeMap::<String, String>::new();
    let dft = [
        ("OPTION", if s.image.changed { "1" } else { "2" }),
        ("CRTPRTF", "N"),
        ("CRTDFT", "N"),
        ("PROTO", "N"),
        ("PROTODFT", "N"),
        ("BATCH", "N"),
        ("JOBD", "*USRPRF"),
    ];
    for (k, x) in dft {
        v.insert(k.into(), x.into());
    }
    v.insert("MBR".into(), s.member.clone());
    v.insert("FILE".into(), s.file.clone());
    v.insert("LIB".into(), s.lib.clone());
    v.insert("TEXT".into(), s.text.clone());
    let mut message: Option<String> = None;
    loop {
        let screen = screen::exit(&|k| v.get(k).cloned().unwrap_or_default(), message.as_deref());
        let Some(a) = show(surface, &screen) else {
            return true;
        };
        for (k, x) in &a.vars {
            let x = if k == "TEXT" { x.trim().to_string() } else { x.trim().to_ascii_uppercase() };
            v.insert(k.clone(), x);
        }
        match a.key {
            Key::F(12) => return false,
            Key::F(5) => continue,
            Key::Enter => {}
            _ => {
                message = Some("That function key cannot be used here.".into());
                continue;
            }
        }
        match v.get("OPTION").map_or("", String::as_str) {
            "1" => {
                s.member.clone_from(&v["MBR"]);
                s.file.clone_from(&v["FILE"]);
                s.lib.clone_from(&v["LIB"]);
                s.text.clone_from(&v["TEXT"]);
                let m = save(host, s);
                if s.image.changed {
                    message = Some(m);
                    continue;
                }
                if v.get("PROTO").map(String::as_str) == Some("Y") {
                    message = Some("This RLU makes no prototype report; type N.".into());
                    continue;
                }
                if v.get("CRTPRTF").map(String::as_str) == Some("Y") {
                    let cmd = format!(
                        "CRTPRTF FILE({lib}/{m}) SRCFILE({lib}/{f}) SRCMBR({m})",
                        lib = s.lib,
                        f = s.file,
                        m = s.member
                    );
                    let out = crate::command::run(job, host, &cmd);
                    if let Some(f) = out.escape {
                        message = Some(format!("The printer file was not created ({}).", f.msgid));
                        continue;
                    }
                }
                return true;
            }
            "2" => return true,
            "3" => return false,
            _ => message = Some("Type 1, 2 or 3.".into()),
        }
    }
}
