// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/files/edit?path=`: a text file in a Courier text area, read and
//! written in the file's CCSID with its own line endings. Save, Save
//! as..., Reset, Cancel. A file changed on the system since it was
//! opened asks before it is overwritten (`QAD8011`); a binary file, or
//! one over [`READ_ONLY`], opens read-only (`QAD8013`); one over
//! [`MAX`] is refused (`QAD8012`).

use qsweb::frame::Page;
use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::msg::{Kind, Message};

use super::transfer::{BINARY, UTF8, decode, read_all};
use super::{GROUP, back, browse_route, error, join, looks_text, name_of, parent, path_arg};
use crate::env::files::Kind as EntryKind;
use crate::msgs;
use crate::policy;
use crate::routes::Rq;
use crate::transfer::grouped;

/// The largest file the editor opens.
pub const MAX: u64 = 64 * 1024 * 1024;
/// The largest file the editor lets be changed.
pub const READ_ONLY: u64 = 8 * 1024 * 1024;

/// The session value of the file being edited: path, modified, size.
const OPENED: &str = "files.edit";

/// The file as the editor sees it.
struct Opened {
    size: u64,
    modified: u64,
    ccsid: u16,
    crlf: bool,
    text: String,
    writable: bool,
}

fn open(rq: &Rq<'_>, path: &str) -> Result<Opened, Message> {
    let files = &rq.env().files;
    let e = files.entry(rq.user(), path)?;
    if e.kind.is_dir() || matches!(e.kind, EntryKind::Link { broken: true, .. }) {
        return Err(rq.message(Kind::Error, "CPFA0DA", &[path]));
    }
    let size = if matches!(e.kind, EntryKind::Link { .. }) {
        files.read(rq.user(), path, 0, MAX + 1)?.len() as u64
    } else {
        e.size
    };
    if size > MAX {
        return Err(rq.message(Kind::Error, msgs::TOO_LARGE, &[path]));
    }
    let data = read_all(rq, path, size)?;
    let writable = e.ccsid != BINARY && looks_text(&data) && size <= READ_ONLY;
    let raw = decode(&data, e.ccsid).unwrap_or_else(|_| String::from_utf8_lossy(&data).into_owned());
    let crlf = raw.contains("\r\n");
    let text = raw.replace("\r\n", "\n");
    Ok(Opened {
        size,
        modified: e.modified,
        ccsid: e.ccsid,
        crlf,
        text,
        writable,
    })
}

/// The text as the file stores it.
fn encode(text: &str, o: &Opened) -> Result<Vec<u8>, String> {
    let lf = text.replace("\r\n", "\n");
    let body = if o.crlf { lf.replace('\n', "\r\n") } else { lf };
    match o.ccsid {
        0 | UTF8 => Ok(body.into_bytes()),
        c => ccsid::conv::from_str(c, &body).map_err(|e| e.to_string()),
    }
}

/// `GET` and `POST /files/edit`.
pub fn page(rq: &Rq<'_>) -> Response {
    let path = path_arg(rq);
    let name = name_of(&path).to_string();
    let crumb = [GROUP, "Browse files", name.as_str()];
    if let Err(r) = rq.policy(policy::FILES_EDIT, &crumb) {
        return r;
    }
    if let Err(r) = super::may_reach(rq, &path, &crumb) {
        return r;
    }
    let dir = parent(&path);
    let o = match open(rq, &path) {
        Ok(o) => o,
        Err(m) => return back(rq, &dir, error(rq, &m)),
    };
    if !rq.post() {
        rq.set(&[(OPENED, &format!("{path}\n{}\n{}", o.modified, o.size))]);
        let mut messages = Vec::new();
        if !o.writable {
            messages.push(rq.message(Kind::Info, msgs::READ_ONLY, &[&path]));
        }
        return show(rq, &path, &o, &o.text, messages, false);
    }
    let f = rq.form();
    if f.has("cancel") {
        return rq.redirect(&browse_route(&dir));
    }
    if f.has("reset") || !o.writable {
        return rq.redirect(&format!("/files/edit{}", qsweb::html::query(&[("path", &path)])));
    }
    let text = f.str("t").to_string();
    if f.has("saveas") {
        return save_as(rq, &path, &text, Vec::new());
    }
    if f.has("saveas_ok") {
        let to = f.str("to").trim();
        let to = if to.starts_with('/') { super::clean(to) } else { join(&dir, to) };
        if !super::good_name(name_of(&to)) {
            let m = rq.message(Kind::Error, msgs::BAD_VALUE, &[f.str("to"), "Save as"]);
            return save_as(rq, &path, &text, vec![m]);
        }
        if let Err(r) = super::may_reach(rq, &to, &crumb) {
            return r;
        }
        if rq.env().files.entry(rq.user(), &to).is_ok() && !f.has("replace") {
            let m = rq.message(Kind::Info, msgs::FILE_EXISTS, &[name_of(&to), &parent(&to)]);
            return save_as(rq, &path, &text, vec![m]);
        }
        return write(rq, &to, &text, &o);
    }
    // Changed on the system since it was opened: ask first.
    let opened = rq.get(OPENED);
    let mut l = opened.lines();
    let same = l.next() == Some(path.as_str())
        && l.next() == Some(o.modified.to_string().as_str())
        && l.next() == Some(o.size.to_string().as_str());
    if !same && !f.has("overwrite") {
        let m = rq.message(Kind::Info, msgs::CHANGED_SINCE, &[&path]);
        return show(rq, &path, &o, &text, vec![m], true);
    }
    write(rq, &path, &text, &o)
}

