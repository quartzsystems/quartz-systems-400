// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Browse files: the folder tree, the path and its name pattern, the
//! entries 500 a page (links with their targets, broken ones in red),
//! the actions on one or the selected entries, the transfer queue and
//! the upload form. The forms post back here.

use std::fmt::Write as _;

use qsweb::frame::Page;
use qsweb::html::{esc, query};
use qsweb::http::Response;
use qsweb::partials::{Cell, Col, List, Paging, ROW_CHOICES, list_table, paging};

use super::{BROWSE, CLIP, GROUP, browse_route, join, matches, name_of, parent, path_arg, rwx, tokened};
use crate::PRODUCT;
use crate::env::files::{Entry, Kind};
use crate::policy;
use crate::prefs;
use crate::routes::Rq;
use crate::transfer::{Direction, State, Transfer, grouped, size};

/// Entries a page, as the desktop shows them.
pub const PAGE: usize = 500;

/// The session values of the download options.
pub const DOWNLOAD_MODE: &str = "files.dm";
/// Their line endings.
pub const DOWNLOAD_ENDINGS: &str = "files.dle";

/// The upload choices for an existing file.
pub const EXISTING: &[&str] = &["Ask", "Skip", "Replace", "Replace if newer"];

/// A select of `options`, `chosen` marked unless it is the first (the
/// browser's choice anyway).
pub fn select(name: &str, options: &[&str], chosen: &str) -> String {
    let mut s = format!("<select name=\"{}\">", esc(name));
    for (i, o) in options.iter().enumerate() {
        let sel = if i > 0 && *o == chosen { " selected" } else { "" };
        let _ = write!(s, "<option{sel}>{}</option>", esc(o));
    }
    s.push_str("</select>");
    s
}

/// `HH:MM` of a `HH:MM:SS` time, the AM/PM kept.
pub fn short_time(t: &str) -> String {
    match t.split_once(' ') {
        Some((hms, ampm)) => format!("{} {ampm}", &hms[..hms.len().min(5)]),
        None => t[..t.len().min(5)].to_string(),
    }
}

/// A path as the queue shows it: under `dir` relative to it.
pub fn shown_path(path: &str, dir: &str) -> String {
    let pre = format!("{}/", dir.trim_end_matches('/'));
    path.strip_prefix(&pre).unwrap_or(path).to_string()
}

/// `GET` and `POST /files`.
pub fn page(rq: &Rq<'_>) -> Response {
    let dir = path_arg(rq);
    if let Err(r) = super::may_reach(rq, &dir, &[GROUP, "Browse files"]) {
        return r;
    }
    if rq.post() {
        return post(rq, &dir);
    }
    let files = &rq.env().files;
    let entries = match files.list(rq.user(), &dir) {
        Ok(e) => e,
        Err(m) => {
            // Not a directory that can be listed: its parent, with why.
            let up = parent(&dir);
            if up == dir || dir == files.home(rq.user()) {
                return show(rq, &dir, Vec::new(), vec![super::error(rq, &m)]);
            }
            return super::back(rq, &up, super::error(rq, &m));
        }
    };
    show(rq, &dir, entries, Vec::new())
}

