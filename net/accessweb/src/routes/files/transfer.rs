// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Transfers: downloads (`/files/download`, `/files/open`) read a piece
//! at a time as the user and answer `Range` requests, so an interrupted
//! download resumes; the browser's multiple-file upload; the queue's
//! Cancel, Retry, Replace, Skip and Download (`/files/queue`). Text is
//! converted between the file's CCSID and UTF-8; binary goes as it is.

use std::io::{self, Read};
use std::sync::Arc;

use qsweb::http::range::{self, Range};
use qsweb::http::{Response, date};
use qsweb::msg::{Kind, Message};

use super::browse::{DOWNLOAD_ENDINGS, DOWNLOAD_MODE, download_crlf, download_text};
use super::{
    GROUP, back, browse_route, error, join, looks_text, name_of, parent, path_arg, token_ok,
};
use crate::env::Env;
use crate::env::files::Kind as EntryKind;
use crate::msgs;
use crate::policy;
use crate::routes::Rq;
use crate::transfer::{Direction, Endings, Mode, State, Transfer};

/// Bytes read from the system at a time.
pub const PIECE: u64 = 256 * 1024;

/// The CCSID of binary data.
pub const BINARY: u16 = 65535;

/// UTF-8.
pub const UTF8: u16 = 1208;

/// A name safe in a `Content-Disposition` header.
fn header_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_graphic() && c != '"' && c != '\\' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// The content type a file opens with in the browser; anything else is
/// downloaded.
fn inline_type(name: &str, text: bool) -> Option<&'static str> {
    let ext = name
        .rsplit_once('.')
        .map(|x| x.1.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "pdf" => Some("application/pdf"),
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        _ if text => Some("text/plain; charset=utf-8"),
        _ => None,
    }
}

/// A file's text as UTF-8 with `crlf` or LF line endings.
pub fn to_utf8(data: &[u8], ccsid: u16, crlf: bool) -> Result<Vec<u8>, String> {
    let lf = decode(data, ccsid)?.replace("\r\n", "\n");
    Ok(if crlf { lf.replace('\n', "\r\n") } else { lf }.into_bytes())
}

/// A file's text as it is, line endings and all.
pub fn decode(data: &[u8], ccsid: u16) -> Result<String, String> {
    if ccsid == UTF8 || ccsid == 0 || ccsid == BINARY {
        Ok(String::from_utf8_lossy(data).into_owned())
    } else {
        ccsid::conv::to_string(ccsid, data).map_err(|e| e.to_string())
    }
}

/// UTF-8 text as `ccsid`, LF line endings.
pub fn from_utf8(data: &[u8], ccsid: u16) -> Result<Vec<u8>, String> {
    let s = String::from_utf8_lossy(data).replace("\r\n", "\n");
    if ccsid == UTF8 {
        return Ok(s.into_bytes());
    }
    ccsid::conv::from_str(ccsid, &s).map_err(|e| e.to_string())
}

/// Reads a file from the system a piece at a time as the user, and
/// keeps its queue entry up to date: done as it goes, Complete at the
/// end, stopped when the connection goes before it.
struct Pull {
    env: Env,
    user: String,
    path: String,
    at: u64,
    end: u64,
    id: u32,
    buf: Vec<u8>,
    ended: bool,
}

impl Pull {
    fn state(&self, s: State) {
        self.env
            .files
            .queue()
            .update(&self.user, self.id, |t| t.state = s);
    }
}

