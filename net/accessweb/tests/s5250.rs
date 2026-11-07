// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The 5250 session's behaviour over the fixture: saved sessions and
//! their files, the display without script (keypad, toolbar, Paste,
//! Print Screen), keyboard maps, macros, the WebSocket and the policies.

mod common;

use std::fmt::Write as _;
use std::io::{self, Read, Write};

use accessweb::env::policy::{PolicyStore, Setting};
use accessweb::env::session5250::{Input, Session5250};
use accessweb::fixture::USER;
use accessweb::policy;
use accessweb::s5250::files::qkm::KeymapFile;
use accessweb::s5250::files::qsd::SessionFile;
use common::Browser;
use qsweb::http::{Handler, Peer, Request};

fn inputs(b: &Browser) -> Vec<Input> {
    b.fx.s5250.inputs.lock().unwrap().iter().map(|(_, _, i)| i.clone()).collect()
}

/// A multipart POST of text fields and files (name, filename, data).
fn multipart(b: &mut Browser, route: &str, fields: &[(&str, &str)], files: &[(&str, &str, &str)]) -> qsweb::http::Response {
    let mut body = String::new();
    let tok = b.csrf.clone();
    for (k, v) in std::iter::once(("csrf", tok.as_str())).chain(fields.iter().copied()) {
        let _ = write!(body, "--XB\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n");
    }
    for (k, f, d) in files {
        let _ = write!(body, "--XB\r\nContent-Disposition: form-data; name=\"{k}\"; filename=\"{f}\"\r\nContent-Type: application/octet-stream\r\n\r\n{d}\r\n");
    }
    body.push_str("--XB--\r\n");
    let mut req = Request::new("POST", &accessweb::PRODUCT.url(route));
    req.headers.push(("content-type".into(), "multipart/form-data; boundary=XB".into()));
    req.body = body.into_bytes();
    b.send(req)
}

#[test]
#[allow(clippy::many_single_char_names)]
fn keypad_toolbar_and_paste_without_script() {
    let mut b = Browser::signed_on();
    let r = b.post("/5250/session/1", &[("k", "F3")]);
    assert_eq!((r.status, r.header("Location")), (303, Some("/qs400/accessweb/5250/session/1")));
    b.post("/5250/session/1", &[("k", "FldExit")]);
    b.post("/5250/session/1/key", &[("k", "Enter")]);
    assert_eq!(inputs(&b), [Input::Key("F:3".into()), Input::Key("FieldExit".into()), Input::Key("Enter".into())]);
    // Paste types at the cursor; the screen's text shows it.
    assert!(b.get("/5250/session/1/paste").text().contains("name=\"t\""));
    b.post("/5250/session/1/paste", &[("t", "WRKSPLF"), ("ok", "Paste")]);
    let t = b.get("/5250/session/1/screen.txt").text();
    assert!(t.lines().nth(23).unwrap().starts_with("F3=ExWRKSPLF"), "{t}");
    // Copy shows the screen's text.
    assert!(b.get("/5250/session/1/copy").text().contains("Work with Active Jobs"));
    // Disconnect: keys are refused with the desktop's message.
    b.post("/5250/session/1", &[("tb", "Disconnect")]);
    let t = b.get("/5250/session/1").text();
    assert!(t.contains("QAD1006") && t.contains("value=\"Connect\">") && t.contains("value=\"F1\" style=\"width:54px;height:22px;font-size:8pt;padding:0\" disabled"), "{t}");
    b.post("/5250/session/1", &[("tb", "Connect")]);
    assert!(b.get("/5250/session/1").text().contains("QSHQ400: Connected"));
    // The keypad hides and shows.
    b.post("/5250/session/1", &[("tb", "Keypad")]);
    assert!(!b.get("/5250/session/1").text().contains("class=\"keypad\""));
}

#[test]
fn pictures_and_print_screen() {
    let mut b = Browser::signed_on();
    let png = b.get("/5250/session/1/screen.png");
    assert_eq!(png.header("Content-Type"), Some("image/png"));
    assert_eq!(png.bytes(), common_png("5250-24x80.png"));
    assert_eq!(b.get("/5250/session/1/screen.png?scale=2").bytes(), common_png("5250-24x80-x2.png"));
    let pdf = b.get("/5250/session/1/print");
    assert_eq!(pdf.header("Content-Type"), Some("application/pdf"));
    assert!(pdf.bytes().starts_with(b"%PDF-1.4"));
    // Print screen denied by policy.
    b.fx.policy.put("ADMINS", &[(policy::S5250_PRINT.into(), Setting::Deny)]).unwrap();
    let r = b.get("/5250/session/1/print");
    assert_eq!(r.status, 403);
    assert!(r.text().contains("XWA0001"));
    assert!(b.get("/5250/session/1").text().contains("value=\"Print Screen\" disabled"));
}

