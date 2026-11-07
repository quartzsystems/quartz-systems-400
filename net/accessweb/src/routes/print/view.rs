// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/print/view/<id>?p=&f=`: the spooled file viewer, Navigator's page
//! (`nav/spooled-view.html`) framed for this product, so it works as the
//! signed-on user whether or not Navigator is installed: one page in
//! Courier, column-exact, its navigation and Find; Hold, Release,
//! Delete. `/pdf` and `/text` give the file as PDF (work/spool's writer)
//! and as text, opened in the browser or, with `?save=1`, saved.

use qsweb::frame::Page;
use qsweb::html::{esc, query};
use qsweb::http::Response;
use qsweb::msg::Kind;

use super::{GROUP, OUTPUT, find, view_route};
use crate::env::print::Splf;
use crate::msgs;
use crate::policy;
use crate::routes::Rq;
use crate::routes::files::tokened;

/// Device types the viewer can read: SCS and line data.
fn readable(f: &Splf) -> bool {
    matches!(f.device_type.as_str(), "*SCS" | "*LINE" | "")
}

/// Route `/print/view/<id>/...`.
pub fn route(rq: &Rq<'_>, id: &str, rest: &[&str]) -> Response {
    let f = match find(rq, id) {
        Ok(f) => f,
        Err(r) => return r,
    };
    let crumb = [GROUP, "Printer output", f.file.as_str()];
    let (function, what) = match rest {
        ["pdf"] => (policy::PRINT_PDF, "PDF"),
        _ => (policy::PRINT_TEXT, "text"),
    };
    if let Err(r) = rq.policy(function, &crumb) {
        return r;
    }
    if f.status == "OPN" {
        return rq.done(OUTPUT, rq.message(Kind::Error, msgs::SPLF_OPEN, &[&f.file]));
    }
    if !readable(&f) {
        return rq.done(OUTPUT, rq.message(Kind::Error, msgs::SPLF_FORMAT, &[&f.file, what]));
    }
    let save = rq.query().str("save") == "1";
    let name = format!("{}-{}", f.file, f.number);
    let disposition = |ext: &str| format!("{}; filename=\"{name}.{ext}\"", if save { "attachment" } else { "inline" });
    let failed = |m: &qsweb::msg::Message| rq.done(OUTPUT, crate::routes::files::error(rq, m));
    match rest {
        [] => match rq.env().print.text(rq.user(), &f) {
            Ok(pages) => view(rq, &f, &pages),
            Err(m) => failed(&m),
        },
        ["pdf"] => match rq.env().print.pdf(rq.user(), &f) {
            Ok(pdf) => Response::new(200, "application/pdf", pdf)
                .with("Content-Disposition", &disposition("pdf"))
                .with("X-Content-Type-Options", "nosniff"),
            Err(m) => failed(&m),
        },
        ["text"] => match rq.env().print.text(rq.user(), &f) {
            // Pages apart by a form feed, as a text printer takes them.
            Ok(pages) => Response::new(200, "text/plain; charset=utf-8", pages.join("\u{c}").into_bytes())
                .with("Content-Disposition", &disposition("txt"))
                .with("X-Content-Type-Options", "nosniff"),
            Err(m) => failed(&m),
        },
        _ => rq.not_found(&format!("page {} of spooled file {}", rest.join("/"), f.file)),
    }
}

/// One page, its navigation and Find.
fn view(rq: &Rq<'_>, f: &Splf, pages: &[String]) -> Response {
    let q = rq.query();
    let total = pages.len().max(1);
    let mut at: usize = q.str("p").trim().parse().unwrap_or(1).clamp(1, total);
    let find_text = q.str("f").to_string();
    let mut messages = Vec::new();
    if !find_text.is_empty() {
        // From the page shown on, the first page holding the text.
        let needle = find_text.to_ascii_uppercase();
        match (at..=total).chain(1..at).find(|p| pages[p - 1].to_ascii_uppercase().contains(&needle)) {
            Some(p) => at = p,
            None => messages.push(rq.message(Kind::Info, "CPI9801", &[&find_text])),
        }
    }
    let base = rq.ctx.url(&view_route(f));
    let nav = |p: usize, label: &str, on: bool| {
        if on {
            format!("<a href=\"{}?p={p}\">{}</a>", esc(&base), esc(label))
        } else {
            format!("<span class=\"grey\">{}</span>", esc(label))
        }
    };
    let head = format!(
        "<p class=\"small\">File: <b class=\"mono\">{}</b> &nbsp; Job: <span class=\"mono\">{}</span> &nbsp; File number: {} &nbsp; Pages: {} &nbsp; {} columns &nbsp; <a href=\"{}\">Back to Printer output</a></p>",
        esc(&f.file),
        esc(&f.job),
        f.number,
        f.pages,
        f.page_width,
        esc(&rq.ctx.url(OUTPUT)),
    );
    let mut tools = Vec::new();
    if rq.allowed(policy::PRINT_PDF) {
        tools.push(format!("<a href=\"{}\">View as PDF</a>", esc(&format!("{base}/pdf"))));
    }
    tools.push(format!("<a href=\"{}\">Download text</a>", esc(&format!("{base}/text?save=1"))));
    if rq.allowed(policy::PRINT_OWN) {
        let id = f.id();
        for (op, label) in [("hold", "Hold"), ("release", "Release")] {
            let u = rq.ctx.url(&tokened(rq, "/print/act", &[("op", op), ("s", &id)]));
            tools.push(format!("<a href=\"{}\">{label}</a>", esc(&u)));
        }
        let u = rq.ctx.url(&format!("/print/confirm{}", query(&[("s", &id)])));
        tools.push(format!("<a href=\"{}\">Delete</a>", esc(&u)));
    }
    let toolbar = format!(
        "<form method=\"get\"><div class=\"toolbar\">Page: <input type=\"text\" name=\"p\" value=\"{at}\" size=\"4\"> of {total} <input type=\"submit\" value=\"Go\"> &nbsp; {} &nbsp; {} &nbsp;&nbsp; Find: <input type=\"text\" name=\"f\" value=\"{}\" size=\"16\"> <input type=\"submit\" value=\"Find\"> &nbsp;&nbsp; {}</div></form>",
        nav(at.saturating_sub(1), "< Previous", at > 1),
        nav(at + 1, "Next >", at < total),
        esc(&find_text),
        tools.join(" &nbsp; "),
    );
    let text = pages.get(at - 1).map_or("", String::as_str);
    let body = format!("{head}{toolbar}<pre class=\"out out132\">{}</pre>", esc(text));
    let h1 = format!("Spooled file: {}", f.file);
    rq.page(Page {
        title: h1.clone(),
        crumb: vec![GROUP.into(), "Printer output".into(), f.file.clone()],
        h1,
        messages,
        body,
        help: "splf-view".into(),
        nav: Some((GROUP, Some(OUTPUT))),
        ..Page::default()
    })
}
