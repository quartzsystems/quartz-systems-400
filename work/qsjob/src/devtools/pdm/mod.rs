// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Programming Development Manager in the job: `STRPDM` and its
//! menu, the three lists (`WRKLIBPDM`, `WRKOBJPDM`, `WRKMBRPDM`) with
//! their options, user-defined options and session defaults, and
//! `FNDSTRPDM`. The logic and the displays' layouts are the `pdm`
//! crate's (IBM's PDM displays are display files, not panel groups);
//! the data comes through the storage service (requests 320-325 and
//! the object and member lists).

mod data;
mod find;
mod lists;
mod panels;
mod ui;

use cmd::{Resolved, Value};
use mi::Job;
use pdm::Defaults;
use pdm::find::Query;
use pdm::uopt::UserOption;
use stg::Client;

use crate::command::{Fail, Outcome};
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::panels::shows_panels;

/// A PDM session: the user's defaults and options, the last command.
pub struct Session {
    /// The defaults (F18).
    pub defaults: Defaults,
    /// The user-defined options (F16).
    pub options: Vec<UserOption>,
    /// The last command run from a command line, for F9.
    pub last: String,
    /// The user.
    pub user: String,
    /// The copyright line has been shown.
    shown: bool,
}

impl Session {
    fn open(host: &Host) -> Self {
        let defaults = data::defaults(host);
        let options = data::user_options(host, &defaults);
        let user = host.link.borrow_mut().with(Client::whoami).user;
        Self {
            defaults,
            options,
            last: String::new(),
            user,
            shown: false,
        }
    }

    /// Whether this is the session's first display (the copyright
    /// line shows on it).
    pub fn first(&mut self) -> bool {
        !std::mem::replace(&mut self.shown, true)
    }

    /// Keep the defaults (and the `*PRV` values) for the next session,
    /// unless they are not to be saved.
    fn save(&self, host: &Host) {
        if self.defaults.save_session {
            let _ = data::save_defaults(host, &self.defaults);
        }
    }
}

/// A parameter's value as typed, upper case; `None` when not given.
pub(crate) fn parm(r: &Resolved, kwd: &str) -> Option<String> {
    r.get(kwd)
        .and_then(|b| b.value.as_ref())
        .map(|v| match v {
            Value::List(items) => items.first().map(Value::source).unwrap_or_default(),
            other => other.source(),
        })
        .map(|s| s.trim().trim_matches('\'').to_ascii_uppercase())
        .filter(|s| !s.is_empty())
}

/// A qualified parameter as `(name, library)`.
#[allow(clippy::many_single_char_names)] // r, v, s, l, n
pub(crate) fn qual(r: &Resolved, kwd: &str) -> (Option<String>, Option<String>) {
    let up = |v: &Value| Some(v.source().trim().to_ascii_uppercase()).filter(|s| !s.is_empty());
    match r.get(kwd).and_then(|b| b.value.as_ref()) {
        Some(Value::Qual(parts)) => (parts.first().and_then(up), parts.get(1).and_then(up)),
        Some(v) => {
            let s = v.source().trim().to_ascii_uppercase();
            match s.split_once('/') {
                Some((l, n)) => (Some(n.to_string()), Some(l.to_string())),
                None => (Some(s).filter(|s| !s.is_empty()), None),
            }
        }
        None => (None, None),
    }
}

/// A list parameter's values.
fn values(r: &Resolved, kwd: &str) -> Vec<String> {
    match r.get(kwd).and_then(|b| b.value.as_ref()) {
        Some(Value::List(items)) => items
            .iter()
            .map(|v| v.source().trim().to_ascii_uppercase())
            .collect(),
        Some(v) => vec![v.source().trim().to_ascii_uppercase()],
        None => Vec::new(),
    }
}

/// An element of a group parameter.
fn elem(r: &Resolved, kwd: &str, i: usize) -> Option<String> {
    match r.get(kwd).and_then(|b| b.value.as_ref()) {
        Some(Value::Elems(e)) => e
            .get(i)
            .and_then(Option::as_ref)
            .map(|v| v.source().trim().trim_matches('\'').to_ascii_uppercase()),
        Some(v) if i == 0 => Some(v.source().trim().to_ascii_uppercase()),
        _ => None,
    }
}

