// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Files (handoff steps 13-14): the pages against their goldens, and
//! what the operations, transfers and the editor do over the fixture.
//! Golden corrections are listed in docs/ACCESSWEB.md.

mod common;

use std::io::Read;

use accessweb::env::Audit;
use accessweb::env::files::Files;
use accessweb::env::policy::{PolicyStore, Setting};
use accessweb::fixture::files::{Data, HOME};
use accessweb::policy;
use common::{Browser, HEADER_USER, assert_golden, golden, proto, relative};
use qsweb::golden::{Normalize, diff};
use qsweb::http::{Body, Request, Response};

/// Compare with a golden text already corrected.
fn assert_text(served: &str, g: &str, file: &str) {
    let dir = file.split('/').next().unwrap_or("");
    let map = |u: &str| proto(u).map(|t| relative(&t, dir));
    if let Some(d) = diff(served, g, &Normalize { url: &map }) {
        panic!("{file} differs from the served page at {d}");
    }
}

fn fix(g: &mut String, from: &str, to: &str) {
    assert!(g.contains(from), "golden fix not applicable: {from}");
    *g = g.replace(from, to);
}

/// The body of a response, a stream read out.
fn body(r: Response) -> Vec<u8> {
    match r.body {
        Body::Bytes(b) => b,
        Body::Sized(_, mut rd) | Body::Stream(mut rd) => {
            let mut v = Vec::new();
            rd.read_to_end(&mut v).unwrap();
            v
        }
        Body::Upgrade(_) => panic!("a Files page answered with a WebSocket upgrade"),
    }
}

/// `web/files.html` as the fixture serves it (docs/ACCESSWEB.md).
fn files_golden() -> String {
    let mut g = golden("web/files.html").replace("\r\n", "\n");
    fix(&mut g, HEADER_USER.0, HEADER_USER.1);
    // The path line is a form of its own, its buttons named.
    fix(&mut g, "<p class=\"small\">Path:", "<form method=\"post\"><p class=\"small\">Path:");
    fix(&mut g, "old2.csv)</span></p>", "old2.csv)</span></p></form>");
    fix(&mut g, "<input type=\"submit\" value=\"Go\"> &nbsp; <input type=\"submit\" value=\"Paste\">", "<input type=\"submit\" name=\"go\" value=\"Go\"> &nbsp; <input type=\"submit\" name=\"paste\" value=\"Paste\">");
    // `..` is not selected, renamed, cut, copied or deleted.
    fix(
        &mut g,
        "<td><input type=\"checkbox\" name=\"s\"></td>\n<td class=\"act\"><a href=\"files.html\">Open</a> | <a href=\"files.html\">Rename</a> | <a href=\"files.html\">Cut</a> | <a href=\"files.html\">Copy</a> | <a href=\"../shared/confirm.html\">Delete</a> | <a href=\"file-props.html\">Properties</a></td>\n<td class=\"mono\"><a href=\"files.html\">..</a></td>",
        "<td></td>\n<td class=\"act\"><a href=\"files.html\">Open</a> | <a href=\"file-props.html\">Properties</a></td>\n<td class=\"mono\"><a href=\"files.html\">..</a></td>",
    );
    // A checkbox says which entry it selects.
    let mut out = String::new();
    let mut rest = g.as_str();
    let cb = "<input type=\"checkbox\" name=\"s\">";
    while let Some(at) = rest.find(cb) {
        out.push_str(&rest[..at]);
        let after = &rest[at + cb.len()..];
        let m = after.find("<td class=\"mono\"><a href=\"").unwrap();
        let s = &after[m + "<td class=\"mono\"><a href=\"".len()..];
        let name = &s[s.find("\">").unwrap() + 2..s.find("</a>").unwrap()];
        out.push_str("<input type=\"checkbox\" name=\"s\" value=\"");
        out.push_str(name);
        out.push_str("\">");
        rest = after;
    }
    out.push_str(rest);
    g = out;
    // Sorted by name: old-link before reports.
    g = swap_rows(&g, "reports", "old-link");
    // invload.clle is the text the editor's golden shows: its size.
    fix(&mut g, "<td class=\"num\">1,844</td>", "<td class=\"num\">2,033</td>");
    // The selection's buttons are named.
    fix(
        &mut g,
        "Selected: <input type=\"submit\" value=\"Download\"><input type=\"submit\" value=\"Cut\"><input type=\"submit\" value=\"Copy\"><input type=\"submit\" value=\"Delete...\"> &nbsp; <input type=\"submit\" value=\"New folder...\">",
        "Selected: <input type=\"submit\" name=\"download\" value=\"Download\"><input type=\"submit\" name=\"cut\" value=\"Cut\"><input type=\"submit\" name=\"copy\" value=\"Copy\"><input type=\"submit\" name=\"delete\" value=\"Delete...\"> &nbsp; <input type=\"submit\" name=\"mkdir\" value=\"New folder...\">",
    );
    // Message IDs are links.
    fix(&mut g, "Failed - QAD8021 not authorized", "Failed - <a class=\"msgid\" href=\"../shared/help.html\" target=\"_blank\">QAD8021</a> not authorized");
    // Data: the user's preference "Transfer files as" is Text.
    fix(&mut g, "<input type=\"radio\" name=\"mode\" value=\"bin\" checked> Binary <input type=\"radio\" name=\"mode\" value=\"text\">", "<input type=\"radio\" name=\"mode\" value=\"bin\"> Binary <input type=\"radio\" name=\"mode\" value=\"text\" checked>");
    fix(&mut g, "<input type=\"radio\" name=\"dm\" value=\"bin\" checked> Binary <input type=\"radio\" name=\"dm\" value=\"text\">", "<input type=\"radio\" name=\"dm\" value=\"bin\"> Binary <input type=\"radio\" name=\"dm\" value=\"text\" checked>");
    g
}