impl Read for Pull {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.buf.is_empty() {
            if self.at >= self.end {
                if !self.ended {
                    self.ended = true;
                    self.state(State::Complete);
                }
                return Ok(0);
            }
            if self
                .env
                .files
                .queue()
                .get(&self.user, self.id)
                .is_some_and(|t| t.state == State::Cancelled)
            {
                self.ended = true;
                return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
            }
            let n = PIECE.min(self.end - self.at);
            match self.env.files.read(&self.user, &self.path, self.at, n) {
                Ok(b) if !b.is_empty() => self.buf = b,
                Ok(_) => {
                    self.ended = true;
                    self.state(State::Failed(
                        msgs::FILE_TRANSFER_FAILED.into(),
                        "the file got shorter".into(),
                    ));
                    return Ok(0);
                }
                Err(m) => {
                    self.ended = true;
                    self.state(State::Failed(
                        msgs::FILE_TRANSFER_FAILED.into(),
                        format!("{} {}", m.id, m.text),
                    ));
                    return Err(io::Error::other(m.text));
                }
            }
        }
        let n = out.len().min(self.buf.len());
        out[..n].copy_from_slice(&self.buf[..n]);
        self.buf.drain(..n);
        self.at += n as u64;
        let at = self.at;
        self.env
            .files
            .queue()
            .update(&self.user, self.id, |t| t.done = at);
        Ok(n)
    }
}

impl Drop for Pull {
    fn drop(&mut self) {
        if !self.ended {
            self.state(State::Failed(
                msgs::FILE_TRANSFER_FAILED.into(),
                "the connection ended".into(),
            ));
        }
    }
}

/// The queue entry of this download: the one `?q=` names, or one
/// already there for the path that did not finish, or a new one.
fn entry_for(rq: &Rq<'_>, path: &str, mode: Mode, size: u64) -> u32 {
    // A HEAD moves nothing: no entry (0 names none).
    if rq.req.method == "HEAD" {
        return 0;
    }
    let q = rq.env().files.queue();
    let user = rq.user();
    let named: Option<u32> = rq.query().str("q").parse().ok();
    let found = q.list(user).into_iter().rev().find(|t| {
        t.direction == Direction::Download
            && t.path == path
            && (named == Some(t.id) || (named.is_none() && !matches!(t.state, State::Complete)))
    });
    if let Some(t) = found {
        q.update(user, t.id, |t| {
            t.state = State::Running(None);
            t.mode = mode;
            t.size = size;
        });
        return t.id;
    }
    let mut t = Transfer::new(path, Direction::Download, mode, size);
    t.state = State::Running(None);
    q.add(user, t)
}

