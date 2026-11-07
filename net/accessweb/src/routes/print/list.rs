// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Printer output: the filter (user, output queue, status, form type,
//! user data), Refresh and Auto-refresh (`<meta refresh>`), the list a
//! page at a time (a column heading sorts the page shown, as the
//! desktop's does), and the selected files' actions.

use std::fmt::Write as _;

use qsweb::frame::Page;
use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::msg::Kind;
use qsweb::partials::{Cell, Col, List, Paging, ROW_CHOICES, actions, list_table, paging};

use super::{GROUP, OUTPUT, status, view_route};
use crate::env::print::Splf;
use crate::msgs;
use crate::policy;
use crate::routes::Rq;
use crate::routes::files::browse::{select, short_time};

/// The Status filter's choices.
const STATUSES: &[&str] = &["*ALL", "RDY", "HLD", "SAV", "WTR", "MSGW"];
/// The Auto-refresh choices.
const AUTO: &[&str] = &["Off", "15", "30", "60"];
/// The session value of the results waiting for the next page (a line
/// a file: name, message ID, text).
pub const DETAILS: &str = "print.details";

/// The filter as the query gives it.
pub struct Filter {
    /// `*CURRENT`, `*ALL` or a user.
    pub user: String,
    /// `*ALL` or `LIB/QUEUE`.
    pub queue: String,
    /// `*ALL` or a status.
    pub status: String,
    /// `*ALL` or a form type.
    pub form: String,
    /// `*ALL` or user data.
    pub data: String,
}

impl Filter {
    fn of(rq: &Rq<'_>) -> Self {
        let q = rq.query();
        let get = |k: &str| {
            let v = q.str(k).trim().to_ascii_uppercase();
            if v.is_empty() { "*ALL".to_string() } else { v }
        };
        let user = match q.str("u").trim().to_ascii_uppercase() {
            v if v.is_empty() => "*CURRENT".to_string(),
            v => v,
        };
        // A queue typed wins over the one chosen.
        let typed = q.str("qn").trim().to_ascii_uppercase();
        let queue = if typed.is_empty() {
            get("q")
        } else if typed.contains('/') {
            typed
        } else {
            format!("*LIBL/{typed}")
        };
        Self {
            user,
            queue,
            status: get("s"),
            form: get("ft"),
            data: get("ud"),
        }
    }

    /// The query pairs that differ from the defaults.
    fn keep(&self) -> Vec<(String, String)> {
        let mut v = Vec::new();
        for (k, val, dft) in [
            ("u", &self.user, "*CURRENT"),
            ("q", &self.queue, "*ALL"),
            ("s", &self.status, "*ALL"),
            ("ft", &self.form, "*ALL"),
            ("ud", &self.data, "*ALL"),
        ] {
            if val != dft {
                v.push((k.to_string(), val.clone()));
            }
        }
        v
    }

    fn matches(&self, f: &Splf) -> bool {
        let queue = match self.queue.strip_prefix("*LIBL/") {
            Some(n) => f.outq.rsplit('/').next() == Some(n),
            None => self.queue == "*ALL" || f.outq == self.queue,
        };
        queue
            && (self.status == "*ALL" || f.status == self.status)
            && (self.form == "*ALL" || f.form_type.eq_ignore_ascii_case(&self.form))
            && (self.data == "*ALL" || f.user_data.eq_ignore_ascii_case(&self.data))
    }
}

/// The page sorted by `key`, as the column headings say.
fn sort_page(v: &mut [Splf], key: &str) {
    match key {
        "f" => v.sort_by(|a, b| a.file.cmp(&b.file)),
        "u" => v.sort_by(|a, b| a.user.cmp(&b.user)),
        "ud" => v.sort_by(|a, b| a.user_data.cmp(&b.user_data)),
        "s" => v.sort_by(|a, b| a.status.cmp(&b.status)),
        "p" => v.sort_by_key(|f| f.pages),
        "q" => v.sort_by(|a, b| a.outq.cmp(&b.outq)),
        "c" => v.sort_by_key(|f| f.created),
        _ => {}
    }
}

