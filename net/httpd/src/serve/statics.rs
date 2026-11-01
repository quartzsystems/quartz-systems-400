// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Static content: a file with its media type, `Last-Modified`,
//! `If-Modified-Since` (304) and one byte range (206, 416); a
//! directory's listing for `Options +Indexes`.

use std::fmt::Write as _;

use qsweb::http::{Request, Response};

use super::dirs::DirConf;
use super::time;
use crate::files::{Entry, Files};

/// Files larger than this go out as a stream (chunked) instead of a
/// counted body.
const STREAM_OVER: u64 = 8 * 1024 * 1024;

/// Media types by extension.
const TYPES: &[(&str, &str)] = &[
    ("html", "text/html"),
    ("htm", "text/html"),
    ("css", "text/css"),
    ("js", "text/javascript"),
    ("mjs", "text/javascript"),
    ("json", "application/json"),
    ("xml", "application/xml"),
    ("txt", "text/plain"),
    ("csv", "text/csv"),
    ("png", "image/png"),
    ("gif", "image/gif"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("svg", "image/svg+xml"),
    ("ico", "image/x-icon"),
    ("webp", "image/webp"),
    ("pdf", "application/pdf"),
    ("zip", "application/zip"),
    ("gz", "application/gzip"),
    ("tar", "application/x-tar"),
    ("jar", "application/java-archive"),
    ("wasm", "application/wasm"),
    ("woff", "font/woff"),
    ("woff2", "font/woff2"),
    ("mp3", "audio/mpeg"),
    ("mp4", "video/mp4"),
    ("savf", "application/octet-stream"),
];

/// The media type of `name`: `AddType`, then the table, then
/// `DefaultType` (`text/plain` when unset).
pub fn media_type(name: &str, d: &DirConf) -> String {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    if let Some((t, _)) = d.types.iter().rev().find(|(_, e)| *e == ext) {
        return t.clone();
    }
    let t = TYPES
        .iter()
        .find(|(e, _)| *e == ext)
        .map_or_else(|| d.default_type.clone().unwrap_or_else(|| "text/plain".into()), |(_, t)| (*t).to_string());
    if t.starts_with("text/") && !t.contains("charset") {
        format!("{t}; charset=utf-8")
    } else {
        t
    }
}

/// HTML-escape `s`.
pub fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '&' => o.push_str("&amp;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            c => o.push(c),
        }
    }
    o
}

/// Percent-encode a path for a link.
pub fn href(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"/-._~".contains(&b) {
            o.push(char::from(b));
        } else {
            let _ = write!(o, "%{b:02X}");
        }
    }
    o
}

/// One byte range of `len` bytes: `Ok(None)` for none asked, `Err` for
/// one that cannot be met.
fn range(req: &Request, len: u64) -> Result<Option<(u64, u64)>, ()> {
    let Some(asked) = req.header("range") else {
        return Ok(None);
    };
    let Some(spec) = asked.trim().strip_prefix("bytes=") else {
        return Ok(None);
    };
    if spec.contains(',') {
        // Several ranges: the whole file instead.
        return Ok(None);
    }
    let (from, to) = spec.split_once('-').ok_or(())?;
    let last = len.saturating_sub(1);
    let (start, end) = if from.is_empty() {
        let tail: u64 = to.parse().map_err(|_| ())?;
        (len.saturating_sub(tail), last)
    } else {
        let first: u64 = from.parse().map_err(|_| ())?;
        let end = if to.is_empty() { last } else { to.parse::<u64>().map_err(|_| ())?.min(last) };
        (first, end)
    };
    if len == 0 || start >= len || start > end {
        return Err(());
    }
    Ok(Some((start, end)))
}