/// `/files/download?path=` (and `/files/open`, `inline`): the file, all
/// of it or the range asked for.
#[allow(clippy::too_many_lines)]
pub fn download(rq: &Rq<'_>, inline: bool) -> Response {
    let path = path_arg(rq);
    let crumb = [GROUP, "Browse files"];
    if let Err(r) = rq.policy(policy::FILES_DOWNLOAD, &crumb) {
        return r;
    }
    if let Err(r) = super::may_reach(rq, &path, &crumb) {
        return r;
    }
    let files = &rq.env().files;
    let props = match files.props(rq.user(), &path) {
        Ok(p) => p,
        Err(m) => return back(rq, &parent(&path), error(rq, &m)),
    };
    let e = &props.entry;
    if e.kind.is_dir() {
        return rq.redirect(&browse_route(&path));
    }
    let size = match &e.kind {
        // A link is read through: its target's size.
        EntryKind::Link { .. } => match files.read(rq.user(), &path, 0, u64::MAX) {
            Ok(b) => b.len() as u64,
            Err(m) => return back(rq, &parent(&path), error(rq, &m)),
        },
        _ => e.size,
    };
    let head = match files.read(rq.user(), &path, 0, 8192) {
        Ok(b) => b,
        Err(m) => return back(rq, &parent(&path), error(rq, &m)),
    };
    let text_file = e.ccsid != BINARY && looks_text(&head);
    let name = name_of(&path).to_string();
    let ctype = if inline {
        inline_type(&name, text_file)
    } else {
        None
    };
    let disposition = format!(
        "{}; filename=\"{}\"",
        if ctype.is_some() {
            "inline"
        } else {
            "attachment"
        },
        header_name(&name)
    );
    let convert = text_file && (inline || download_text(rq));
    let etag = format!("\"{size}-{}-{}\"", e.modified, u8::from(convert));
    let modified = date::imf_fixdate(i64::try_from(e.modified).unwrap_or(0));
    let ask = range::parse(
        rq.req.header("range"),
        rq.req.header("if-range"),
        &[&etag, &modified],
        size,
    );
    let mut r = if convert {
        // Text is converted whole; a range is of the converted bytes.
        let crlf = !inline && download_crlf(rq);
        let data = match files
            .read(rq.user(), &path, 0, size)
            .map(|d| to_utf8(&d, e.ccsid, crlf))
        {
            Ok(Ok(d)) => d,
            Ok(Err(why)) => {
                return back(
                    rq,
                    &parent(&path),
                    rq.message(Kind::Error, msgs::FILE_TRANSFER_FAILED, &[&path, &why]),
                );
            }
            Err(m) => return back(rq, &parent(&path), error(rq, &m)),
        };
        let len = data.len() as u64;
        let ask = range::parse(
            rq.req.header("range"),
            rq.req.header("if-range"),
            &[&etag, &modified],
            len,
        );
        let mode = Mode::Text(e.ccsid, if crlf { Endings::CrLf } else { Endings::Lf });
        let id = entry_for(rq, &path, mode, len);
        let (off, n) = ask.span(len);
        let (s, end) = (
            usize::try_from(off).unwrap_or(0),
            usize::try_from(off + n).unwrap_or(0),
        );
        rq.env().files.queue().update(rq.user(), id, |t| {
            t.done = len;
            t.state = State::Complete;
        });
        let mut r = Response::new(
            ask.status(),
            ctype.unwrap_or("text/plain; charset=utf-8"),
            data.get(s..end).unwrap_or(&[]).to_vec(),
        );
        if let Some(cr) = ask.content_range(len) {
            r = r.with("Content-Range", &cr);
        }
        r
    } else {
        let id = entry_for(rq, &path, Mode::Binary, size);
        if ask == Range::Unsatisfiable {
            rq.env()
                .files
                .queue()
                .update(rq.user(), id, |t| t.state = State::Complete);
            return Response::new(416, "text/plain; charset=utf-8", Vec::new()).with(
                "Content-Range",
                &ask.content_range(size).unwrap_or_default(),
            );
        }
        let (off, n) = ask.span(size);
        let pull = Pull {
            env: rq.env().clone(),
            user: rq.user().to_string(),
            path: path.clone(),
            at: off,
            end: off + n,
            id,
            buf: Vec::new(),
            ended: false,
        };
        rq.env()
            .files
            .queue()
            .update(rq.user(), id, |t| t.done = off);
        let mut r = Response::new(
            ask.status(),
            ctype.unwrap_or("application/octet-stream"),
            Vec::new(),
        )
        .body_sized(n, Box::new(pull));
        if let Some(cr) = ask.content_range(size) {
            r = r.with("Content-Range", &cr);
        }
        r
    };
    if ask == Range::Unsatisfiable && !convert {
        return r;
    }
    r = r
        .with("Content-Disposition", &disposition)
        .with("Accept-Ranges", "bytes")
        .with("ETag", &etag)
        .with("Last-Modified", &modified)
        .with("X-Content-Type-Options", "nosniff")
        .with("Cache-Control", "private, no-cache");
    if inline {
        // What a user stored never runs as the product's page.
        r = r.with("Content-Security-Policy", "sandbox");
    }
    r
}

/// Download of the selected entries: one goes at once; several wait in
/// the queue, each fetched from its row.
pub fn download_selected(rq: &Rq<'_>, dir: &str, names: &[String]) -> Response {
    let crumb = [GROUP, "Browse files"];
    if let Err(r) = rq.policy(policy::FILES_DOWNLOAD, &crumb) {
        return r;
    }
    let files = &rq.env().files;
    let mut plain: Vec<String> = Vec::new();
    for n in names {
        let p = join(dir, n);
        match files.entry(rq.user(), &p) {
            Ok(e) if !e.kind.is_dir() => plain.push(p),
            Ok(_) => {}
            Err(m) => return back(rq, dir, error(rq, &m)),
        }
    }
    match plain.as_slice() {
        [] => back(rq, dir, rq.message(Kind::Info, msgs::NONE_SELECTED, &[])),
        [one] => rq.redirect(&format!(
            "/files/download{}",
            qsweb::html::query(&[("path", one)])
        )),
        many => {
            for p in many {
                let size = files.entry(rq.user(), p).map_or(0, |e| e.size);
                files.queue().add(
                    rq.user(),
                    Transfer::new(p, Direction::Download, Mode::Binary, size),
                );
            }
            rq.redirect(&browse_route(dir))
        }
    }
}