#[test]
fn files_page() {
    let mut b = Browser::signed_on();
    // Two entries cut in upload/ wait to be pasted.
    let r = b.post("/files?path=/home/CWELLMAN/upload", &[("s", "old.csv"), ("s", "old2.csv"), ("cut", "Cut")]);
    assert_eq!(r.status, 303, "{}", r.text());
    let r = b.get("/files?path=/home/CWELLMAN");
    assert_eq!(r.status, 200);
    assert_text(&r.text(), &files_golden(), "web/files.html");
}

#[test]
fn file_props_page() {
    let mut b = Browser::signed_on();
    let r = b.get("/files/props?path=/home/CWELLMAN/assets_br1.csv");
    assert_eq!(r.status, 200);
    assert_golden(&r.text(), "web/file-props.html", &[HEADER_USER]);
}

#[test]
fn my_files_page() {
    let mut b = Browser::signed_on();
    let r = b.get("/files/shares");
    assert_eq!(r.status, 200);
    assert_golden(&r.text(), "web/my-files.html", &[HEADER_USER]);
}

#[test]
fn file_edit_page() {
    let mut b = Browser::signed_on();
    let r = b.get("/files/edit?path=/home/CWELLMAN/invload.clle");
    assert_eq!(r.status, 200);
    assert_golden(
        &r.text(),
        "web/file-edit.html",
        &[
            HEADER_USER,
            // The editor is part of Browse files, as Properties is.
            (
                "<div class=\"navgrp\"><a class=\"glyph\" href=\"files.html\">[+]</a> <b>Files</b></div>",
                "<div class=\"navgrp\"><a class=\"glyph\" href=\"files.html\">[-]</a> <b>Files</b></div>\n<div class=\"navitem\"><a href=\"files.html\" class=\"navcur\">Browse files</a></div>\n<div class=\"navitem\"><a href=\"my-files.html\">My files</a></div>",
            ),
            // The file's real size.
            ("1,844 bytes", "2,033 bytes"),
            // Four nameless buttons in one form cannot be told apart.
            (
                "<input type=\"submit\" value=\"Save\"><input type=\"submit\" value=\"Save as...\"><input type=\"submit\" value=\"Reset\"><input type=\"submit\" value=\"Cancel\">",
                "<input type=\"submit\" name=\"save\" value=\"Save\"><input type=\"submit\" name=\"saveas\" value=\"Save as...\"><input type=\"submit\" name=\"reset\" value=\"Reset\"><input type=\"submit\" name=\"cancel\" value=\"Cancel\">",
            ),
        ],
    );
}

