// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/outq`: the output queues (`WRKOUTQ`) with Hold, Release and Clear;
//! a queue's spooled files (`WRKOUTQ OUTQ()`, `/outq/<lib>/<name>`) and
//! a job's (`/job/.../splf`); `/splf/<job>/<file>/<number>`: a spooled
//! file as 132-column text with page navigation and Find (`DSPSPLF`),
//! as PDF and as text, and Hold (`HLDSPLF`), Release (`RLSSPLF`), Delete
//! (`DLTSPLF`, confirmed) and Move (`CHGSPLFA OUTQ()`).

use std::fmt::Write as _;

use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::partials::{
    Button, Cell, Col, FilterField, List, Paging, Prop, PropGroup, actions, buttons, filter, form,
    list_table, paging, props_table, select,
};

use super::{
    Confirm, all_or, confirm_page, confirmed, keep, matches, mono, need, page, strings, word,
};
use crate::env::work::{Choices, OutQueue, Splf};
use crate::env::{Need, Target};
use crate::routes::Rq;

/// The spooled file statuses the filter offers.
const STATUSES: &[&str] = &["*ALL", "RDY", "HLD", "SAV", "WTR", "MSGW"];

/// Route `/outq/...`.
pub fn route(rq: &Rq<'_>, rest: &[&str]) -> Response {
    let [lib, name, more @ ..] = rest else {
        return if rest.is_empty() {
            queues(rq)
        } else {
            rq.not_found(&format!("outq/{}", rest.join("/")))
        };
    };
    let Some(q) = rq
        .env()
        .work
        .output_queues(rq.user())
        .into_iter()
        .find(|q| q.lib == *lib && q.name == *name)
    else {
        return rq.not_found(&format!("output queue {lib}/{name}"));
    };
    match more {
        [] => files(rq, &q),
        [op @ ("hold" | "release" | "confirm")] => act(rq, &q, op),
        ["props"] => props(rq, &q),
        _ => rq.not_found(&format!(
            "page {} of output queue {lib}/{name}",
            more.join("/")
        )),
    }
}

fn held(h: bool) -> String {
    if h {
        word("Held", Some(false))
    } else {
        word("Released", Some(true))
    }
}

/// `/outq?lib=&q=&sort=`.
fn queues(rq: &Rq<'_>) -> Response {
    let q = rq.query();
    let lib = all_or(q.str("lib"));
    let name = all_or(q.str("q"));
    let key = q.str("sort");
    let w = &rq.env().work;
    let mut qs: Vec<OutQueue> = w
        .output_queues(rq.user())
        .into_iter()
        .filter(|o| (lib == "*ALL" || o.lib == lib) && matches(&name, &o.name))
        .collect();
    match key {
        "name" => qs.sort_by(|a, b| a.name.cmp(&b.name)),
        "lib" => qs.sort_by(|a, b| a.lib.cmp(&b.lib)),
        "files" => qs.sort_by_key(|o| std::cmp::Reverse(o.files)),
        "wtr" => qs.sort_by(|a, b| a.writer.cmp(&b.writer)),
        "st" => qs.sort_by_key(|o| o.held),
        _ => {}
    }
    let mut libs = vec!["*ALL".to_string()];
    libs.extend(w.choices(rq.user(), Choices::OutqLibraries));
    let f = filter(
        &[
            FilterField::Select(
                "Library".into(),
                "lib".into(),
                libs,
                if lib == "*ALL" {
                    String::new()
                } else {
                    lib.clone()
                },
            ),
            FilterField::Text("Queue".into(), "q".into(), name.clone(), 10),
        ],
        false,
    );
    let rows = qs
        .iter()
        .map(|o| {
            let base = format!("/outq/{}/{}", o.lib, o.name);
            let u = |t: &str| rq.ctx.url(&format!("{base}{t}"));
            vec![
                Cell::act(actions(&[
                    (u(""), "Open"),
                    (u("/hold"), "Hold"),
                    (u("/release"), "Release"),
                    (u("/confirm"), "Clear"),
                    (u("/props"), "Properties"),
                ])),
                Cell::mono(&o.name),
                Cell::mono(&o.lib),
                Cell::num(&o.files.to_string()),
                Cell::mono(&o.writer),
                Cell::html(held(o.held)),
                Cell::text(&o.text),
            ]
        })
        .collect();
    let list = List {
        cols: vec![
            Col::plain("Actions"),
            Col::sorted("Output queue", "name"),
            Col::sorted("Library", "lib"),
            Col::sorted("Files", "files"),
            Col::sorted("Writer", "wtr"),
            Col::sorted("Status", "st"),
            Col::plain("Text"),
        ],
        rows,
        empty: "No output queues match the filter.".into(),
        keep: keep(&[("lib", &lib), ("q", &name)]),
    };
    let body = format!("{f}{}", list_table(&list));
    page(
        rq,
        &["Output queues"],
        "Output queues",
        Some("Output queues and the spooled files on them (WRKOUTQ)."),
        body,
        "outq",
        Some("/outq"),
    )
}