/// The page over `entries` of `dir`.
#[allow(clippy::too_many_lines)]
fn show(rq: &Rq<'_>, dir: &str, mut entries: Vec<Entry>, messages: Vec<qsweb::msg::Message>) -> Response {
    let q = rq.query();
    let home = rq.env().files.home(rq.user());
    let pat = q.get("pat").map(str::trim).filter(|p| !p.is_empty()).unwrap_or("*").to_string();
    let sort = q.str("sort").to_string();
    entries.retain(|e| matches(&pat, &e.name));
    sort_entries(&mut entries, &sort);
    // The query the page's links keep.
    let mut keep: Vec<(String, String)> = Vec::new();
    if dir != home {
        keep.push(("path".into(), dir.to_string()));
    }
    if pat != "*" {
        keep.push(("pat".into(), pat.clone()));
    }
    let up = dir != "/";
    let total = entries.len() + usize::from(up);
    let rows = q.str("rows").parse().ok().filter(|r| ROW_CHOICES.contains(r)).unwrap_or(PAGE);
    let mut pg_keep = keep.clone();
    if !sort.is_empty() {
        pg_keep.push(("sort".into(), sort.clone()));
    }
    let mut pg = Paging {
        page: 1,
        rows,
        total,
        keep: pg_keep,
    };
    pg.page = q.str("page").parse().unwrap_or(1).clamp(1, pg.pages());
    let range = pg.range();

    let mut body = String::from(
        "<table width=\"100%\" cellspacing=\"0\"><tr><td width=\"200\" valign=\"top\" style=\"background:#f0f0f0;border-right:1px solid #c0c0c0;padding:4px\" class=\"small\">",
    );
    body.push_str(&tree(rq, dir));
    body.push_str("</td><td valign=\"top\" style=\"padding-left:8px\">");
    body.push_str(&path_form(rq, dir, &pat));

    let can = Can {
        download: rq.allowed(policy::FILES_DOWNLOAD),
        edit: rq.allowed(policy::FILES_EDIT),
        delete: rq.allowed(policy::FILES_DELETE),
    };
    let mut list_rows: Vec<Vec<Cell>> = Vec::new();
    let mut i = 0;
    if up {
        if range.contains(&0) {
            let p = parent(dir);
            let acts = vec![
                (rq.ctx.url(&browse_route(&p)), "Open"),
                (rq.ctx.url(&format!("/files/props{}", query(&[("path", &p)]))), "Properties"),
            ];
            list_rows.push(vec![
                Cell::html(String::new()),
                Cell::act(qsweb::partials::actions(&acts)),
                Cell::mono_html(qsweb::partials::link(&rq.ctx.url(&browse_route(&p)), "..")),
                Cell::text("Directory"),
                Cell::num(""),
                Cell::text(""),
                Cell::mono(""),
                Cell::mono(""),
                Cell::num(""),
            ]);
        }
        i = 1;
    }
    for e in &entries {
        if range.contains(&i) {
            list_rows.push(row(rq, dir, e, &can));
        }
        i += 1;
    }
    let list = List {
        cols: vec![
            Col::plain("Select"),
            Col::plain("Actions"),
            Col::sorted("Name", "n"),
            Col::sorted("Type", "t"),
            Col::sorted("Size", "s"),
            Col::sorted("Modified", "m"),
            Col::sorted("Owner", "o"),
            Col::plain("Authority"),
            Col::plain("CCSID"),
        ],
        rows: list_rows,
        empty: "No entries match the pattern.".into(),
        keep: keep.clone(),
    };
    let mut buttons = String::from("<div class=\"buttons\">Selected: ");
    if can.download {
        buttons.push_str("<input type=\"submit\" name=\"download\" value=\"Download\">");
    }
    buttons.push_str("<input type=\"submit\" name=\"cut\" value=\"Cut\"><input type=\"submit\" name=\"copy\" value=\"Copy\">");
    if can.delete {
        buttons.push_str("<input type=\"submit\" name=\"delete\" value=\"Delete...\">");
    }
    buttons.push_str(" &nbsp; <input type=\"submit\" name=\"mkdir\" value=\"New folder...\"></div>");
    body.push_str(&qsweb::partials::form(&rq.ctx, "", &format!("{}{buttons}", list_table(&list))));
    body.push_str(&paging(&pg));
    body.push_str("<h3>Transfer queue</h3>");
    body.push_str(&queue(rq, dir));
    if rq.allowed(policy::FILES_UPLOAD) {
        body.push_str(&upload_form(rq, dir));
    }
    body.push_str("</td></tr></table>");
    rq.page(Page {
        title: "Browse files".into(),
        crumb: vec![GROUP.into(), "Browse files".into()],
        h1: "Browse files".into(),
        desc: Some("The integrated file system, as QS/400 Access for Desktop's Integrated File System window shows it: a folder tree, 500 entries a page, links with their targets.".into()),
        messages,
        body,
        help: "files".into(),
        nav: Some((GROUP, Some(BROWSE))),
        ..Page::default()
    })
}