/// Write one upload; the queue entry says how it went.
fn write_one(rq: &Rq<'_>, id: u32, path: &str, data: &[u8], ccsid: u16) -> Result<(), Message> {
    let q = rq.env().files.queue();
    match rq.env().files.write(rq.user(), path, data, ccsid) {
        Ok(()) => {
            q.update(rq.user(), id, |t| {
                t.done = t.size;
                t.state = State::Complete;
                t.data = None;
            });
            Ok(())
        }
        Err(m) => {
            let why = format!("{} {}", m.id, m.text);
            q.update(rq.user(), id, |t| {
                t.state = State::Failed(msgs::FILE_TRANSFER_FAILED.into(), why.clone())
            });
            Err(m)
        }
    }
}

/// The multipart post of the upload form: every file chosen, as binary
/// or as text in the CCSID given; an existing file as the form says.
pub fn upload(rq: &Rq<'_>, dir: &str) -> Response {
    let crumb = [GROUP, "Browse files"];
    if let Err(r) = rq.policy(policy::FILES_UPLOAD, &crumb) {
        return r;
    }
    let f = rq.form();
    // The download options go with the form.
    let dm = if f.str("dm") == "text" { "text" } else { "bin" };
    let dle = if f.str("dle") == "LF" { "LF" } else { "CR LF" };
    rq.set(&[(DOWNLOAD_MODE, dm), (DOWNLOAD_ENDINGS, dle)]);
    let parts: Vec<_> = rq
        .req
        .multipart()
        .unwrap_or_default()
        .into_iter()
        .filter(|p| p.filename.as_deref().is_some_and(|n| !n.is_empty()))
        .collect();
    if parts.is_empty() {
        return rq.redirect(&browse_route(dir));
    }
    let text = f.str("mode") == "text";
    let ccsid: u16 = if text {
        match f.str("ccsid").trim().parse() {
            Ok(c) if c != BINARY && ccsid::conv::supported().contains(&c) => c,
            _ => {
                return back(
                    rq,
                    dir,
                    rq.message(Kind::Error, msgs::BAD_VALUE, &[f.str("ccsid"), "CCSID"]),
                );
            }
        }
    } else {
        0
    };
    let ex = f.str("ex");
    let files = &rq.env().files;
    let q = files.queue();
    let mut written = Vec::new();
    let mut first_problem: Option<Message> = None;
    for p in parts {
        // Some browsers send the client's whole path.
        let raw = p.filename.unwrap_or_default();
        let name = raw.rsplit(['/', '\\']).next().unwrap_or("").to_string();
        if !super::good_name(&name) {
            first_problem.get_or_insert_with(|| {
                rq.message(Kind::Error, msgs::BAD_VALUE, &[&raw, "File name"])
            });
            continue;
        }
        let path = join(dir, &name);
        let (data, tag) = if text {
            let Ok(d) = from_utf8(&p.data, ccsid) else {
                first_problem.get_or_insert_with(|| {
                    rq.message(Kind::Error, msgs::BAD_VALUE, &[f.str("ccsid"), "CCSID"])
                });
                continue;
            };
            (d, ccsid)
        } else {
            // Binary: tagged UTF-8 when it reads as such, else binary.
            let tag = if looks_text(&p.data) && std::str::from_utf8(&p.data).is_ok() {
                UTF8
            } else {
                BINARY
            };
            (p.data, tag)
        };
        let mode = if text {
            Mode::Text(ccsid, Endings::Lf)
        } else {
            Mode::Binary
        };
        let mut t = Transfer::new(&path, Direction::Upload, mode, data.len() as u64);
        let exists = files.entry(rq.user(), &path).is_ok();
        if exists && (ex == "Skip" || ex == "Ask" || ex.is_empty()) {
            let skip = ex == "Skip";
            t.state = if skip { State::Skipped } else { State::Exists };
            if !skip {
                t.data = Some(Arc::new(data));
            }
            q.add(rq.user(), t);
            first_problem
                .get_or_insert_with(|| rq.message(Kind::Info, msgs::FILE_EXISTS, &[&name, dir]));
            continue;
        }
        // An upload carries no time of its own: it is newer than what
        // the system holds, so Replace if newer replaces.
        t.data = Some(Arc::new(data.clone()));
        let id = q.add(rq.user(), t);
        match write_one(rq, id, &path, &data, tag) {
            Ok(()) => written.push(name),
            Err(m) => {
                let why = format!("{} {}", m.id, m.text);
                first_problem.get_or_insert_with(|| {
                    rq.message(Kind::Error, msgs::FILE_TRANSFER_FAILED, &[&path, &why])
                });
            }
        }
    }
    let m = match first_problem {
        Some(m) => m,
        None => rq.message(Kind::Done, msgs::UPLOADED, &[&written.join(", ")]),
    };
    back(rq, dir, m)
}