/// Hold, Release or Clear (`confirm`) an output queue.
fn act(rq: &Rq<'_>, o: &OutQueue, op: &str) -> Response {
    let qual = format!("{}/{}", o.lib, o.name);
    let (verb, cmd) = match op {
        "hold" => ("Hold", "HLDOUTQ"),
        "release" => ("Release", "RLSOUTQ"),
        _ => ("Clear", "CLROUTQ"),
    };
    let what = format!("{verb} Output Queue");
    let command = format!("{cmd} OUTQ({qual})");
    if rq.post() {
        let t = Target::obj(&o.lib, &o.name, "*OUTQ");
        if let Err(r) = rq.require(
            Need::JobCtl,
            &what,
            &t,
            &["Work management", "Output queues"],
        ) {
            return r;
        }
        return confirmed(rq, &command, "/outq");
    }
    let sentence = if op == "confirm" {
        format!(
            "Clearing an output queue deletes every spooled file on it ({} now). Press OK to clear the output queues listed below, or Cancel to return without clearing.",
            o.files
        )
    } else {
        format!(
            "Press OK to {} the output queues listed below.",
            verb.to_ascii_lowercase()
        )
    };
    confirm_page(
        rq,
        &Confirm {
            what: &what,
            crumb: &["Output queues"],
            sentence,
            cols: &["Output queue", "Status", "Files", "Writer", "Text"],
            rows: vec![vec![
                mono(&qual),
                held(o.held),
                o.files.to_string(),
                mono(&o.writer),
                esc(&o.text),
            ]],
            options: String::new(),
            command,
        },
    )
}

/// `/outq/<lib>/<name>/props`: the queue's attributes (`WRKOUTQD`).
fn props(rq: &Rq<'_>, o: &OutQueue) -> Response {
    let qual = format!("{}/{}", o.lib, o.name);
    let rows = vec![
        Prop::new("Output queue", mono(&qual)),
        Prop::new("Status", held(o.held)),
        Prop::new("Files", o.files.to_string()),
        Prop::new("Writer", mono(&o.writer)),
        Prop::new("Order of files", mono(&o.order)),
        Prop::new(
            "Operator controlled",
            if o.operator_control { "*YES" } else { "*NO" }.to_string(),
        ),
        Prop::new("Text", esc(&o.text)),
    ];
    let body = format!(
        "{}<p><a href=\"{}\">All output queues</a></p>",
        props_table(rq.ctx.product, &[PropGroup { title: None, rows }], true),
        esc(&rq.ctx.url("/outq"))
    );
    let h1 = format!("Output queue: {qual}");
    page(
        rq,
        &["Output queues", &o.name],
        &h1,
        None,
        body,
        "outq",
        None,
    )
}