fn write(rq: &Rq<'_>, path: &str, text: &str, o: &Opened) -> Response {
    let data = match encode(text, o) {
        Ok(d) => d,
        Err(why) => {
            let m = rq.message(Kind::Error, msgs::FILE_TRANSFER_FAILED, &[path, &why]);
            return show(rq, path, o, text, vec![m], false);
        }
    };
    let ccsid = if o.ccsid == 0 { UTF8 } else { o.ccsid };
    match rq.env().files.write(rq.user(), path, &data, ccsid) {
        Ok(()) => {
            let m = rq.message(Kind::Done, msgs::SAVED, &[path]);
            rq.done(&format!("/files/edit{}", qsweb::html::query(&[("path", path)])), m)
        }
        Err(m) => {
            let m = error(rq, &m);
            show(rq, path, o, text, vec![m], false)
        }
    }
}

/// Save as: the new path, the text carried in the form.
fn save_as(rq: &Rq<'_>, path: &str, text: &str, messages: Vec<Message>) -> Response {
    let name = name_of(path);
    let replace = messages.iter().any(|m| m.id == msgs::FILE_EXISTS);
    let mut inner = format!(
        "<table class=\"props\" cellspacing=\"0\">\n<tr><th>Save as <span class=\"req\">*</span></th><td>{}</td></tr>\n</table>\n<input type=\"hidden\" name=\"t\" value=\"{}\">",
        qsweb::partials::input_cmd("to", &rq.form().get("to").map_or_else(|| path.to_string(), ToString::to_string), 50),
        esc(text)
    );
    if replace {
        inner.push_str("<input type=\"hidden\" name=\"replace\" value=\"1\">");
    }
    inner.push_str(&qsweb::partials::buttons(&[
        qsweb::partials::Button::new("saveas_ok", if replace { "Replace" } else { "OK" }),
        qsweb::partials::Button::new("cancel", "Cancel"),
    ]));
    let body = qsweb::partials::form(&rq.ctx, "", &inner);
    let title = format!("Save as: {name}");
    rq.page(Page {
        title: title.clone(),
        crumb: vec![GROUP.into(), "Browse files".into(), name.into()],
        h1: title,
        messages,
        body,
        help: "file-edit".into(),
        nav: Some((GROUP, Some(super::BROWSE))),
        ..Page::default()
    })
}

fn show(rq: &Rq<'_>, path: &str, o: &Opened, text: &str, messages: Vec<Message>, ask: bool) -> Response {
    let dir = parent(path);
    let name = name_of(path);
    // A text area drops one newline at its start.
    let shown = if text.starts_with('\n') { format!("\n{text}") } else { text.to_string() };
    let head = format!(
        "<p class=\"small\">Path: <span class=\"mono\">{}</span> &nbsp; {} bytes &nbsp; CCSID {} &nbsp; Line ends: {} &nbsp; {}</p>",
        esc(path),
        grouped(o.size),
        o.ccsid,
        if o.crlf { "CR LF" } else { "LF" },
        qsweb::partials::link(&rq.ctx.url(&browse_route(&dir)), &format!("Back to {dir}")),
    );
    let ro = if o.writable { "" } else { " readonly" };
    let buttons = if !o.writable {
        "<div class=\"buttons\"><input type=\"submit\" name=\"cancel\" value=\"Cancel\"></div>".to_string()
    } else if ask {
        "<div class=\"buttons\"><input type=\"submit\" name=\"overwrite\" value=\"Overwrite\"><input type=\"submit\" name=\"reset\" value=\"Reload\"><input type=\"submit\" name=\"cancel\" value=\"Cancel\"></div>".to_string()
    } else {
        "<div class=\"buttons\"><input type=\"submit\" name=\"save\" value=\"Save\"><input type=\"submit\" name=\"saveas\" value=\"Save as...\"><input type=\"submit\" name=\"reset\" value=\"Reset\"><input type=\"submit\" name=\"cancel\" value=\"Cancel\"></div>".to_string()
    };
    let body = format!(
        "{head}{}",
        qsweb::partials::form(
            &rq.ctx,
            "",
            &format!("<textarea name=\"t\" class=\"conf\" rows=\"30\" wrap=\"off\"{ro}>{}</textarea>{buttons}", esc(&shown))
        )
    );
    let title = format!("Edit file: {name}");
    rq.page(Page {
        title: title.clone(),
        crumb: vec![GROUP.into(), "Browse files".into(), name.into()],
        h1: title,
        messages,
        body,
        help: "file-edit".into(),
        nav: Some((GROUP, Some(super::BROWSE))),
        ..Page::default()
    })
}