fn text_of(b: &Browser, path: &str) -> String {
    let t = b.fx.files.tree.lock().unwrap();
    match &t.get(path).unwrap().data {
        Data::File(d) => String::from_utf8_lossy(d).into_owned(),
        d => panic!("{d:?}"),
    }
}

#[test]
fn folders_rename_copy_paste_delete() {
    let mut b = Browser::signed_on();
    // New folder.
    let r = b.post("/files/mkdir?path=/home/CWELLMAN", &[("name", "archive"), ("ok", "OK")]);
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/files?path=/home/CWELLMAN"), "{}", r.text());
    assert!(b.get("/files").text().contains("QAD8002</a> - Folder /home/CWELLMAN/archive created."));
    let t = b.post("/files/mkdir?path=/home/CWELLMAN", &[("name", "a/b"), ("ok", "OK")]).text();
    assert!(t.contains("XWA0004"), "{t}");
    // Rename.
    b.post("/files/rename?path=/home/CWELLMAN/notes.txt", &[("name", "notes-old.txt"), ("ok", "OK")]);
    assert!(b.get("/files").text().contains("QAD8003</a> - /home/CWELLMAN/notes.txt renamed to /home/CWELLMAN/notes-old.txt."));
    // Copy two into archive, paste twice is refused the second time.
    b.post("/files", &[("s", "notes-old.txt"), ("s", ".profile"), ("copy", "Copy")]);
    let r = b.post("/files?path=/home/CWELLMAN/archive", &[("paste", "Paste")]);
    assert_eq!(r.status, 303);
    assert!(b.get("/files?path=/home/CWELLMAN/archive").text().contains("QAD8005</a> - 2 entries copied to /home/CWELLMAN/archive."));
    assert_eq!(text_of(&b, "/home/CWELLMAN/archive/.profile"), text_of(&b, "/home/CWELLMAN/.profile"));
    b.post("/files?path=/home/CWELLMAN/archive", &[("paste", "Paste")]);
    assert!(b.get("/files?path=/home/CWELLMAN/archive").text().contains("CPFA0A0"));
    // Cut moves, the link's token checked.
    let csrf = b.csrf.clone();
    assert_eq!(b.get("/files/cut?path=/home/CWELLMAN/archive/.profile&t=bad").status, 403);
    b.get(&format!("/files/cut?path=/home/CWELLMAN/archive/.profile&t={csrf}"));
    b.post("/files?path=/home/CWELLMAN/upload", &[("paste", "Paste")]);
    assert!(b.fx.files.tree.lock().unwrap().contains_key("/home/CWELLMAN/upload/.profile"));
    // Delete: the confirmation counts, OK deletes.
    let t = b.get("/files/confirm?path=/home/CWELLMAN&s=archive").text();
    assert!(t.contains("1 object in all") || t.contains("2 objects in all"), "{t}");
    let r = b.post("/files/confirm?path=/home/CWELLMAN&s=archive", &[("ok", "OK")]);
    assert_eq!(r.status, 303);
    assert!(!b.fx.files.tree.lock().unwrap().keys().any(|k| k.starts_with("/home/CWELLMAN/archive")));
    // The list's Delete... asks first.
    let r = b.post("/files", &[("s", "notes-old.txt"), ("delete", "Delete...")]);
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/files/confirm?path=/home/CWELLMAN&s=notes-old.txt"));
    // Nothing selected.
    assert_eq!(b.post("/files", &[("cut", "Cut")]).status, 303);
    assert!(b.get("/files").text().contains("XWA0041"));
}