/// A spooled file's route.
fn splf_route(f: &Splf) -> String {
    format!("/splf/{}/{}/{}", f.job, f.file, f.number)
}

fn splf_status(s: &str) -> String {
    if s == "HLD" {
        word("Held", Some(false))
    } else {
        esc(s)
    }
}

/// The spooled files list, filtered, sorted and paged.
#[allow(clippy::many_single_char_names)]
fn file_list(rq: &Rq<'_>, all: Vec<Splf>) -> String {
    let q = rq.query();
    let user = all_or(q.str("u"));
    let ud = all_or(q.str("ud"));
    let st = all_or(q.str("s"));
    let ft = all_or(q.str("ft"));
    let key = q.str("sort").to_string();
    let mut fs: Vec<Splf> = all
        .into_iter()
        .filter(|f| {
            matches(&user, &f.user) && matches(&ud, &f.user_data) && matches(&ft, &f.form_type)
        })
        .filter(|f| st == "*ALL" || f.status == st)
        .collect();
    match key.as_str() {
        "file" => fs.sort_by(|a, b| a.file.cmp(&b.file)),
        "user" => fs.sort_by(|a, b| a.user.cmp(&b.user)),
        "ud" => fs.sort_by(|a, b| a.user_data.cmp(&b.user_data)),
        "st" => fs.sort_by(|a, b| a.status.cmp(&b.status)),
        "pages" => fs.sort_by_key(|f| std::cmp::Reverse(f.pages)),
        "crt" => fs.sort_by_key(|f| f.created),
        _ => {}
    }
    let filters = keep(&[("u", &user), ("ud", &ud), ("s", &st), ("ft", &ft)]);
    let mut kept = filters.clone();
    if !key.is_empty() {
        kept.push(("sort".into(), key));
    }
    let p = Paging::from_query(q.str("page"), q.str("rows"), fs.len(), kept);
    let f = filter(
        &[
            FilterField::Text("User".into(), "u".into(), user.clone(), 10),
            FilterField::Text("User data".into(), "ud".into(), ud.clone(), 10),
            FilterField::Select(
                "Status".into(),
                "s".into(),
                strings(STATUSES),
                if st == "*ALL" {
                    String::new()
                } else {
                    st.clone()
                },
            ),
            FilterField::Text("Form type".into(), "ft".into(), ft.clone(), 10),
        ],
        false,
    );
    let rows = fs[p.range()]
        .iter()
        .map(|s| {
            let r = splf_route(s);
            let u = |t: &str| rq.ctx.url(&format!("{r}{t}"));
            let (d, t) = rq.stamp(s.created);
            vec![
                Cell::act(actions(&[
                    (u(""), "View"),
                    (u("/pdf"), "PDF"),
                    (u("/hold"), "Hold"),
                    (u("/release"), "Release"),
                    (u("/confirm"), "Delete"),
                    (u("/move"), "Move"),
                ])),
                Cell::mono_html(format!("<a href=\"{}\">{}</a>", esc(&u("")), esc(&s.file))),
                Cell::mono(&s.user),
                Cell::mono(&s.user_data),
                Cell::mono_html(splf_status(&s.status)),
                Cell::num(&s.pages.to_string()),
                Cell::num(&s.copies.to_string()),
                Cell::mono(&s.form_type),
                Cell::text(&format!("{d} {}", t.get(..5).unwrap_or(&t))),
                Cell::mono(&s.job),
                Cell::num(&s.number.to_string()),
            ]
        })
        .collect();
    let list = List {
        cols: vec![
            Col::plain("Actions"),
            Col::sorted("File", "file"),
            Col::sorted("User", "user"),
            Col::sorted("User data", "ud"),
            Col::sorted("Status", "st"),
            Col::sorted("Pages", "pages"),
            Col::plain("Copies"),
            Col::plain("Form type"),
            Col::sorted("Created", "crt"),
            Col::plain("Job"),
            Col::plain("File nbr"),
        ],
        rows,
        empty: "No spooled files match the filter.".into(),
        keep: filters,
    };
    format!("{f}{}{}", list_table(&list), paging(&p))
}