/// `GET` and `POST /print`.
#[allow(clippy::too_many_lines)]
pub fn page(rq: &Rq<'_>) -> Response {
    if rq.post() {
        return super::act::post(rq);
    }
    let q = rq.query();
    let flt = Filter::of(rq);
    let others = rq.allowed(policy::PRINT_OTHERS);
    let owner = match flt.user.as_str() {
        "*CURRENT" => Some(rq.user().to_string()),
        "*ALL" => None,
        u => Some(u.to_string()),
    };
    if owner.as_deref() != Some(rq.user())
        && let Err(r) = rq.policy(policy::PRINT_OTHERS, &[GROUP, "Printer output"])
    {
        return r;
    }
    let mut messages = Vec::new();
    let all = match rq.env().print.spooled(rq.user(), owner.as_deref()) {
        Ok(v) => v,
        Err(m) => {
            messages.push(crate::routes::files::error(rq, &m));
            Vec::new()
        }
    };
    let mut queues: Vec<String> = vec!["*ALL".into()];
    for f in &all {
        if !queues.contains(&f.outq) {
            queues.push(f.outq.clone());
        }
    }
    let mut statuses: Vec<String> = STATUSES.iter().map(ToString::to_string).collect();
    for f in &all {
        if !statuses.contains(&f.status) {
            statuses.push(f.status.clone());
        }
    }
    let hits: Vec<Splf> = all.into_iter().filter(|f| flt.matches(f)).collect();
    let sort = q.str("sort").to_string();
    let auto = q.get("auto").filter(|a| AUTO[1..].contains(a)).unwrap_or("Off").to_string();
    let rows = q
        .str("rows")
        .parse()
        .ok()
        .filter(|r| ROW_CHOICES.contains(r))
        .unwrap_or_else(|| rq.prefs().rows());
    let mut keep = flt.keep();
    if auto != "Off" {
        keep.push(("auto".into(), auto.clone()));
    }
    let mut pg_keep = keep.clone();
    if !sort.is_empty() {
        pg_keep.push(("sort".into(), sort.clone()));
    }
    let mut pg = Paging {
        page: 1,
        rows,
        total: hits.len(),
        keep: pg_keep,
    };
    pg.page = q.str("page").parse().unwrap_or(1).clamp(1, pg.pages());
    let mut shown: Vec<Splf> = hits[pg.range()].to_vec();
    sort_page(&mut shown, &sort);

    // The filter row.
    let mut users: Vec<String> = vec!["*CURRENT".into()];
    if others {
        users.push("*ALL".into());
        users.extend(rq.env().print.users(rq.user()));
    }
    let typed = q.str("qn");
    let mut body = format!(
        "<form method=\"get\"><table class=\"filter\" cellspacing=\"0\"><tr><td>User:</td><td>{}</td><td>Output queue:</td><td>{} or <input type=\"text\" name=\"qn\" value=\"{}\" size=\"20\" class=\"cmd\"></td><td>Status:</td><td>{}</td><td>Form type:</td><td><input type=\"text\" name=\"ft\" value=\"{}\" size=\"8\"></td><td>User data:</td><td><input type=\"text\" name=\"ud\" value=\"{}\" size=\"10\"></td><td><input type=\"submit\" value=\"Go\"> <input type=\"submit\" name=\"refresh\" value=\"Refresh\"> &nbsp; Auto-refresh: {} s</td></tr></table></form>",
        select("u", &strs(&users), &flt.user),
        select("q", &strs(&queues), if typed.is_empty() { &flt.queue } else { "*ALL" }),
        esc(typed),
        select("s", &strs(&statuses), &flt.status),
        esc(&flt.form),
        esc(&flt.data),
        select("auto", AUTO, &auto),
    );
    // The results of the last action, file by file.
    let details = rq.get(DETAILS);
    if !details.is_empty() {
        rq.set(&[(DETAILS, "")]);
        body.insert_str(0, &details_table(&details));
    }

    let text = rq.allowed(policy::PRINT_TEXT);
    let pdf = rq.allowed(policy::PRINT_PDF);
    let rows: Vec<Vec<Cell>> = shown
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let v = rq.ctx.url(&view_route(f));
            let open = if text { v.clone() } else { format!("{v}/pdf") };
            let mut acts: Vec<(String, &str)> = vec![(open.clone(), "Open")];
            if pdf {
                acts.push((format!("{v}/pdf"), "PDF"));
            }
            if text {
                acts.push((format!("{v}/text"), "Text"));
            }
            acts.push((rq.ctx.url(&format!("/print/props/{}", f.id())), "Properties"));
            let (d, t) = rq.stamp(f.created);
            vec![
                Cell::html(format!("<input type=\"checkbox\" name=\"s{i}\" value=\"{}\">", esc(&f.id()))),
                Cell::act(actions(&acts)),
                Cell::mono_html(qsweb::partials::link(&open, &f.file)),
                Cell::mono(&f.user),
                Cell::mono(&f.user_data),
                Cell::mono_html(status(&f.status)),
                Cell::num(&f.pages.to_string()),
                Cell::num(&f.copies.to_string()),
                Cell::mono(&f.form_type),
                Cell::mono(&f.outq),
                Cell::text(&format!("{d} {}", short_time(&t))),
                Cell::mono(&f.job),
            ]
        })
        .collect();
    let list = List {
        cols: vec![
            Col::plain("Select"),
            Col::plain("Actions"),
            Col::sorted("File", "f"),
            Col::sorted("User", "u"),
            Col::sorted("User data", "ud"),
            Col::sorted("Status", "s"),
            Col::sorted("Pages", "p"),
            Col::plain("Copies"),
            Col::plain("Form type"),
            Col::sorted("Output queue", "q"),
            Col::sorted("Created", "c"),
            Col::plain("Job"),
        ],
        rows,
        empty: "No spooled files match the filter.".into(),
        keep,
    };
    body.push_str(&qsweb::partials::form(&rq.ctx, "", &format!("{}{}", list_table(&list), buttons(rq))));
    body.push_str(&paging(&pg));
    rq.page(Page {
        title: "Printer output".into(),
        crumb: vec![GROUP.into(), "Printer output".into()],
        h1: "Printer output".into(),
        desc: Some("Spooled files on the system, as QS/400 Access for Desktop's Printer Output lists them (QAD6001 gives the total).".into()),
        messages,
        body,
        help: "printer-output".into(),
        nav: Some((GROUP, Some(OUTPUT))),
        refresh: auto.parse().ok(),
        ..Page::default()
    })
}