/// Directories first, then by name; `?sort=` by a column instead.
fn sort_entries(v: &mut [Entry], sort: &str) {
    let key = |e: &Entry| e.name.to_lowercase();
    match sort {
        "n" => v.sort_by_key(key),
        "t" => v.sort_by(|a, b| type_of(a).cmp(&type_of(b)).then_with(|| key(a).cmp(&key(b)))),
        "s" => v.sort_by(|a, b| a.size.cmp(&b.size).then_with(|| key(a).cmp(&key(b)))),
        "m" => v.sort_by(|a, b| a.modified.cmp(&b.modified).then_with(|| key(a).cmp(&key(b)))),
        "o" => v.sort_by(|a, b| a.owner.cmp(&b.owner).then_with(|| key(a).cmp(&key(b)))),
        _ => v.sort_by(|a, b| (a.kind != Kind::Dir).cmp(&(b.kind != Kind::Dir)).then_with(|| key(a).cmp(&key(b)))),
    }
}

/// The Type column's text.
fn type_of(e: &Entry) -> String {
    match &e.kind {
        Kind::Dir => "Directory".into(),
        Kind::File => "File".into(),
        Kind::Link { target, broken, .. } => {
            format!("Link \u{2192} {target}{}", if *broken { " (broken)" } else { "" })
        }
        Kind::Other(t) => t.clone(),
    }
}

/// One entry's row.
/// The actions the policies leave the user.
struct Can {
    download: bool,
    edit: bool,
    delete: bool,
}

#[allow(clippy::many_single_char_names)]
fn row(rq: &Rq<'_>, dir: &str, e: &Entry, can: &Can) -> Vec<Cell> {
    let p = join(dir, &e.name);
    let pq = query(&[("path", &p)]);
    let u = |route: &str| rq.ctx.url(&format!("{route}{pq}"));
    let open_dir = e.kind.is_dir();
    let mut acts: Vec<(String, &str)> = Vec::new();
    if !open_dir && can.download {
        acts.push((u("/files/download"), "Download"));
    }
    if open_dir {
        acts.push((rq.ctx.url(&browse_route(&p)), "Open"));
    } else if can.download {
        acts.push((u("/files/open"), "Open"));
    }
    if !open_dir && can.edit {
        acts.push((u("/files/edit"), "Edit"));
    }
    acts.push((u("/files/rename"), "Rename"));
    acts.push((rq.ctx.url(&tokened(rq, "/files/cut", &[("path", &p)])), "Cut"));
    acts.push((rq.ctx.url(&tokened(rq, "/files/copy", &[("path", &p)])), "Copy"));
    if can.delete {
        acts.push((rq.ctx.url(&format!("/files/confirm{}", query(&[("path", dir), ("s", &e.name)]))), "Delete"));
    }
    acts.push((u("/files/props"), "Properties"));
    let name_href = if open_dir {
        Some(rq.ctx.url(&browse_route(&p)))
    } else if can.edit {
        Some(u("/files/edit"))
    } else if can.download {
        Some(u("/files/open"))
    } else {
        None
    };
    let name = match name_href {
        Some(h) => qsweb::partials::link(&h, &e.name),
        None => esc(&e.name),
    };
    let typ = match &e.kind {
        Kind::Link { broken: true, .. } => format!("<span class=\"err\">{}</span>", esc(&type_of(e))),
        _ => esc(&type_of(e)),
    };
    let is_file = e.kind == Kind::File;
    let (d, t) = rq.stamp(e.modified);
    vec![
        Cell::html(format!("<input type=\"checkbox\" name=\"s\" value=\"{}\">", esc(&e.name))),
        Cell::act(qsweb::partials::actions(&acts)),
        Cell::mono_html(name),
        Cell::html(typ),
        Cell::num(&if is_file { grouped(e.size) } else { String::new() }),
        Cell::text(&format!("{d} {}", short_time(&t))),
        Cell::mono(&e.owner),
        Cell::mono(&rwx(e.mode)),
        Cell::num(&if is_file && e.ccsid != 0 { e.ccsid.to_string() } else { String::new() }),
    ]
}

