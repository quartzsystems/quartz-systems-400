// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Message and object displays: `DSPMSG`/`WRKMSG` with Additional
//! Message Information, `DSPMSGD`, `WRKRPYLE`, `DSPLOG` (`msgs`);
//! `DSPLIBL`, `WRKOBJ`, `DSPLIB`, `DSPOBJD`, `DSPOBJAUT`, `WRKBNDDIR`
//! (`libs`). The helpers here are theirs.

// Throughout: `d` the dialog, `r` the command, `l` the link.
#![allow(clippy::many_single_char_names)]

mod libs;
mod msgs;

use cmd::{Resolved, Value};
use mi::Job;
use stg::Client;

use crate::command::{Fail, Outcome};
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{CMDLINE_VAR, Choice, Response, UimDialog};
use uim::model::Action;

/// The commands these drivers show; without a display each runs its
/// text form in the storage service.
pub const COMMANDS: &[&str] = &[
    "DSPMSG",
    "WRKMSG",
    "DSPMSGD",
    "WRKRPYLE",
    "DSPLOG",
    "DSPLIBL",
    "WRKOBJ",
    "DSPLIB",
    "DSPOBJD",
    "DSPOBJAUT",
    "WRKBNDDIR",
];

/// The driver of a message or object command, if it has one.
pub fn run(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
    _out: &mut Outcome,
) -> Option<Result<String, Fail>> {
    Some(match r.name.as_str() {
        "DSPMSG" | "WRKMSG" => msgs::dspmsg(host, surface, r),
        "DSPMSGD" => msgs::dspmsgd(host, surface, r),
        "WRKRPYLE" => msgs::wrkrpyle(job, host, surface),
        "DSPLOG" => msgs::dsplog(host, surface, r),
        "DSPLIBL" => libs::dsplibl(job, host, surface),
        "WRKOBJ" => libs::wrkobj(job, host, surface, r),
        "DSPLIB" => libs::dsplib(job, host, surface, r),
        "DSPOBJD" => libs::dspobjd(job, host, surface, r),
        "DSPOBJAUT" => libs::dspobjaut(host, surface, r),
        "WRKBNDDIR" => libs::wrkbnddir(job, host, surface, r),
        _ => return None,
    })
}

/// A parameter's value as text, `None` when it has none.
fn parm(r: &Resolved, kwd: &str) -> Option<String> {
    r.get(kwd)
        .and_then(|b| b.value.as_ref())
        .map(|v| match v {
            Value::List(items) => items.first().map(Value::source).unwrap_or_default(),
            other => other.source(),
        })
        .map(|s| s.trim().trim_matches('\'').to_ascii_uppercase())
}

/// A qualified parameter as `(name, library)`, the library `lib` when
/// none was given.
fn qual(r: &Resolved, kwd: &str, lib: &str) -> (String, String) {
    let up = |v: &Value| v.source().trim().to_ascii_uppercase();
    match r.get(kwd).and_then(|b| b.value.as_ref()) {
        Some(Value::Qual(parts)) => (
            parts.first().map(up).unwrap_or_default(),
            parts
                .get(1)
                .map(up)
                .filter(|l| !l.is_empty())
                .unwrap_or_else(|| lib.into()),
        ),
        Some(v) => {
            let s = up(v);
            match s.split_once('/') {
                Some((l, n)) => (n.to_string(), l.to_string()),
                None => (s, lib.into()),
            }
        }
        None => (String::new(), lib.into()),
    }
}

/// Show a panel, a render failure as the command's escape.
fn show(d: &mut UimDialog, surface: &mut Surface, panel: &str) -> Result<Response, Fail> {
    d.show(surface, panel)
        .map_err(|e| Fail::new("CPF6A98", format!("panel {panel}: {e}")))
}

/// The user the job runs as.
fn user(host: &Host) -> String {
    host.link.borrow_mut().with(Client::whoami).user
}

/// Run a command from a panel: the line to show after it, an error
/// when it failed. A leading `?` prompts it.
fn exec(job: &mut Job, host: &Host, text: &str) -> Option<(String, bool)> {
    let out = crate::command::run(job, host, text);
    if let Some(f) = &out.escape {
        let line = f.text.lines().last().unwrap_or("").trim().to_string();
        let line = if line.is_empty() {
            format!("{} ended with {}.", out.name, f.msgid)
        } else {
            line
        };
        return Some((line, true));
    }
    let first = out
        .text
        .lines()
        .find(|l| !l.trim().is_empty())?
        .trim()
        .to_string();
    Some((first, false))
}

