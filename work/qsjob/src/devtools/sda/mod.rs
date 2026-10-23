// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Screen Design Aid in the job: `STRSDA` and its menu, designing
//! screens (the records of a display-file member, the design image,
//! fields, keywords, comments, saving and creating the file), designing
//! menus, and testing display files. The member model, the image, the
//! DDS writer and the displays' layouts are the `sda` crate's (IBM's
//! SDA displays are display files); members are read and written
//! through PDM's storage requests (320, 321), the objects made with the
//! commands that make them.

mod design;
mod fields;
mod image;
mod kw;
mod menus;
mod test;
mod ui;

use cmd::{Resolved, Value};
use mi::Job;
use stg::Client;

use crate::command::{Fail, Outcome};
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::panels::shows_panels;
use ui::Key;

/// What the operator chose last, kept over one `STRSDA`.
#[derive(Clone, Debug, Default)]
pub struct Session {
    /// Design Screens' and Design Menus' values.
    pub src: (String, String, String),
    /// Design Menus' values.
    pub menu: (String, String, String),
    /// Test Display File's values.
    pub test: (String, String, String, String),
    /// Change Defaults: a printed image lists the fields.
    pub flddef: bool,
    /// The copyright line has been shown.
    shown: bool,
}

impl Session {
    fn first(&mut self) -> bool {
        !std::mem::replace(&mut self.shown, true)
    }
}

/// How a display was left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    /// F12: back one display.
    Back,
    /// F3: leave.
    Exit,
    /// The display is gone.
    Gone,
}

/// The display went away while SDA waited on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gone;

/// A parameter's value as typed, upper case; `None` when not given.
fn parm(r: &Resolved, kwd: &str) -> Option<String> {
    r.get(kwd)
        .and_then(|b| b.value.as_ref())
        .map(Value::source)
        .map(|s| s.trim().trim_matches('\'').to_ascii_uppercase())
        .filter(|s| !s.is_empty() && s != "*SELECT")
}

/// A qualified parameter as `(name, library)`.
#[allow(clippy::many_single_char_names)] // r, v, s, l, n
fn qual(r: &Resolved, kwd: &str) -> (Option<String>, Option<String>) {
    let up = |v: &Value| Some(v.source().trim().to_ascii_uppercase()).filter(|s| !s.is_empty());
    match r.get(kwd).and_then(|b| b.value.as_ref()) {
        Some(Value::Qual(parts)) => (parts.first().and_then(up), parts.get(1).and_then(up)),
        Some(v) => {
            let s = v.source().trim().to_ascii_uppercase();
            match s.split_once('/') {
                Some((l, n)) => (Some(n.to_string()), Some(l.to_string())),
                None => (Some(s).filter(|s| !s.is_empty() && s != "*SELECT"), None),
            }
        }
        None => (None, None),
    }
}

/// `STRSDA`: the menu, or the option the command names.
pub fn run(job: &mut Job, host: &Host, r: &Resolved, out: &mut Outcome) -> Result<String, Fail> {
    let _ = out;
    if !shows_panels(host, r) {
        return Err(Fail::new(
            "CPF9898",
            "STRSDA needs a display; it cannot run in this job.",
        ));
    }
    let mut surface = Surface::new(host.clone());
    let mut s = Session {
        src: ("QDDSSRC".into(), "*LIBL".into(), String::new()),
        menu: (String::new(), "*LIBL".into(), String::new()),
        test: (String::new(), "*LIBL".into(), String::new(), String::new()),
        flddef: true,
        shown: false,
    };
    let (file, lib) = qual(r, "SRCFILE");
    if let Some(f) = file {
        s.src.0.clone_from(&f);
        s.menu.0 = f;
    }
    if let Some(l) = lib {
        s.src.1.clone_from(&l);
        s.menu.1 = l;
    }
    if let Some(m) = parm(r, "SRCMBR") {
        s.src.2.clone_from(&m);
        s.menu.2 = m;
    }
    let (tf, tl) = qual(r, "TSTFILE");
    if let Some(f) = tf {
        s.test.0 = f;
    }
    if let Some(l) = tl {
        s.test.1 = l;
    }
    let (src_given, menu_given) = (!s.src.2.is_empty(), !s.menu.2.is_empty());
    let mut option = parm(r, "OPTION");
    // PDM's option 17 on a menu member asks for option 1: a menu is
    // designed with option 2.
    if option.as_deref() == Some("1") && src_given {
        let lib = file_library(host, &s.src.1, &s.src.0).unwrap_or_default();
        if member_info(host, &lib, &s.src.0, &s.src.2).is_some_and(|(t, _, _)| t.trim() == "MNUDDS") {
            option = Some("2".into());
        }
    }
    let flow = match option.as_deref() {
        Some("1") => design::start(job, host, &mut surface, &mut s, src_given),
        Some("2") => menus::start(job, host, &mut surface, &mut s, menu_given),
        Some("3") => test::start(host, &mut surface, &mut s),
        _ => menu(job, host, &mut surface, &mut s),
    };
    if flow == Flow::Gone {
        return Err(Fail::new("CPF5140", "The display was lost."));
    }
    Ok(String::new())
}