fn multipart(path: &str, fields: &[(&str, &str)], files: &[(&str, &[u8])], csrf: &str) -> Request {
    let mut req = Request::new("POST", &accessweb::PRODUCT.url(path));
    let mut body = Vec::new();
    let mut part = |head: String, data: &[u8]| {
        body.extend_from_slice(b"--XyZ\r\n");
        body.extend_from_slice(head.as_bytes());
        body.extend_from_slice(b"\r\n\r\n");
        body.extend_from_slice(data);
        body.extend_from_slice(b"\r\n");
    };
    part("Content-Disposition: form-data; name=\"csrf\"".into(), csrf.as_bytes());
    for (k, v) in fields {
        part(format!("Content-Disposition: form-data; name=\"{k}\""), v.as_bytes());
    }
    for (name, data) in files {
        part(format!("Content-Disposition: form-data; name=\"f\"; filename=\"{name}\"\r\nContent-Type: application/octet-stream"), data);
    }
    body.extend_from_slice(b"--XyZ--\r\n");
    req.body = body;
    req.headers.push(("content-type".into(), "multipart/form-data; boundary=XyZ".into()));
    req
}

#[test]
fn upload_several_text_and_existing() {
    let mut b = Browser::signed_on();
    let csrf = b.csrf.clone();
    let req = multipart(
        "/files?path=/home/CWELLMAN/upload",
        &[("mode", "bin"), ("ex", "Ask"), ("dm", "bin"), ("dle", "LF")],
        &[("a.bin", b"\x00\x01\x02"), ("b.txt", b"one\r\ntwo\r\n")],
        &csrf,
    );
    let r = b.send(req);
    assert_eq!(r.status, 303);
    assert!(b.get("/files?path=/home/CWELLMAN/upload").text().contains("XWA0040</a> - File a.bin, b.txt uploaded."));
    assert_eq!(b.fx.files.entry("CWELLMAN", "/home/CWELLMAN/upload/a.bin").unwrap().ccsid, 65535);
    assert_eq!(b.fx.files.entry("CWELLMAN", "/home/CWELLMAN/upload/b.txt").unwrap().ccsid, 1208);
    // Text in CCSID 37 with LF on the system.
    let req = multipart("/files?path=/home/CWELLMAN/upload", &[("mode", "text"), ("ccsid", "37"), ("ex", "Replace")], &[("e.txt", b"AB\r\n")], &csrf);
    b.send(req);
    let t = b.fx.files.tree.lock().unwrap();
    assert_eq!(t["/home/CWELLMAN/upload/e.txt"].data, Data::File(vec![0xC1, 0xC2, 0x25]));
    drop(t);
    // Exists: Ask waits in the queue; Replace writes it.
    let req = multipart("/files?path=/home/CWELLMAN/upload", &[("mode", "bin"), ("ex", "Ask")], &[("b.txt", b"new")], &csrf);
    b.send(req);
    let page = b.get("/files?path=/home/CWELLMAN/upload").text();
    assert!(page.contains("QAD8010</a> - b.txt already exists in /home/CWELLMAN/upload."), "{page}");
    let id = b.fx.files.queue.list("CWELLMAN").last().unwrap().id;
    b.get(&format!("/files/queue?n={id}&op=replace&t={csrf}"));
    assert_eq!(text_of(&b, "/home/CWELLMAN/upload/b.txt"), "new");
    // A CCSID the system lacks: nothing written.
    let req = multipart("/files?path=/home/CWELLMAN/upload", &[("mode", "text"), ("ccsid", "9999")], &[("z.txt", b"x")], &csrf);
    b.send(req);
    assert!(b.get("/files?path=/home/CWELLMAN/upload").text().contains("XWA0004"));
    // The sample's failed upload: Retry writes it.
    let id = b.fx.files.queue.list("CWELLMAN").iter().find(|t| t.path.ends_with("backup.sh")).unwrap().id;
    b.get(&format!("/files/queue?n={id}&op=retry&t={csrf}"));
    assert!(text_of(&b, "/home/CWELLMAN/scripts/backup.sh").starts_with("#!/bin/sh"));
    assert!(b.get("/files").text().contains("XWA0040"));
    // Another user is refused in a directory it may not change.
    let mut w = Browser::as_user("WEBUSER");
    w.fx.files.mkdir("CWELLMAN", "/home/WEBUSER").unwrap();
    w.fx.files.tree.lock().unwrap().get_mut("/home/WEBUSER").unwrap().owner = "WEBUSER".into();
    w.fx.files.tree.lock().unwrap().get_mut(HOME).unwrap().mode = 0o705;
    let wc = w.csrf.clone();
    w.send(multipart("/files?path=/home/WEBUSER", &[("mode", "bin")], &[("ok.txt", b"x")], &wc));
    assert!(w.fx.files.entry("WEBUSER", "/home/WEBUSER/ok.txt").is_ok());
}