fn common_png(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/tests/golden/shared/img/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

#[test]
#[allow(clippy::many_single_char_names)]
fn saved_sessions_and_their_files() {
    let mut b = Browser::signed_on();
    // Save the form's session as a new one.
    let r = multipart(
        &mut b,
        "/5250/session/new",
        &[("n", "Night shift"), ("sys", "QSBR1400"), ("sz", "27x132"), ("cp", "285 - United Kingdom"), ("dev", "NIGHT01"), ("col", "bw"), ("ac", "on"), ("h1", "on"), ("km", "CWELLMAN-laptop (.qkm)"), ("save", "Save session")],
        &[],
    );
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/5250"), "{}", r.text());
    let t = b.get("/5250").text();
    assert!(t.contains("XWA0010</a> - Session Night shift saved.") && t.contains("<td>Night shift</td>"), "{t}");
    let s = b.fx.s5250.sessions(USER).into_iter().find(|s| s.file.session.title == "Night shift").unwrap();
    assert_eq!((s.file.session.ccsid, s.file.session.device_name.as_str(), s.file.system.name.as_str()), (285, "NIGHT01", "QSBR1400"));
    assert_eq!(s.file.keymap().function("Ctrl+Enter"), Some("Enter"));
    // Download it: the desktop's file, which reads back the same.
    let d = b.get(&format!("/5250/{}/download", s.id));
    assert!(d.header("Content-Disposition").unwrap().contains("Night_shift.qsd"));
    let back = SessionFile::parse(&d.text()).unwrap();
    assert_eq!(back, s.file);
    // Upload a desktop file.
    let mut f = SessionFile::new("QSHQ400");
    f.session.title = "From the desktop".into();
    let r = multipart(&mut b, "/5250/session/new", &[("start", "Start session")], &[("qsd", "desk.qsd", &f.to_text())]);
    assert_eq!(r.status, 303);
    assert!(b.get("/5250").text().contains("<td>From the desktop</td>"));
    // A newer format is refused, nothing kept.
    let r = multipart(&mut b, "/5250/session/new", &[("start", "Start session")], &[("qsd", "new.qsd", "[qsaccess]\nkind = \"display\"\nversion = 2\n")]);
    assert!(r.text().contains("QAD2002</a> - Session file new.qsd was made by a newer version (2)."), "{}", r.text());
    let r = multipart(&mut b, "/5250/session/new", &[("start", "Start session")], &[("qsd", "bad.qsd", "[qsaccess]\nkind = \"display\"\nversion = 1\n[system]\nname = \"a\"\n[session]\nccsid = 9\n")]);
    assert!(r.text().contains("QAD2001") && r.text().contains("CCSID 9 is not supported"), "{}", r.text());
    // Start without saving: kept for the sign-on only, number 0.
    let r = multipart(&mut b, "/5250/session/new", &[("n", "Scratch"), ("sys", "QSHQ400 (this system)"), ("sz", "24x80"), ("start", "Start session")], &[]);
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/5250/session/0?connect=1"));
    assert!(b.get("/5250/session/0?connect=1").text().contains("5250 display: Scratch"));
    assert!(!b.get("/5250").text().contains("<td>Scratch</td>"));
    // Delete asks first.
    let id = s.id;
    assert!(b.get(&format!("/5250/{id}/delete/confirm")).text().contains("Delete this saved session?"));
    let r = b.post(&format!("/5250/{id}/delete/confirm"), &[("ok", "OK")]);
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/5250"));
    assert!(b.get("/5250").text().contains("XWA0011"));
    assert!(b.fx.s5250.sessions(USER).iter().all(|x| x.id != id));
}

#[test]
fn properties_change_and_refuse() {
    let mut b = Browser::signed_on();
    let r = b.post(
        "/5250/1/properties",
        &[("t", "QSHQ400 operator"), ("sys", "QSHQ400"), ("sz", "24x80"), ("cp", "37 - USA, Canada"), ("dev", "QPADEV0009"), ("tls", "on"), ("col", "console"), ("kp", "on"), ("ok", "OK")],
    );
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/5250"));
    let s = b.fx.s5250.sessions(USER).into_iter().find(|s| s.id == 1).unwrap();
    assert_eq!(s.file.session.device_name, "QPADEV0009");
    assert!(!s.file.session.auto_connect && s.file.session.keypad_visible);
    let t = b.post("/5250/1/properties", &[("t", "x"), ("dev", "9BAD"), ("ok", "OK")]).text();
    assert!(t.contains("XWA0004</a> - Value 9BAD is not valid for Device name."), "{t}");
    // Verify a system from the Start form.
    let r = multipart(&mut b, "/5250/verify", &[("sys", "QSBR1400"), ("verify", "Verify connection...")], &[]);
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/5250/verify?sys=QSBR1400"));
    let t = b.post("/5250/verify?sys=QSBR1400", &[("ts", "on"), ("verify", "Verify")]).text();
    assert!(t.contains("Verify connection: QSBR1400") && t.contains("Signed on as CWELLMAN and off again"), "{t}");
}

#[test]
fn keyboard_maps() {
    let mut b = Browser::signed_on();
    // Enter (row m0) becomes Field Exit.
    let r = b.post("/5250/1/keymap", &[("m0", "Field Exit"), ("ok", "OK")]);
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/5250/session/1"));
    let s = b.fx.s5250.sessions(USER).into_iter().find(|s| s.id == 1).unwrap();
    assert_eq!(s.file.keymap().function("Enter"), Some("FieldExit"));
    assert!(s.file.to_text().contains("[keymap.overrides]\nEnter = \"FieldExit\""), "{}", s.file.to_text());
    // Add a key; export; the script's view of it.
    b.post("/5250/1/keymap", &[("key", "Ctrl+Enter"), ("fn", "Enter"), ("add", "Add")]);
    let f = KeymapFile::parse(&b.get("/5250/1/keymap/export").text()).unwrap();
    assert_eq!(f.map.function("Ctrl+Enter"), Some("Enter"));
    assert!(b.get("/5250/1/keymap/json").text().contains("\"Ctrl+Enter\":\"Enter\""));
    // Load the desktop's file.
    let qkm = include_str!("../src/s5250/files/testdata/operations.qkm");
    let r = multipart(&mut b, "/5250/1/keymap", &[("load", "Load")], &[("qkm", "operations.qkm", qkm)]);
    assert_eq!(r.status, 303);
    assert!(b.get("/5250/1/keymap").text().contains("QAD2010</a> - Keyboard map loaded from operations.qkm."));
    assert!(b.fx.s5250.keymaps(USER).iter().any(|(n, _)| n == "operations"));
    b.post("/5250/1/keymap", &[("restore_defaults", "Restore defaults")]);
    let s = b.fx.s5250.sessions(USER).into_iter().find(|s| s.id == 1).unwrap();
    assert_eq!(s.file.keymap(), accessweb::s5250::files::keymap::KeyMap::defaults());
}

#[test]
#[allow(clippy::many_single_char_names)]
fn macros_record_edit_play() {
    let mut b = Browser::signed_on();
    // Record: typing, a key that goes to the system and its wait.
    let r = b.post("/5250/1/macros", &[("n", "wrksplf"), ("record", "Record...")]);
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/5250/session/1"));
    assert!(b.get("/5250/session/1").text().contains("Recording macro WRKSPLF"));
    b.post("/5250/session/1/paste", &[("t", "WRKSPLF"), ("ok", "Paste")]);
    b.post("/5250/session/1", &[("k", "Enter")]);
    let t = b.get("/5250/1/macros").text();
    assert!(t.contains("value=\"Stop\""), "{t}");
    b.post("/5250/1/macros", &[("stop", "Stop")]);
    let t = b.get("/5250/1/macros").text();
    assert!(t.contains("QAD2021</a> - Macro WRKSPLF recorded with 3 steps."), "{t}");
    // Edit SIGNON-DEV's steps; a bad line names itself.
    let t = b.post("/5250/1/macros?edit=1", &[("steps", "key   Warp\n"), ("save", "Save")]).text();
    assert!(t.contains("QAD2020") && t.contains("line 1"), "{t}");
    b.post("/5250/1/macros?edit=1", &[("steps", "type  \"CWELLMAN\"\nkey   Enter\n"), ("save", "Save")]);
    let s = b.fx.s5250.sessions(USER).into_iter().find(|s| s.id == 1).unwrap();
    assert_eq!(s.macros[0].file.steps.len(), 2);
    // Download the desktop's file; open one.
    let d = b.get("/5250/1/macros/1/download");
    assert!(accessweb::s5250::files::qsm::MacroFile::parse(&d.text()).is_ok());
    let qsm = include_str!("../src/s5250/files/testdata/qs400-signon-24x80.qsm");
    multipart(&mut b, "/5250/1/macros", &[("save", "Save")], &[("qsm", "signon.qsm", qsm)]);
    assert!(b.get("/5250/1/macros").text().contains("Empty sign-on"));
    // Play: the edited macro types and presses Enter.
    let before = inputs(&b).len();
    let r = b.get("/5250/session/1?play=SIGNON-DEV");
    assert_eq!(r.status, 303);
    assert_eq!(&inputs(&b)[before..], [Input::Text("CWELLMAN".into()), Input::Key("Enter".into())]);
    // A wait that never comes stops the macro, naming the step.
    let short = "[qsaccess]\nkind = \"macro\"\nversion = 1\n[macro]\nname = \"WAITS\"\n[[step]]\naction = \"wait-text\"\nrow = 1\ncol = 36\ntext = \"Sign On\"\ntimeout_ms = 200\n";
    multipart(&mut b, "/5250/1/macros", &[("save", "Save")], &[("qsm", "waits.qsm", short)]);
    b.get("/5250/session/1?play=WAITS");
    let t = b.get("/5250/session/1").text();
    assert!(t.contains("QAD2020</a> - Macro WAITS stopped at step 1:"), "{t}");
}

struct Pipe {
    input: io::Cursor<Vec<u8>>,
    output: Vec<u8>,
}

impl Read for Pipe {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        self.input.read(b)
    }
}