/// The options typed on a list, run in order: a `CMD` action with the
/// entry's values, else what `special` makes of it.
fn act(
    job: &mut Job,
    host: &Host,
    d: &mut UimDialog,
    choices: &[Choice],
    special: &dyn Fn(&UimDialog, &Choice) -> Option<String>,
) {
    for c in choices {
        let text = if let Some(Action::Cmd(t)) = &c.action {
            d.substitute(t, Some((&c.list, c.row)))
        } else if let Some(t) = special(d, c) {
            t
        } else {
            d.message = Some((format!("Option {} is not valid here.", c.option), true));
            continue;
        };
        if let Some(m) = exec(job, host, &text) {
            d.message = Some(m);
        }
    }
    for c in choices {
        d.list_mut(&c.list).options.clear();
    }
}

/// A list panel's common answers: a command, F4 with nothing typed,
/// F9. `None` when the answer is the caller's.
fn common(
    job: &mut Job,
    host: &Host,
    d: &mut UimDialog,
    r: Response,
    last: &mut String,
) -> Option<Response> {
    match r {
        Response::Command(text) => {
            let typed = text.trim_start_matches('?').to_string();
            *last = typed;
            if let Some(m) = exec(job, host, &text) {
                d.message = Some(m);
            }
            None
        }
        Response::Retrieve => {
            if last.is_empty() {
                d.message = Some(("There is no command to retrieve.".into(), true));
            } else {
                d.set(CMDLINE_VAR, last);
            }
            None
        }
        Response::Return(4) => {
            d.message = Some(("Type a command to prompt on the command line.".into(), true));
            None
        }
        other => Some(other),
    }
}

/// The entry index under the cursor of a list whose first entry is on
/// row `first` and that shows `rows` entries a page.
fn at_cursor(d: &mut UimDialog, list: &str, first: usize, rows: usize) -> Option<usize> {
    let (row, _) = d.cursor;
    if row < first || row >= first + rows {
        return None;
    }
    let l = d.list_mut(list);
    let i = l.top + row - first;
    (i < l.rows.len()).then_some(i)
}

/// Fill a list with entries, every one given; the page kept when it
/// still has entries.
fn fill(d: &mut UimDialog, list: &str, rows: Vec<Vec<(String, String)>>) {
    let top = d.list_mut(list).top;
    d.clear_list(list);
    let n = rows.len();
    for r in rows {
        d.add_row(list, r);
    }
    let l = d.list_mut(list);
    l.complete = true;
    l.top = if top < n { top } else { 0 };
}

/// One entry from `(variable, value)` pairs.
fn entry(pairs: &[(&str, String)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.clone()))
        .collect()
}

/// Words packed into rows of at most `width` characters.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut rows = Vec::new();
    let mut row = String::new();
    for word in text.split_whitespace() {
        let mut word = word.to_string();
        while word.chars().count() > width {
            if !row.is_empty() {
                rows.push(std::mem::take(&mut row));
            }
            let head: String = word.chars().take(width).collect();
            word = word.chars().skip(width).collect();
            rows.push(head);
        }
        if row.is_empty() {
            row = word;
        } else if row.chars().count() + 1 + word.chars().count() <= width {
            row.push(' ');
            row.push_str(&word);
        } else {
            rows.push(std::mem::replace(&mut row, word));
        }
    }
    if !row.is_empty() {
        rows.push(row);
    }
    rows
}

/// `CYYMMDDHHMMSS` as `MM/DD/YY` and `HH:MM:SS`; blanks when short.
fn stamp(s: &str) -> (String, String) {
    let b = s.as_bytes();
    if b.len() < 13 || !b.iter().all(u8::is_ascii_digit) {
        return (String::new(), String::new());
    }
    (
        format!("{}/{}/{}", &s[3..5], &s[5..7], &s[1..3]),
        format!("{}:{}:{}", &s[7..9], &s[9..11], &s[11..13]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapping_and_stamps() {
        assert_eq!(wrap("a bb ccc dddd", 6), ["a bb", "ccc", "dddd"]);
        assert_eq!(wrap("abcdefgh", 3), ["abc", "def", "gh"]);
        assert_eq!(
            stamp("1261023101112"),
            ("10/23/26".into(), "10:11:12".into())
        );
        assert_eq!(stamp(""), (String::new(), String::new()));
    }
}