/// `/files/queue?n=&op=&t=`: an action on a queue entry.
pub fn queue_op(rq: &Rq<'_>) -> Response {
    if !token_ok(rq) {
        return super::stale(rq);
    }
    let qy = rq.query();
    let n: u32 = qy.str("n").parse().unwrap_or(0);
    let files = &rq.env().files;
    let q = files.queue();
    let Some(t) = q.get(rq.user(), n) else {
        return rq.redirect(super::BROWSE);
    };
    let dir = parent(&t.path);
    match (qy.str("op"), t.direction, &t.state) {
        ("cancel", _, State::Running(_) | State::Ready) => {
            q.update(rq.user(), n, |t| t.state = State::Cancelled);
            back(
                rq,
                &dir,
                rq.message(Kind::Info, msgs::FILE_TRANSFER_CANCELLED, &[&t.path]),
            )
        }
        ("fetch" | "retry", Direction::Download, _) => rq.redirect(&format!(
            "/files/download{}",
            qsweb::html::query(&[("path", &t.path), ("q", &n.to_string())])
        )),
        (
            "retry" | "replace",
            Direction::Upload,
            State::Failed(..) | State::Exists | State::Cancelled,
        ) => {
            let Some(data) = t.data.clone() else {
                return back(
                    rq,
                    &dir,
                    rq.message(
                        Kind::Error,
                        msgs::FILE_TRANSFER_FAILED,
                        &[&t.path, "its data is no longer kept; upload it again"],
                    ),
                );
            };
            if let Err(r) = rq.policy(policy::FILES_UPLOAD, &[GROUP, "Browse files"]) {
                return r;
            }
            let tag = match t.mode {
                Mode::Text(c, _) => c,
                Mode::Binary => {
                    if looks_text(&data) && std::str::from_utf8(&data).is_ok() {
                        UTF8
                    } else {
                        BINARY
                    }
                }
            };
            match write_one(rq, n, &t.path, &data, tag) {
                Ok(()) => back(
                    rq,
                    &dir,
                    rq.message(Kind::Done, msgs::UPLOADED, &[name_of(&t.path)]),
                ),
                Err(m) => {
                    let why = format!("{} {}", m.id, m.text);
                    back(
                        rq,
                        &dir,
                        rq.message(Kind::Error, msgs::FILE_TRANSFER_FAILED, &[&t.path, &why]),
                    )
                }
            }
        }
        ("skip", Direction::Upload, State::Exists) => {
            q.update(rq.user(), n, |t| {
                t.state = State::Skipped;
                t.data = None;
            });
            rq.redirect(&browse_route(&dir))
        }
        _ => rq.redirect(&browse_route(&dir)),
    }
}

/// Bytes of a file for the editor and the like: all of it as the user.
pub fn read_all(rq: &Rq<'_>, path: &str, size: u64) -> Result<Vec<u8>, Message> {
    rq.env().files.read(rq.user(), path, 0, size)
}