/// `/outq/<lib>/<name>?u=&ud=&s=&ft=&sort=&page=`.
fn files(rq: &Rq<'_>, o: &OutQueue) -> Response {
    let qual = format!("{}/{}", o.lib, o.name);
    let w = &rq.env().work;
    let writer = if o.writer.is_empty() {
        "<span class=\"mono\">*NONE</span>".to_string()
    } else {
        let started = w
            .writers(rq.user())
            .iter()
            .any(|x| x.device == o.writer && x.started);
        format!(
            "{} ({})",
            mono(&o.writer),
            if started {
                word("Started", Some(true))
            } else {
                word("Ended", Some(false))
            }
        )
    };
    let all = w.spooled_files(rq.user(), &qual, "");
    let head = format!(
        "<p class=\"small\">Output queue: <b class=\"mono\">{}</b> &nbsp; Status: {} &nbsp; Writer: {writer} &nbsp; Files: {} &nbsp; <a href=\"{}\">All output queues</a></p>",
        esc(&qual),
        held(o.held),
        all.len(),
        esc(&rq.ctx.url("/outq"))
    );
    let body = format!("{head}{}", file_list(rq, all));
    let h1 = format!("Spooled files: {qual}");
    page(
        rq,
        &["Output queues", &o.name],
        &h1,
        None,
        body,
        "splf",
        None,
    )
}

/// `/job/<number>/<user>/<name>/splf`: a job's spooled files.
pub fn job_files(rq: &Rq<'_>, job: &str) -> Response {
    let all = rq.env().work.spooled_files(rq.user(), "", job);
    let head = format!(
        "<p class=\"small\">Job: <b class=\"mono\">{}</b> &nbsp; Files: {} &nbsp; <a href=\"{}\">Job properties</a></p>",
        esc(job),
        all.len(),
        esc(&rq.ctx.url(&format!("/job/{job}/general")))
    );
    let body = format!("{head}{}", file_list(rq, all));
    let h1 = format!("Spooled files: {job}");
    page(
        rq,
        &["Active jobs", job, "Spooled files"],
        &h1,
        None,
        body,
        "splf",
        None,
    )
}

/// `/splf/<number>/<user>/<job>/<file>/<nbr>/...`.
pub fn splf(rq: &Rq<'_>, job: &str, file: &str, nbr: &str, rest: &[&str]) -> Response {
    let w = &rq.env().work;
    let n: u32 = nbr.parse().unwrap_or(0);
    let Some((f, pages)) = w.spooled_text(rq.user(), job, file, n) else {
        return rq.not_found(&format!("spooled file {file} number {nbr} of job {job}"));
    };
    match rest {
        [] => view(rq, &f, &pages),
        ["pdf"] => match w.spooled_pdf(rq.user(), job, file, n) {
            Some(pdf) => Response::download(&format!("{file}-{nbr}.pdf"), "application/pdf", pdf),
            None => rq.not_found(&format!("PDF of spooled file {file}")),
        },
        ["text"] => Response::download(
            &format!("{file}-{nbr}.txt"),
            "text/plain; charset=utf-8",
            pages.concat().into_bytes(),
        ),
        [op @ ("hold" | "release" | "confirm")] => act_splf(rq, &f, op),
        ["move"] => move_splf(rq, &f),
        _ => rq.not_found(&format!("page {} of spooled file {file}", rest.join("/"))),
    }
}