/// The folder tree: the path from `/` open down to `dir`, each level's
/// other directories closed, `dir`'s own directories under it.
fn tree(rq: &Rq<'_>, dir: &str) -> String {
    let parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    let t = Tree { rq };
    let mut out = String::new();
    Tree::line(&mut out, 0, "[-]", &t.label("/", "/", parts.is_empty()));
    if parts.is_empty() {
        for c in t.subdirs("/") {
            Tree::line(&mut out, 1, "[+]", &t.label(&join("/", &c), &c, false));
        }
    } else {
        t.level(&mut out, "/", 1, &parts);
    }
    out
}

/// The tree's lines.
struct Tree<'a, 'b> {
    rq: &'a Rq<'b>,
}

impl Tree<'_, '_> {
    fn line(out: &mut String, depth: usize, glyph: &str, label: &str) {
        let open = match depth {
            0 => "<div>".to_string(),
            1 => "<div class=\"navitem\">".into(),
            2 => "<div class=\"navitem2\">".into(),
            n => format!("<div class=\"navitem2\" style=\"margin-left:{}px\">", 14 * n),
        };
        let _ = write!(out, "{open}<span class=\"glyph\">{glyph}</span> {label}</div>");
    }

    fn label(&self, path: &str, text: &str, here: bool) -> String {
        if here {
            format!("<b>{}</b>", esc(text))
        } else {
            qsweb::partials::link(&self.rq.ctx.url(&browse_route(path)), text)
        }
    }

    /// The directories in `path`, by name.
    fn subdirs(&self, path: &str) -> Vec<String> {
        let mut v: Vec<String> = self
            .rq
            .env()
            .files
            .list(self.rq.user(), path)
            .unwrap_or_default()
            .into_iter()
            .filter(|e| e.kind == Kind::Dir)
            .map(|e| e.name)
            .collect();
        v.sort_by_key(|n| n.to_lowercase());
        v
    }

    /// The directories of `at`, the one on the path open with the rest
    /// of the path inside it.
    fn level(&self, out: &mut String, at: &str, depth: usize, rest: &[&str]) {
        let mut names = self.subdirs(at);
        if let Some(next) = rest.first()
            && !names.iter().any(|n| n == next)
        {
            names.push((*next).to_string());
            names.sort_by_key(|n| n.to_lowercase());
        }
        for n in names {
            let p = join(at, &n);
            if rest.first() != Some(&n.as_str()) {
                Self::line(out, depth, "[+]", &self.label(&p, &n, false));
                continue;
            }
            Self::line(out, depth, "[-]", &self.label(&p, &n, rest.len() == 1));
            if rest.len() == 1 {
                for c in self.subdirs(&p) {
                    Self::line(out, depth + 1, "[+]", &self.label(&join(&p, &c), &c, false));
                }
            } else {
                self.level(out, &p, depth + 1, &rest[1..]);
            }
        }
    }
}