impl Write for Pipe {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.output.write(b)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn websocket_carries_screens_and_keys_but_no_hidden_field() {
    let b = Browser::signed_on();
    // A field that does not display holds a secret.
    b.fx.s5250.set_live(1, |l| {
        let c = &mut l.view.screen.cells[5 * 80 + 10];
        c.ch = 'Z';
        c.attr = 0x27;
    });
    let mut input = format!(
        "GET /qs400/accessweb/5250/ws/1 HTTP/1.1\r\nHost: x\r\nCookie: QS400SID={}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n",
        b.cookie.clone().unwrap()
    )
    .into_bytes();
    input.extend(qsweb::ws::client_frame("{\"t\":\"key\",\"k\":\"F:5\"}"));
    input.extend(qsweb::ws::client_frame("{\"t\":\"type\",\"s\":\"AB\"}"));
    let mut p = Pipe {
        input: io::Cursor::new(input),
        output: Vec::new(),
    };
    let peer = Peer {
        addr: "10.0.10.55:50000".parse().unwrap(),
        secure: false,
    };
    let h: &dyn Handler = &b.app;
    qsweb::http::serve(&mut p, &peer, h);
    let out = String::from_utf8_lossy(&p.output).into_owned();
    assert!(out.starts_with("HTTP/1.1 101 Switching Protocols\r\n") && out.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo="), "{out}");
    let body = &p.output[out.find("\r\n\r\n").unwrap() + 4..];
    let texts = qsweb::ws::server_texts(body);
    assert!(texts[0].starts_with("{\"t\":\"screen\"") && texts[0].contains("Work with Active Jobs"), "{texts:?}");
    assert!(!texts[0].contains("[5,\"          Z") && texts[0].contains("\"rows\":24"), "{}", texts[0]);
    let row5 = texts[0].split("[5,\"").nth(1).unwrap();
    assert_eq!(&row5[10..11], " ", "a nondisplay cell went out");
    assert_eq!(inputs(&b)[..2], [Input::Key("F:5".into()), Input::Text("AB".into())]);
    // A plain GET of the socket's address is refused.
    let mut b = b;
    assert_eq!(b.get("/5250/ws/1").status, 400);
}

#[test]
fn the_script_and_the_policies() {
    let mut b = Browser::signed_on();
    let js = b.get("/5250/script.js").text();
    assert!(js.starts_with("var QS_FONT = {32:[0,0,") && js.contains("var KEYPAD = {\"F1\":\"F:1\"") && js.contains("new WebSocket"));
    // Auto sign-on denied: the box is off and a start does not ask for a ticket.
    b.fx.policy.put("ADMINS", &[(policy::S5250_AUTOSIGNON.into(), Setting::Deny)]).unwrap();
    assert!(b.get("/5250").text().contains("<input type=\"checkbox\" name=\"aso\" disabled>"));
    // Start denied: nothing of the area opens.
    let mut w = Browser::as_user("WEBUSER");
    w.fx.policy.put("WEBUSERS", &[(policy::S5250_START.into(), Setting::Deny)]).unwrap();
    let r = w.get("/5250");
    assert_eq!(r.status, 403);
    assert!(r.text().contains("XWA0001"));
    assert_eq!(w.get("/5250/session/1").status, 404);
}