/// The spooled file view: one page, its navigation and Find.
fn view(rq: &Rq<'_>, f: &Splf, pages: &[String]) -> Response {
    let q = rq.query();
    let total = pages.len().max(1);
    let mut at: usize = q.str("p").trim().parse().unwrap_or(1).clamp(1, total);
    let find = q.str("f").to_string();
    let mut messages = Vec::new();
    if !find.is_empty() {
        // From the page shown on, the first page holding the text.
        let needle = find.to_ascii_uppercase();
        match (at..=total)
            .chain(1..at)
            .find(|p| pages[p - 1].to_ascii_uppercase().contains(&needle))
        {
            Some(p) => at = p,
            None => messages.push(rq.message(qsweb::msg::Kind::Info, "CPI9801", &[&find])),
        }
    }
    let r = splf_route(f);
    let base = rq.ctx.url(&r);
    let nav = |p: usize, label: &str, on: bool| {
        if on {
            format!("<a href=\"{}?p={p}\">{}</a>", esc(&base), esc(label))
        } else {
            format!("<span class=\"grey\">{}</span>", esc(label))
        }
    };
    let queue = f.outq.rsplit('/').next().unwrap_or(&f.outq).to_string();
    let (ql, qn) = f.outq.split_once('/').unwrap_or(("QGPL", &f.outq));
    let head = format!(
        "<p class=\"small\">File: <b class=\"mono\">{}</b> &nbsp; Job: <span class=\"mono\">{}</span> &nbsp; File number: {} &nbsp; Pages: {} &nbsp; 132 columns &nbsp; <a href=\"{}\">Back to {}</a></p>",
        esc(&f.file),
        esc(&f.job),
        f.number,
        f.pages,
        esc(&rq.ctx.url(&format!("/outq/{ql}/{qn}"))),
        esc(&queue)
    );
    let toolbar = format!(
        "<form method=\"get\"><div class=\"toolbar\">Page: <input type=\"text\" name=\"p\" value=\"{at}\" size=\"4\"> of {total} <input type=\"submit\" value=\"Go\"> &nbsp; {} &nbsp; {} &nbsp;&nbsp; Find: <input type=\"text\" name=\"f\" value=\"{}\" size=\"16\"> <input type=\"submit\" value=\"Find\"> &nbsp;&nbsp; <a href=\"{}\">View as PDF</a> &nbsp; <a href=\"{}\">Download text</a> &nbsp; <a href=\"{}\">Hold</a> &nbsp; <a href=\"{}\">Release</a> &nbsp; <a href=\"{}\">Delete</a></div></form>",
        nav(at.saturating_sub(1), "< Previous", at > 1),
        nav(at + 1, "Next >", at < total),
        esc(&find),
        esc(&format!("{base}/pdf")),
        esc(&format!("{base}/text")),
        esc(&format!("{base}/hold")),
        esc(&format!("{base}/release")),
        esc(&format!("{base}/confirm")),
    );
    let text = pages.get(at - 1).map_or("", String::as_str);
    let body = format!(
        "{head}{toolbar}<pre class=\"out out132\">{}</pre>",
        esc(text)
    );
    let h1 = format!("Spooled file: {}", f.file);
    let mut c = vec![
        "Work management".to_string(),
        "Output queues".to_string(),
        queue,
        f.file.clone(),
    ];
    if f.outq.is_empty() {
        c.truncate(1);
    }
    rq.page(qsweb::frame::Page {
        title: h1.clone(),
        crumb: c,
        h1,
        messages,
        body,
        help: "splf-view".into(),
        ..qsweb::frame::Page::default()
    })
}

/// The spooled file's CL parameters.
fn parms(f: &Splf) -> String {
    format!("FILE({}) JOB({}) SPLNBR({})", f.file, f.job, f.number)
}

/// The function's authority check: another user's file needs `*JOBCTL`
/// (or `*SPLCTL`, which the command itself accepts).
fn allowed(rq: &Rq<'_>, f: &Splf, function: &str) -> Result<(), Response> {
    let own = f.user == rq.user() || rq.has(Need::SplCtl);
    let t = Target {
        shown: format!("{} {}", f.job, f.file),
        name: f.file.clone(),
        lib: String::new(),
        typ: "*SPLF".into(),
    };
    need(
        rq,
        own,
        Need::JobCtl,
        function,
        &t,
        &["Work management", "Output queues"],
    )
}