/// The selection's actions, as the policies leave them.
fn buttons(rq: &Rq<'_>) -> String {
    let mut s = String::from("<div class=\"buttons\">Selected: ");
    if rq.allowed(policy::PRINT_OWN) {
        s.push_str("<input type=\"submit\" name=\"hold\" value=\"Hold\"> <input type=\"radio\" name=\"hw\" value=\"now\" checked> now <input type=\"radio\" name=\"hw\" value=\"page\"> at end of page &nbsp; <input type=\"submit\" name=\"release\" value=\"Release\"> <input type=\"submit\" name=\"delete\" value=\"Delete...\"> &nbsp; ");
    }
    if rq.allowed(policy::PRINT_MOVE) {
        let queues = rq.env().print.queues(rq.user());
        let to = rq.query().str("to").to_string();
        let qs: Vec<&str> = queues.iter().map(String::as_str).collect();
        let _ = write!(
            s,
            "Move to {} <input type=\"checkbox\" name=\"top\"> top of queue <input type=\"submit\" name=\"move\" value=\"Move\"> &nbsp; ",
            select("to", &qs, &to)
        );
    }
    if rq.allowed(policy::PRINT_PDF) {
        s.push_str("<input type=\"submit\" name=\"pdf\" value=\"Save as PDF\"> ");
    }
    if rq.allowed(policy::PRINT_TEXT) {
        s.push_str("<input type=\"submit\" name=\"text\" value=\"Save as text\">");
    }
    let s = s.trim_end().trim_end_matches("&nbsp;").trim_end().to_string();
    format!("{s}</div>")
}

/// The details of an action on several files: each file and how it went.
fn details_table(details: &str) -> String {
    let rows = details
        .lines()
        .filter_map(|l| {
            let mut p = l.splitn(3, '\t');
            let (file, id, text) = (p.next()?, p.next()?, p.next().unwrap_or(""));
            let result = if id.is_empty() {
                qsweb::partials::status("Done", Some(true))
            } else {
                format!("{} {}", qsweb::partials::msgid(&crate::PRODUCT, id), esc(text))
            };
            Some(vec![Cell::mono(file), Cell::html(result)])
        })
        .collect();
    list_table(&List {
        cols: vec![Col::plain("Spooled file"), Col::plain("Result")],
        rows,
        empty: String::new(),
        keep: Vec::new(),
    })
}

/// The ids a list post selected.
pub fn selected(rq: &Rq<'_>) -> Vec<String> {
    rq.form()
        .pairs()
        .iter()
        .filter(|(k, _)| k.len() > 1 && k.starts_with('s') && k[1..].bytes().all(|b| b.is_ascii_digit()))
        .map(|(_, v)| v.clone())
        .collect()
}

/// A summary of `n` actions with `failed` refused, the desktop's way.
pub fn summary(rq: &Rq<'_>, failed: usize, n: usize) -> qsweb::msg::Message {
    rq.message(Kind::Error, msgs::SPLF_SUMMARY, &[&failed.to_string(), &n.to_string()])
}

fn strs(v: &[String]) -> Vec<&str> {
    v.iter().map(String::as_str).collect()
}