/// The path line: the path's parts as links, the path and pattern to
/// go to, Paste when something is cut or copied.
fn path_form(rq: &Rq<'_>, dir: &str, pat: &str) -> String {
    let parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    let mut crumbs = Vec::new();
    let mut at = String::from("/");
    if parts.is_empty() {
        crumbs.push("<b>/</b>".to_string());
    } else {
        crumbs.push(qsweb::partials::link(&rq.ctx.url(&browse_route("/")), "/"));
    }
    for (i, p) in parts.iter().enumerate() {
        at = join(&at, p);
        if i + 1 == parts.len() {
            crumbs.push(format!("<b>{}</b>", esc(p)));
        } else {
            crumbs.push(qsweb::partials::link(&rq.ctx.url(&browse_route(&at)), p));
        }
    }
    let mut s = format!(
        "<p class=\"small\">Path: {} &nbsp; <input type=\"text\" name=\"path\" value=\"{}\" size=\"36\" class=\"cmd\"> Pattern: <input type=\"text\" name=\"pat\" value=\"{}\" size=\"10\" class=\"cmd\"> <input type=\"submit\" name=\"go\" value=\"Go\">",
        crumbs.join(" &gt; "),
        esc(dir),
        esc(pat)
    );
    let clip = rq.get(CLIP);
    let mut lines = clip.lines();
    if let Some(how) = lines.next() {
        let paths: Vec<String> = lines.map(|p| shown_path(p, dir)).collect();
        if !paths.is_empty() {
            let _ = write!(
                s,
                " &nbsp; <input type=\"submit\" name=\"paste\" value=\"Paste\"> <span class=\"small grey\">({} {how}: {})</span>",
                paths.len(),
                esc(&paths.join(", "))
            );
        }
    }
    s.push_str("</p>");
    qsweb::partials::form(&rq.ctx, "", &s)
}

/// The transfer queue.
fn queue(rq: &Rq<'_>, dir: &str) -> String {
    let rows = rq
        .env()
        .files
        .queue()
        .list(rq.user())
        .iter()
        .map(|t| queue_row(rq, dir, t))
        .collect();
    list_table(&List {
        cols: ["File", "Direction", "Mode", "Size", "Done", "Status", ""].iter().map(|c| Col::plain(c)).collect(),
        rows,
        empty: "No transfers in this session.".into(),
        keep: Vec::new(),
    })
}

fn queue_row(rq: &Rq<'_>, dir: &str, t: &Transfer) -> Vec<Cell> {
    let n = t.id.to_string();
    let op = |o: &str, label: &str| {
        qsweb::partials::link(&rq.ctx.url(&tokened(rq, "/files/queue", &[("n", &n), ("op", o)])), label)
    };
    let (status, action) = match &t.state {
        State::Ready => ("Ready".to_string(), op("fetch", "Download")),
        State::Running(Some(part)) => (format!("In progress (written as {})", esc(part)), op("cancel", "Cancel")),
        State::Running(None) => ("In progress".to_string(), op("cancel", "Cancel")),
        State::Complete => ("<span class=\"ok\">Complete</span>".to_string(), String::new()),
        State::Failed(id, why) => (
            format!(
                "<span class=\"err\">Failed - {} {}</span>",
                qsweb::partials::msgid(&PRODUCT, id),
                esc(why)
            ),
            op("retry", "Retry"),
        ),
        State::Cancelled => ("Cancelled".to_string(), op("retry", "Retry")),
        State::Exists => (
            format!(
                "<span class=\"err\">{} already exists</span>",
                qsweb::partials::msgid(&PRODUCT, "QAD8010")
            ),
            format!("{} | {}", op("replace", "Replace"), op("skip", "Skip")),
        ),
        State::Skipped => ("Skipped".to_string(), String::new()),
    };
    vec![
        Cell::mono(&shown_path(&t.path, dir)),
        Cell::text(match t.direction {
            Direction::Upload => "Upload",
            Direction::Download => "Download",
        }),
        Cell::text(&t.mode.shown(t.direction)),
        Cell::num(&size(t.size)),
        Cell::num(&size(t.done)),
        Cell::html(status),
        Cell::html(action),
    ]
}

/// Binary or text, as the session or the preference says.
fn text_default(rq: &Rq<'_>, session_key: &str) -> bool {
    match rq.get(session_key).as_str() {
        "text" => true,
        "bin" => false,
        _ => rq.prefs().get(prefs::TRANSFER) == "Text",
    }
}