fn back(f: &Splf) -> String {
    let (l, n) = f.outq.split_once('/').unwrap_or(("QGPL", "QPRINT"));
    format!("/outq/{l}/{n}")
}

const COLS: &[&str] = &["File", "User", "Status", "Pages", "Job", "File nbr"];

fn confirm_row(f: &Splf) -> Vec<String> {
    vec![
        mono(&f.file),
        esc(&f.user),
        splf_status(&f.status),
        f.pages.to_string(),
        mono(&f.job),
        f.number.to_string(),
    ]
}

/// Hold, Release or Delete (`confirm`) a spooled file.
fn act_splf(rq: &Rq<'_>, f: &Splf, op: &str) -> Response {
    let (verb, cmd) = match op {
        "hold" => ("Hold", "HLDSPLF"),
        "release" => ("Release", "RLSSPLF"),
        _ => ("Delete", "DLTSPLF"),
    };
    let what = format!("{verb} Spooled File");
    let command = format!("{cmd} {}", parms(f));
    if rq.post() {
        if let Err(r) = allowed(rq, f, &what) {
            return r;
        }
        return confirmed(rq, &command, &back(f));
    }
    let sentence = if op == "confirm" {
        "A deleted spooled file cannot be printed or viewed again. Press OK to delete the spooled files listed below, or Cancel to return without deleting.".to_string()
    } else {
        format!(
            "Press OK to {} the spooled files listed below.",
            verb.to_ascii_lowercase()
        )
    };
    let queue = f.outq.rsplit('/').next().unwrap_or("").to_string();
    confirm_page(
        rq,
        &Confirm {
            what: &what,
            crumb: &["Output queues", &queue],
            sentence,
            cols: COLS,
            rows: vec![confirm_row(f)],
            options: String::new(),
            command,
        },
    )
}

/// `/splf/.../move`: Move to another output queue (`CHGSPLFA OUTQ()`).
fn move_splf(rq: &Rq<'_>, f: &Splf) -> Response {
    let queues: Vec<String> = rq
        .env()
        .work
        .output_queues(rq.user())
        .iter()
        .map(|o| format!("{}/{}", o.lib, o.name))
        .filter(|q| *q != f.outq)
        .collect();
    if rq.post() {
        let form = rq.form();
        if form.has("cancel") {
            return rq.redirect(&back(f));
        }
        let to = form.str("outq");
        if !queues.iter().any(|q| q == to) {
            return rq.redirect(&format!("{}/move", splf_route(f)));
        }
        if let Err(r) = allowed(rq, f, "Move spooled file") {
            return r;
        }
        return super::run(rq, &format!("CHGSPLFA {} OUTQ({to})", parms(f)), &back(f));
    }
    let opts: Vec<&str> = queues.iter().map(String::as_str).collect();
    let rows = vec![
        Prop::new(
            "Spooled file",
            mono(&format!("{} {} {}", f.job, f.file, f.number)),
        ),
        Prop::new("Output queue now", mono(&f.outq)),
        Prop::new("Move to", select("outq", &opts, "")).required(),
    ];
    let mut body = String::new();
    let _ = write!(
        body,
        "<p>Choose the output queue to move the spooled file to. The command run is <span class=\"mono\">CHGSPLFA OUTQ()</span>.</p>{}",
        form(
            &rq.ctx,
            "",
            &format!(
                "{}{}",
                props_table(rq.ctx.product, &[PropGroup { title: None, rows }], true),
                buttons(&[Button::new("ok", "OK"), Button::new("cancel", "Cancel")])
            )
        )
    );
    let queue = f.outq.rsplit('/').next().unwrap_or("").to_string();
    page(
        rq,
        &["Output queues", &queue, "Move Spooled File"],
        "Move Spooled File",
        None,
        body,
        "splf",
        None,
    )
}
