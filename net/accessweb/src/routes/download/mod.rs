// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Download (handoff step 21): the client pieces in
//! `/QQS/ProdData/Access/Web/download`, by section, each with its size;
//! `/download/<file>` sends one, read as the signed-on user.

use std::fmt::Write as _;

use qsweb::frame::Page;
use qsweb::html::esc;
use qsweb::http::Response;

use super::Rq;
use crate::PRODUCT;
use crate::env::download::{DIR, DlFile};

/// The data section, where Export a table as CSV goes.
const DATA: &str = "Data";

/// Route the area's first path segments.
pub fn route(rq: &Rq<'_>, segs: &[&str]) -> Response {
    match segs {
        ["download"] => page(rq),
        ["download", file] => send(rq, file),
        _ => rq.not_found(&rq.req.path),
    }
}

/// `4.2 MB`, `48 KB`.
pub fn size(bytes: u64) -> String {
    const MB: u64 = 1024 * 1024;
    if bytes >= MB {
        #[allow(clippy::cast_precision_loss)]
        let m = bytes as f64 / MB as f64;
        format!("{m:.1} MB")
    } else {
        format!("{} KB", bytes.div_ceil(1024).max(1))
    }
}

/// The content type of a file name.
fn content_type(file: &str) -> &'static str {
    let ext = file.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "txt" => "text/plain; charset=utf-8",
        "pdf" => "application/pdf",
        "pem" => "application/x-pem-file",
        "zip" => "application/zip",
        "gz" => "application/gzip",
        "ttf" => "font/ttf",
        _ => "application/octet-stream",
    }
}

/// `GET /download`.
fn page(rq: &Rq<'_>) -> Response {
    let files = rq.env().download.files(rq.user());
    let mut sections: Vec<(String, Vec<&DlFile>)> = Vec::new();
    for f in &files {
        match sections.iter_mut().find(|s| s.0 == f.section) {
            Some(s) => s.1.push(f),
            None => sections.push((f.section.clone(), vec![f])),
        }
    }
    if !sections.iter().any(|s| s.0 == DATA) {
        sections.push((DATA.into(), Vec::new()));
    }
    let mut body = format!(
        "<p>Client pieces for use with QS/400 Access for Web and 5250 sessions. All files are served from <span class=\"mono\">{DIR}</span> on this system.</p>"
    );
    for (name, list) in &sections {
        let mut links: Vec<String> = list
            .iter()
            .map(|f| {
                let mut s = format!(
                    "<a href=\"{}\">{}, {}</a>",
                    esc(&PRODUCT.url(&format!("/download/{}", qsweb::http::form::encode(&f.file)))),
                    esc(&f.label),
                    size(f.size)
                );
                if !f.note.is_empty() {
                    let _ = write!(s, " - {}", esc(&f.note));
                }
                s
            })
            .collect();
        if name == DATA {
            links.push(format!("<a href=\"{}\">Export a table as CSV</a>", esc(&PRODUCT.url("/db/tables"))));
        }
        let _ = write!(body, "<h2>{}</h2><div class=\"small\">{}</div>", esc(name), links.join("<br>"));
    }
    rq.page(Page {
        title: "Download".into(),
        crumb: vec!["Download".into()],
        h1: "Download".into(),
        body,
        help: "download".into(),
        nav: Some(("Download", Some("/download"))),
        ..Page::default()
    })
}

/// `GET /download/<file>`.
fn send(rq: &Rq<'_>, file: &str) -> Response {
    let file = qsweb::http::form::decode(file, false);
    if file.contains('/') || file.starts_with('.') {
        return rq.not_found(&format!("file {file}"));
    }
    match rq.env().download.read(rq.user(), &file) {
        Ok(data) => Response::download(&file, content_type(&file), data),
        Err(m) => rq.done("/download", m),
    }
}

#[cfg(test)]
mod tests {
    use super::size;

    #[test]
    fn sizes() {
        assert_eq!(size(4_404_019), "4.2 MB");
        assert_eq!(size(49_152), "48 KB");
        assert_eq!(size(612), "1 KB");
        assert_eq!(size(13_002_342), "12.4 MB");
    }
}