/// Whether downloads convert text.
pub fn download_text(rq: &Rq<'_>) -> bool {
    text_default(rq, DOWNLOAD_MODE)
}

/// The line endings of text downloads.
pub fn download_crlf(rq: &Rq<'_>) -> bool {
    match rq.get(DOWNLOAD_ENDINGS).as_str() {
        "LF" => false,
        "CR LF" => true,
        _ => rq.prefs().get(prefs::ENDINGS).starts_with("CRLF"),
    }
}

fn upload_form(rq: &Rq<'_>, dir: &str) -> String {
    let text = rq.prefs().get(prefs::TRANSFER) == "Text";
    let radio = |name: &str, value: &str, on: bool| qsweb::partials::radio(name, value, on);
    let ex = rq.prefs().get(prefs::EXISTING);
    let dtext = download_text(rq);
    let dle = if download_crlf(rq) { "CR LF" } else { "LF" };
    let inner = format!(
        "<table class=\"props\"><tr><th>Upload</th><td><input type=\"file\" name=\"f\" size=\"30\" multiple> to <span class=\"mono\">{}</span> &nbsp; {} Binary {} Text, CCSID <input type=\"text\" name=\"ccsid\" value=\"1208\" size=\"5\"> line endings <select name=\"le\"><option>LF on the system</option></select> &nbsp; If it exists: {} <input type=\"submit\" value=\"Upload\"></td></tr><tr><th>Download options</th><td>{} Binary {} Text: convert from the file's CCSID to UTF-8, line endings {}</td></tr></table>",
        esc(dir),
        radio("mode", "bin", !text),
        radio("mode", "text", text),
        select("ex", EXISTING, &ex),
        radio("dm", "bin", !dtext),
        radio("dm", "text", dtext),
        select("dle", &["CR LF", "LF"], dle),
    );
    qsweb::partials::upload_form(&rq.ctx, "", &inner)
}

/// The selected names of a post.
pub fn selected(rq: &Rq<'_>) -> Vec<String> {
    rq.form()
        .all("s")
        .into_iter()
        .filter(|n| super::good_name(n))
        .map(ToString::to_string)
        .collect()
}

/// The forms' posts.
fn post(rq: &Rq<'_>, dir: &str) -> Response {
    if rq.req.multipart().is_some() {
        return super::transfer::upload(rq, dir);
    }
    let f = rq.form();
    if f.has("go") {
        let to = super::clean(f.get("path").map_or(dir, str::trim));
        let pat = f.str("pat").trim();
        let mut q: Vec<(&str, &str)> = vec![("path", &to)];
        if !pat.is_empty() && pat != "*" {
            q.push(("pat", pat));
        }
        return rq.redirect(&format!("{BROWSE}{}", query(&q)));
    }
    if f.has("paste") {
        return super::ops::paste(rq, dir);
    }
    if f.has("mkdir") {
        return rq.redirect(&format!("/files/mkdir{}", query(&[("path", dir)])));
    }
    let names = selected(rq);
    if names.is_empty() {
        return super::back(
            rq,
            dir,
            rq.message(qsweb::msg::Kind::Info, crate::msgs::NONE_SELECTED, &[]),
        );
    }
    if f.has("download") {
        return super::transfer::download_selected(rq, dir, &names);
    }
    if f.has("cut") || f.has("copy") {
        let how = if f.has("cut") { "cut" } else { "copy" };
        let mut v = vec![how.to_string()];
        v.extend(names.iter().map(|n| join(dir, n)));
        rq.set(&[(CLIP, &v.join("\n"))]);
        return rq.redirect(&browse_route(dir));
    }
    if f.has("delete") {
        let mut q: Vec<(&str, &str)> = vec![("path", dir)];
        q.extend(names.iter().map(|n| ("s", n.as_str())));
        return rq.redirect(&format!("/files/confirm{}", query(&q)));
    }
    rq.redirect(&browse_route(dir))
}

/// The name a path ends in, for messages.
pub fn short(path: &str) -> &str {
    name_of(path)
}