/// The SDA menu until F3 or F12.
fn menu(job: &mut Job, host: &Host, surface: &mut Surface, s: &mut Session) -> Flow {
    let mut cmdline = String::new();
    let mut message: Option<String> = None;
    let mut last = String::new();
    loop {
        let first = s.first();
        let screen = sda::screen::menu::strsda(&cmdline, message.as_deref(), first);
        message = None;
        let Some(a) = ui::show(surface, &screen) else {
            return Flow::Gone;
        };
        cmdline.clear();
        let flow = match a.key() {
            Key::F(3 | 12) => return Flow::Exit,
            Key::F(4) => {
                message = exec(job, host, &format!("?{}", a.cmdline));
                last.clone_from(&a.cmdline);
                continue;
            }
            Key::F(9) => {
                cmdline.clone_from(&last);
                continue;
            }
            Key::F(1) | Key::Help => {
                message = Some("Choose an option, or type a command and press Enter.".into());
                continue;
            }
            Key::Enter => match a.cmdline.as_str() {
                "" => continue,
                "1" => design::start(job, host, surface, s, false),
                "2" => menus::start(job, host, surface, s, false),
                "3" => test::start(host, surface, s),
                text => {
                    last = text.to_string();
                    message = exec(job, host, text);
                    continue;
                }
            },
            _ => {
                message = Some("Function key not allowed.".into());
                continue;
            }
        };
        if flow == Flow::Gone {
            return Flow::Gone;
        }
    }
}

/// Run a command line: the first line of its output, or its error.
pub fn exec(job: &mut Job, host: &Host, text: &str) -> Option<String> {
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

/// Run a command; its error text when it failed.
pub fn must(job: &mut Job, host: &Host, text: &str) -> Result<(), String> {
    let out = crate::command::run(job, host, text);
    match out.escape {
        Some(f) => Err(f
            .text
            .lines()
            .last()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| format!("{} ended with {}.", out.name, f.msgid))),
        None => Ok(()),
    }
}

/// The library a `*LIBL`/`*CURLIB` file reference resolves to.
pub fn file_library(host: &Host, lib: &str, file: &str) -> Option<String> {
    host.link
        .borrow_mut()
        .with(|l| l.file(lib.into(), file.into()))
        .map(|f| f.lib)
}

/// A member's source type, text and change date (`yy-mm-dd`).
pub fn member_info(host: &Host, lib: &str, file: &str, member: &str) -> Option<(String, String, String)> {
    host.link
        .borrow_mut()
        .with(|l| l.members(lib.into(), file.into()))
        .into_iter()
        .find(|m| m.name.eq_ignore_ascii_case(member))
        .map(|m| (m.srctype, m.text, short_date(&m.changed)))
}

/// `CYYMMDDHHMMSS` as `yy-mm-dd`.
fn short_date(c: &str) -> String {
    if c.len() >= 7 {
        format!("{}-{}-{}", &c[1..3], &c[3..5], &c[5..7])
    } else {
        String::new()
    }
}

/// Today as `yy-mm-dd`, and the stamp SDA writes.
pub fn today(host: &Host) -> (String, sda::source::Stamp) {
    let env = crate::display::dds::env_of(host, "");
    let (y, m, d) = env.date;
    let (h, mi, sec) = env.time;
    (
        format!("{:02}-{m:02}-{d:02}", y % 100),
        sda::source::Stamp {
            date: format!("{y:04}{m:02}{d:02}"),
            time: format!("{h:02}{mi:02}{sec:02}"),
            user: host.start.user.clone(),
            release: release::RELEASE.to_string(),
        },
    )
}

/// A member's lines; `None` when it is not there.
pub fn read_member(host: &Host, lib: &str, file: &str, member: &str) -> Option<Vec<String>> {
    host.link
        .borrow_mut()
        .with(|l| l.pdm_read(lib.into(), file.into(), member.into()))
        .ok()
        .flatten()
}

/// Write a member's lines, the member made when missing.
pub fn write_member(host: &Host, lib: &str, file: &str, member: &str, lines: Vec<String>, srctype: &str) -> Result<(), String> {
    host.link.borrow_mut().with(|l| {
        l.pdm_write(lib.into(), file.into(), member.into(), lines, srctype.into())
    })
}

/// Whether a name is a valid object or member name.
pub fn valid_name(n: &str) -> bool {
    let mut c = n.chars();
    c.next().is_some_and(|f| f.is_ascii_uppercase() || "$#@".contains(f))
        && n.len() <= 10
        && c.all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || "$#@_.".contains(ch))
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
        Err(e) => return format!("The image was not printed: {e}"),
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
    match put.and(closed) {
        Ok(()) => "The image was printed to QSYSPRT.".into(),
        Err(e) => format!("The image was not printed: {e}"),
    }
}