/// The response for file `path` (its entry `e`).
pub fn file(files: &dyn Files, path: &str, e: &Entry, req: &Request, d: &DirConf) -> Response {
    let ctype = media_type(&e.name, d);
    let modified = time::http_date(e.modified);
    if e.modified > 0
        && let Some(since) = req.header("if-modified-since").and_then(time::parse_http_date)
        && e.modified <= since
    {
        let mut r = Response::new(304, &ctype, Vec::new());
        r.headers.clear();
        r.headers.push(("Last-Modified".into(), modified));
        return r;
    }
    let Ok(ranged) = range(req, e.size) else {
        return Response::new(416, "text/plain; charset=utf-8", Vec::new())
            .with("Content-Range", &format!("bytes */{}", e.size));
    };
    if ranged.is_none() && e.size > STREAM_OVER && req.method != "HEAD" {
        return match files.open(path) {
            Ok(r) => Response {
                status: 200,
                headers: vec![
                    ("Content-Type".into(), ctype),
                    ("Last-Modified".into(), modified),
                    ("Accept-Ranges".into(), "bytes".into()),
                ],
                body: qsweb::http::Body::Stream(r),
            },
            Err(_) => Response::new(500, "text/plain; charset=utf-8", Vec::new()),
        };
    }
    let Ok(data) = files.read(path) else {
        return Response::new(500, "text/plain; charset=utf-8", Vec::new());
    };
    let r = match ranged {
        Some((s, en)) => {
            let s = usize::try_from(s).unwrap_or(usize::MAX).min(data.len());
            let en = usize::try_from(en).unwrap_or(usize::MAX).min(data.len().saturating_sub(1));
            Response::new(206, &ctype, data[s..=en].to_vec())
                .with("Content-Range", &format!("bytes {s}-{en}/{}", data.len()))
        }
        None => Response::new(200, &ctype, data),
    };
    r.with("Last-Modified", &modified).with("Accept-Ranges", "bytes")
}

/// The listing of directory `url` (ending in `/`) with `entries`.
pub fn listing(url: &str, entries: &[Entry]) -> Response {
    let mut rows = String::new();
    if url != "/" {
        rows.push_str("<tr><td><a href=\"../\">Parent directory</a></td><td></td><td></td></tr>\n");
    }
    let mut sorted: Vec<&Entry> = entries.iter().filter(|e| !e.name.starts_with('.')).collect();
    sorted.sort_by(|a, b| b.dir.cmp(&a.dir).then(a.name.cmp(&b.name)));
    for e in sorted {
        let slash = if e.dir { "/" } else { "" };
        let size = if e.dir { "-".to_string() } else { e.size.to_string() };
        let _ = writeln!(
            rows,
            "<tr><td><a href=\"{}{slash}\">{}{slash}</a></td><td>{}</td><td>{size}</td></tr>",
            href(&e.name),
            esc(&e.name),
            if e.modified > 0 { time::http_date(e.modified) } else { String::new() },
        );
    }
    let t = esc(url);
    let page = format!(
        "<!DOCTYPE html>\n<html>\n<head><title>Contents of {t}</title></head>\n<body>\n<h1>Contents of {t}</h1>\n\
         <table>\n<tr><th>Name</th><th>Changed</th><th>Size</th></tr>\n{rows}</table>\n</body>\n</html>\n"
    );
    Response::new(200, "text/html; charset=utf-8", page.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_and_ranges() {
        let mut d = DirConf::default();
        assert_eq!(media_type("a.HTML", &d), "text/html; charset=utf-8");
        assert_eq!(media_type("a.png", &d), "image/png");
        assert_eq!(media_type("noext", &d), "text/plain; charset=utf-8");
        d.types.push(("application/x-thing".into(), "png".into()));
        d.default_type = Some("application/octet-stream".into());
        assert_eq!(media_type("a.png", &d), "application/x-thing");
        assert_eq!(media_type("a.zzz", &d), "application/octet-stream");
        let mut r = Request::new("GET", "/");
        r.headers.push(("range".into(), "bytes=2-4".into()));
        assert_eq!(range(&r, 10), Ok(Some((2, 4))));
        r.headers[0].1 = "bytes=-3".into();
        assert_eq!(range(&r, 10), Ok(Some((7, 9))));
        r.headers[0].1 = "bytes=20-".into();
        assert_eq!(range(&r, 10), Err(()));
        assert_eq!(href("a b&c.html"), "a%20b%26c.html");
        assert_eq!(esc("<a&>"), "&lt;a&amp;&gt;");
    }
}