/// A PDM command: its display, or for `FNDSTRPDM` without one the
/// printed report.
pub fn run(job: &mut Job, host: &Host, r: &Resolved, out: &mut Outcome) -> Result<String, Fail> {
    let _ = out;
    if !shows_panels(host, r) {
        if r.name == "FNDSTRPDM" {
            return fndstrpdm_text(host, r);
        }
        return Err(Fail::new(
            "CPF9898",
            format!("{} needs a display; it cannot run in this job.", r.name),
        ));
    }
    let mut surface = Surface::new(host.clone());
    let mut s = Session::open(host);
    let res: Result<String, Fail> = match r.name.as_str() {
        "STRPDM" => panels::strpdm(job, host, &mut surface, &mut s).map(|()| String::new()),
        "WRKLIBPDM" => {
            let lib = parm(r, "LIB").unwrap_or_else(|| "*PRV".into());
            lists::libraries(job, host, &mut surface, &mut s, &lib).map(|_| String::new())
        }
        "WRKOBJPDM" => {
            let spec = lists::ObjSpec {
                lib: parm(r, "LIB").unwrap_or_else(|| "*PRV".into()),
                obj: parm(r, "OBJ").unwrap_or_else(|| "*PRV".into()),
                ty: parm(r, "OBJTYPE").unwrap_or_else(|| "*PRV".into()),
                attr: parm(r, "OBJATR").unwrap_or_else(|| "*PRV".into()),
            };
            lists::objects(job, host, &mut surface, &mut s, spec).map(|_| String::new())
        }
        "WRKMBRPDM" => {
            let (file, lib) = qual(r, "FILE");
            let spec = lists::MbrSpec {
                file: file.unwrap_or_else(|| "*PRV".into()),
                lib: lib.unwrap_or_else(|| "*PRV".into()),
                mbr: parm(r, "MBR").unwrap_or_else(|| "*PRV".into()),
                ty: parm(r, "MBRTYPE").unwrap_or_else(|| "*PRV".into()),
            };
            lists::members(job, host, &mut surface, &mut s, spec).map(|_| String::new())
        }
        "FNDSTRPDM" => {
            let search = search_of(host, r)?;
            let option = elem(r, "OPTION", 0).unwrap_or_else(|| "*NONE".into());
            let prompt = elem(r, "OPTION", 1).as_deref() == Some("*YES");
            let print = parm(r, "PRTMBRLIST").as_deref() == Some("*YES");
            Ok(lists::found(
                job, host, &mut s, &search, &option, prompt, print,
            ))
        }
        other => Err(Fail::new(
            "CPF9898",
            format!("{other} is not a PDM command."),
        )),
    };
    s.save(host);
    res
}

/// `FNDSTRPDM`'s parameters as a search.
fn search_of(host: &Host, r: &Resolved) -> Result<find::Search, Fail> {
    let string = r
        .get("STRING")
        .and_then(|b| b.value.as_ref())
        .map(|v| v.source().trim().trim_matches('\'').replace("''", "'"))
        .unwrap_or_default();
    let (file, lib) = qual(r, "FILE");
    let file = file.ok_or_else(|| Fail::new("CPD0071", "A file is needed."))?;
    let lib = lib.unwrap_or_else(|| "*LIBL".into());
    let lib = data::file_library(host, &lib, &file).ok_or_else(|| {
        Fail::new(
            "CPF9812",
            format!("File {file} in library {lib} not found."),
        )
    })?;
    let mut members = values(r, "MBR");
    if members.is_empty() {
        members.push("*ALL".into());
    }
    let from = elem(r, "COL", 0).and_then(|v| v.parse().ok()).unwrap_or(1);
    let to = elem(r, "COL", 1).and_then(|v| v.parse().ok());
    Ok(find::Search {
        lib,
        file,
        members,
        query: Query {
            string,
            from,
            to,
            match_case: parm(r, "CASE").as_deref() == Some("*MATCH"),
        },
    })
}

/// `FNDSTRPDM` without a display: the report as the command's output.
fn fndstrpdm_text(host: &Host, r: &Resolved) -> Result<String, Fail> {
    let s = search_of(host, r)?;
    let (hits, searched) = find::run(host, &s);
    let records = match parm(r, "PRTRCDS").as_deref() {
        None | Some("*NONE") => None,
        Some("*ALL") => Some(None),
        Some(n) => Some(n.parse().ok()),
    };
    Ok(pdm::find::report(
        &s.lib,
        &s.file,
        &s.query,
        &hits,
        searched.len(),
        records,
    ))
}