#[test]
fn download_ranges_and_queue() {
    let mut b = Browser::signed_on();
    // Binary: all of it, then the rest from 100,000 on.
    let r = b.get("/files/download?path=/home/CWELLMAN/inventory_2026-10.pdf");
    assert_eq!(r.status, 200);
    assert_eq!(r.header("Accept-Ranges"), Some("bytes"));
    let etag = r.header("ETag").unwrap().to_string();
    let all = body(r);
    assert_eq!(all.len(), 240_112);
    let mut req = Request::new("GET", &accessweb::PRODUCT.url("/files/download?path=/home/CWELLMAN/inventory_2026-10.pdf"));
    req.headers.push(("range".into(), "bytes=100000-".into()));
    req.headers.push(("if-range".into(), etag));
    let r = b.send(req);
    assert_eq!((r.status, r.header("Content-Range")), (206, Some("bytes 100000-240111/240112")));
    assert_eq!(body(r), all[100_000..]);
    let q = b.fx.files.queue.list("CWELLMAN");
    let last = q.iter().rev().find(|t| t.path.contains("inventory")).unwrap();
    assert_eq!(last.state, accessweb::transfer::State::Complete);
    // A download left part way says so and offers Retry.
    let r = b.get("/files/download?path=/home/CWELLMAN/webstore.p12");
    drop(r);
    let page = b.get("/files").text();
    assert!(page.contains("the connection ended"), "{page}");
    assert!(page.contains("op=retry"));
    // Text: converted to UTF-8 with CR LF (the preference).
    let r = b.get("/files/download?path=/home/CWELLMAN/invload.clle");
    let t = String::from_utf8(body(r)).unwrap();
    assert!(t.starts_with("/* INVLOAD") && t.contains("\r\n"));
    // Open: inline, sandboxed.
    let r = b.get("/files/open?path=/home/CWELLMAN/notes.txt");
    assert_eq!(r.header("Content-Security-Policy"), Some("sandbox"));
    assert!(r.header("Content-Disposition").unwrap().starts_with("inline"));
    // Several selected wait in the queue.
    b.post("/files", &[("s", "notes.txt"), ("s", ".profile"), ("download", "Download")]);
    assert!(b.get("/files").text().contains("op=fetch"));
}

#[test]
fn editor_saves_in_the_file_ccsid_and_asks_when_changed() {
    let mut b = Browser::signed_on();
    // A CCSID 37 file with CR LF.
    b.fx.files.write("CWELLMAN", "/home/CWELLMAN/e37.txt", &[0xC1, 0x0D, 0x25, 0xC2], 37).unwrap();
    let t = b.get("/files/edit?path=/home/CWELLMAN/e37.txt").text();
    assert!(t.contains("CCSID 37 &nbsp; Line ends: CR LF"), "{t}");
    let r = b.post("/files/edit?path=/home/CWELLMAN/e37.txt", &[("t", "X\r\nY"), ("save", "Save")]);
    assert_eq!(r.status, 303, "{}", r.text());
    assert_eq!(b.fx.files.read("CWELLMAN", "/home/CWELLMAN/e37.txt", 0, 99).unwrap(), vec![0xE7, 0x0D, 0x25, 0xE8]);
    // Changed by someone else since it was opened.
    b.get("/files/edit?path=/home/CWELLMAN/notes.txt");
    b.fx.files.write("CWELLMAN", "/home/CWELLMAN/notes.txt", b"theirs", 1208).unwrap();
    let t = b.post("/files/edit?path=/home/CWELLMAN/notes.txt", &[("t", "mine"), ("save", "Save")]).text();
    assert!(t.contains("QAD8011") && t.contains("name=\"overwrite\""), "{t}");
    b.post("/files/edit?path=/home/CWELLMAN/notes.txt", &[("t", "mine"), ("overwrite", "Overwrite")]);
    assert_eq!(text_of(&b, "/home/CWELLMAN/notes.txt"), "mine");
    // Save as another name.
    b.post("/files/edit?path=/home/CWELLMAN/notes.txt", &[("t", "copy"), ("to", "notes2.txt"), ("saveas_ok", "OK")]);
    assert_eq!(text_of(&b, "/home/CWELLMAN/notes2.txt"), "copy");
    // Binary opens read-only.
    let t = b.get("/files/edit?path=/home/CWELLMAN/webstore.p12").text();
    assert!(t.contains("QAD8013") && t.contains("readonly"), "{t}");
    let r = b.post("/files/edit?path=/home/CWELLMAN/webstore.p12", &[("t", "x"), ("save", "Save")]);
    assert_eq!(r.status, 303);
    assert_eq!(b.fx.files.entry("CWELLMAN", "/home/CWELLMAN/webstore.p12").unwrap().size, 3402);
}

#[test]
fn properties_change_authority() {
    let mut b = Browser::signed_on();
    let r = b.post("/files/props?path=/home/CWELLMAN/notes.txt", &[("auth", "rw-r-----"), ("pub", "*R"), ("autl", "*NONE"), ("ok", "OK")]);
    assert_eq!(r.status, 303);
    assert_eq!(b.fx.files.entry("CWELLMAN", "/home/CWELLMAN/notes.txt").unwrap().mode, 0o644);
    let t = b.post("/files/props?path=/home/CWELLMAN/notes.txt", &[("auth", "rwz"), ("pub", "*R"), ("autl", "*NONE"), ("ok", "OK")]).text();
    assert!(t.contains("XWA0004"), "{t}");
}

#[test]
fn policies_gate_files() {
    let mut b = Browser::signed_on();
    b.fx.policy.put("ADMINS", &[(policy::FILES_DELETE.into(), Setting::Deny), (policy::FILES_EDIT.into(), Setting::Deny)]).unwrap();
    let t = b.get("/files").text();
    assert!(!t.contains("Delete...") && !t.contains("/files/edit"), "{t}");
    let r = b.post("/files/confirm?path=/home/CWELLMAN&s=notes.txt", &[("ok", "OK")]);
    assert_eq!(r.status, 403);
    assert!(r.text().contains("XWA0001"));
    assert!(b.fx.files.entry("CWELLMAN", "/home/CWELLMAN/notes.txt").is_ok());
    assert!(b.fx.system.audit.lock().unwrap().iter().any(|a| matches!(a, Audit::Refused { need, .. } if need == "policy files.delete")));
    // Outside the home directory.
    b.fx.policy.put("CWELLMAN", &[(policy::FILES_OUTSIDE.into(), Setting::Deny)]).unwrap();
    assert_eq!(b.get("/files?path=/www").status, 403);
    assert!(!b.get("/files/shares").text().contains("/www/webstore"));
}

/// The cells of the rows of entries `a` and `b` swapped, each row's
/// stripe kept.
#[allow(clippy::many_single_char_names)]
fn swap_rows(g: &str, a: &str, b: &str) -> String {
    let cells = |name: &str| {
        let at = g.find(&format!("value=\"{name}\">")).unwrap();
        let start = g[..at].rfind("<td>").unwrap();
        let end = at + g[at..].find("</tr>").unwrap();
        (start, end)
    };
    let (mut x, mut y) = (cells(a), cells(b));
    if x.0 > y.0 {
        std::mem::swap(&mut x, &mut y);
    }
    format!("{}{}{}{}{}", &g[..x.0], &g[y.0..y.1], &g[x.1..y.0], &g[x.0..x.1], &g[y.1..])
}
