// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/5250/session/<id>`: the 5250 display, laid out as the desktop's
//! window: menu bar (`?menu=`), toolbar, the screen (`screen.png`,
//! drawn by the server with the console's rules), the keypad and the
//! status bar. Without script the toolbar and keypad post, and Edit's
//! Copy and Paste are pages; with script the page's WebSocket
//! (`/5250/ws/<id>`) carries the screen and keys and the picture becomes
//! a canvas of the same size.

use std::fmt::Write as _;
use std::time::Duration;

use qsweb::frame::{Page, render};
use qsweb::html::{esc, query};
use qsweb::http::Response;
use qsweb::msg::{Kind, Message};
use qsweb::ws::{self, Socket};

use super::{GROUP, ITEM, Rq, saved, start_policy};
use crate::env::session5250::{Input, Live, Saved, State};
use crate::policy;
use crate::s5250::files::qsd::ColorMap;
use crate::s5250::files::qsm::{Action, MacroFile, Step};
use crate::s5250::look::pdf;
use crate::s5250::look::render::Scheme;
use crate::s5250::screen::{Screen, View};

/// The keypad: label and the 5250 function it sends.
pub const KEYPAD: &[(&str, &str)] = &[
    ("F1", "F:1"),
    ("F2", "F:2"),
    ("F3", "F:3"),
    ("F4", "F:4"),
    ("F5", "F:5"),
    ("F6", "F:6"),
    ("F7", "F:7"),
    ("F8", "F:8"),
    ("F9", "F:9"),
    ("F10", "F:10"),
    ("F11", "F:11"),
    ("F12", "F:12"),
    ("F13", "F:13"),
    ("F14", "F:14"),
    ("F15", "F:15"),
    ("F16", "F:16"),
    ("F17", "F:17"),
    ("F18", "F:18"),
    ("F19", "F:19"),
    ("F20", "F:20"),
    ("F21", "F:21"),
    ("F22", "F:22"),
    ("F23", "F:23"),
    ("F24", "F:24"),
    ("Enter", "Enter"),
    ("Reset", "Reset"),
    ("Attn", "Attention"),
    ("SysRq", "SysReq"),
    ("PgUp", "PageUp"),
    ("PgDn", "PageDown"),
    ("FldExit", "FieldExit"),
    ("Field+", "FieldPlus"),
    ("Field-", "FieldMinus"),
    ("Help", "Help"),
    ("Print", "Print"),
    ("Clear", "Clear"),
];

/// The functions that send the screen to the system.
pub fn is_aid(f: &str) -> bool {
    f.starts_with("F:")
        || matches!(
            f,
            "Enter" | "PageUp" | "PageDown" | "Attention" | "SysReq" | "Help" | "Print" | "Clear"
        )
}

/// The menus: key and name.
const MENUS: &[(&str, &str)] = &[
    ("file", "File"),
    ("edit", "Edit"),
    ("view", "View"),
    ("communication", "Communication"),
    ("actions", "Actions"),
];

const BUTTON: &str = "width:54px;height:22px;font-size:8pt;padding:0";

fn key(id: u32, what: &str) -> String {
    format!("5250.{what}.{id}")
}

/// The session's look as this sign-on has it: the scale, keypad and
/// colour map the toolbar changed.
struct Prefs {
    scale: usize,
    keypad: bool,
    scheme: Scheme,
}

fn prefs(rq: &Rq<'_>, s: &Saved) -> Prefs {
    let scale = match rq.get(&key(s.id, "scale")).as_str() {
        "2" => 2,
        _ => 1,
    };
    let keypad = match rq.get(&key(s.id, "kp")).as_str() {
        "0" => false,
        "1" => true,
        _ => s.file.session.keypad_visible,
    };
    let bw = match rq.get(&key(s.id, "col")).as_str() {
        "bw" => true,
        "console" => false,
        _ => s.file.session.colour_map == ColorMap::BlackOnWhite,
    };
    Prefs {
        scale,
        keypad,
        scheme: if bw {
            Scheme::BlackOnWhite
        } else {
            Scheme::Console
        },
    }
}

/// The live session, connecting it first when it never was and the
/// session connects automatically (or `connect` says so).
fn live(rq: &Rq<'_>, s: &Saved, connect: bool) -> Result<Live, Message> {
    let env = &rq.env().session5250;
    let have = env.live(rq.user(), s.id);
    if connect || (have.is_none() && s.file.session.auto_connect) {
        connect_now(rq, s)?;
        return env.live(rq.user(), s.id).ok_or_else(|| gone(s));
    }
    Ok(have.unwrap_or_else(|| idle(s)))
}

/// A session that is not connected: a blank screen.
fn idle(s: &Saved) -> Live {
    let (rows, cols) = s.file.session.screen_size.rows_cols();
    Live {
        view: View {
            screen: Screen::blank(rows, cols),
            system: s.file.system.name.clone(),
            connected: false,
            tls: s.file.system.tls,
            scheme: Scheme::Console,
        },
        state: State::Disconnected,
        message: None,
        version: 0,
    }
}

fn gone(s: &Saved) -> Message {
    let sys = s.file.system.name.as_str();
    Message::new(
        Kind::Error,
        "QAD1006",
        &format!("Session to {sys} disconnected."),
    )
    .with_data(&[sys])
}

/// Connect, auto sign-on only where the policy allows it; the session's
/// last use is kept.
fn connect_now(rq: &Rq<'_>, s: &Saved) -> Result<(), Message> {
    let mut s = s.clone();
    if !rq.allowed(policy::S5250_AUTOSIGNON) {
        s.file.session.auto_sign_on = false;
    }
    rq.env().session5250.connect(rq.user(), &s)?;
    if s.id != 0 {
        let mut kept = s.clone();
        kept.last_used = rq.env().system.now();
        if let Some(orig) = saved(rq, s.id) {
            kept.file = orig.file;
        }
        let _ = rq.env().session5250.put(rq.user(), kept);
    }
    Ok(())
}

/// `GET/POST /5250/session/<id>`.
pub fn page(rq: &Rq<'_>, id: u32) -> Response {
    let Some(s) = saved(rq, id) else {
        return rq.not_found("5250 session");
    };
    let title = s.file.display_title();
    let crumb = ["5250 session", title.as_str()];
    if let Err(r) = start_policy(rq, &crumb) {
        return r;
    }
    let q = rq.query();
    if let Some(n) = q.get("scale").filter(|n| *n == "1" || *n == "2") {
        rq.set(&[(&key(id, "scale"), n)]);
    }
    if rq.post() {
        return post(rq, &s);
    }
    if q.has("save") {
        return save(rq, &s);
    }
    if let Some(name) = q.get("play") {
        return play(rq, &s, name);
    }
    let mut messages = Vec::new();
    let l = match live(rq, &s, q.has("connect")) {
        Ok(l) => l,
        // The connection failed: the page shows why over the screen as
        // it is, without trying again.
        Err(m) => {
            messages.push(m);
            rq.env()
                .session5250
                .live(rq.user(), s.id)
                .unwrap_or_else(|| idle(&s))
        }
    };
    if let Some(m) = &l.message
        && l.state == State::Disconnected
    {
        messages.push(m.clone());
    }
    let menu = q
        .get("menu")
        .map_or_else(|| rq.get(&key(id, "menu")), ToString::to_string);
    let menu = if MENUS.iter().any(|m| m.0 == menu) {
        menu
    } else {
        "file".into()
    };
    rq.set(&[(&key(id, "menu"), &menu)]);
    let p = prefs(rq, &s);
    let wide = if l.view.screen.rows == 27 {
        " (27x132)"
    } else {
        ""
    };
    let body = body(rq, &s, &l, &p, &menu);
    let h1 = format!("5250 display: {title}{wide}");
    let mut page = Page {
        title: h1.clone(),
        crumb: crumb.iter().map(ToString::to_string).collect(),
        h1,
        messages,
        body,
        help: "5250session".into(),
        nav: Some((GROUP, Some(ITEM))),
        ..Page::default()
    };
    if let Some(m) = rq.app.sessions.take_flash(&rq.session.id) {
        page.messages.insert(0, m);
    }
    // A failed Connect, then the page connecting again, says it once.
    page.messages
        .dedup_by(|a, b| a.id == b.id && a.text == b.text);
    Response::html(200, wide_frame(&render(&rq.ctx, &page), rq, id, &p))
}

/// The frame without the tree, the content across both columns, and the
/// page's one script.
fn wide_frame(html: &str, rq: &Rq<'_>, id: u32, p: &Prefs) -> String {
    let mut out = html.to_string();
    if let (Some(a), Some(b)) = (
        out.find("<td class=\"nav\" valign=\"top\">"),
        out.find("<td class=\"content\" valign=\"top\">"),
    ) {
        out.replace_range(a..b, "");
    }
    out = out.replacen(
        "<td class=\"content\" valign=\"top\">",
        "<td class=\"content\" valign=\"top\" colspan=\"2\">",
        1,
    );
    let script = format!(
        "<script src=\"{}\" data-session=\"{id}\" data-scale=\"{}\" data-scheme=\"{}\"></script>\n",
        esc(&rq.ctx.url("/5250/script.js")),
        p.scale,
        if p.scheme == Scheme::BlackOnWhite {
            "bw"
        } else {
            "console"
        }
    );
    out.replacen("</head>", &format!("{script}</head>"), 1)
}

#[allow(clippy::many_single_char_names)]
#[allow(clippy::too_many_lines)]
fn body(rq: &Rq<'_>, s: &Saved, l: &Live, p: &Prefs, menu: &str) -> String {
    let id = s.id;
    let url = |r: &str| esc(&rq.ctx.url(r));
    let here = format!("/5250/session/{id}");
    let mut b = String::new();
    // Menu bar: the menu names, the open menu's items below.
    let names: Vec<String> = MENUS
        .iter()
        .map(|(k, n)| {
            if *k == menu {
                format!("<b>{n}</b>")
            } else {
                format!(
                    "<a href=\"{}{}\">{n}</a>",
                    url(&here),
                    esc(&query(&[("menu", k)]))
                )
            }
        })
        .chain([
            format!(
                "<a href=\"{}\">Macro</a>",
                url(&format!("/5250/{id}/macros"))
            ),
            format!(
                "<a href=\"{}\">Help</a>",
                esc(&rq.ctx.product.help("5250session"))
            ),
        ])
        .collect();
    let connected = l.state != State::Disconnected;
    let items = menu_items(rq, s, menu, connected);
    let _ = write!(
        b,
        "<table class=\"list\" cellspacing=\"0\" style=\"margin-bottom:3px\"><tr><th>{}</th></tr><tr><td class=\"small\">{items}</td></tr></table>",
        names.join(" &nbsp;&nbsp; ")
    );
    // Toolbar.
    let tb = |v: &str, on: bool| {
        format!(
            "<input type=\"submit\" name=\"tb\" value=\"{v}\"{}>",
            if on { "" } else { " disabled" }
        )
    };
    let scale = if p.scale == 2 {
        format!(
            "<a href=\"{}{}\">1x</a> <b>2x</b>",
            url(&here),
            esc(&query(&[("scale", "1")]))
        )
    } else {
        format!(
            "<b>1x</b> <a href=\"{}{}\">2x</a>",
            url(&here),
            esc(&query(&[("scale", "2")]))
        )
    };
    let print = rq.allowed(policy::S5250_PRINT);
    let _ = write!(
        b,
        "<form method=\"post\">{}<div class=\"toolbar\">{}{} &nbsp;|&nbsp; {}{}{} &nbsp;|&nbsp; {}{} &nbsp;|&nbsp; <span class=\"small\">Scale: {scale}</span></div></form>",
        rq.ctx.csrf_field(),
        tb("Connect", !connected),
        tb("Disconnect", connected),
        tb("Copy", true),
        tb("Paste", true),
        tb("Print Screen", print),
        tb("Keypad", true),
        tb("Keyboard Map", true),
    );
    // The screen and the keypad.
    let mut view = l.view.clone();
    view.scheme = p.scheme;
    let (w, h) = view.size(p.scale);
    let png = format!(
        "/5250/session/{id}/screen.png{}",
        query(&[
            ("v", &l.version.to_string()),
            ("scale", &p.scale.to_string())
        ])
    );
    let _ = write!(
        b,
        "<form method=\"post\">{}<table cellspacing=\"0\" cellpadding=\"0\" style=\"border:1px solid #808080\"><tr><td style=\"padding:0;vertical-align:top;background:#000000\"><img src=\"{}\" width=\"{w}\" height=\"{h}\" alt=\"5250 screen\" style=\"display:block;image-rendering:pixelated;image-rendering:crisp-edges\"></td>",
        rq.ctx.csrf_field(),
        url(&png)
    );
    if p.keypad {
        b.push_str("<td style=\"padding:0;vertical-align:top;background:#e8e8e8;border-left:1px solid #c0c0c0;width:178px\"><table cellspacing=\"0\" cellpadding=\"2\" style=\"margin:2px\" class=\"keypad\">");
        for (i, row) in KEYPAD.chunks(3).enumerate() {
            b.push_str("<tr>");
            for (label, _) in row {
                let td = if i == 8 {
                    "<td style=\"padding-top:8px\">"
                } else {
                    "<td>"
                };
                let dis = if connected { "" } else { " disabled" };
                let _ = write!(
                    b,
                    "{td}<input type=\"submit\" name=\"k\" value=\"{label}\" style=\"{BUTTON}\"{dis}></td>"
                );
            }
            b.push_str("</tr>");
        }
        b.push_str("</table></td>");
    }
    b.push_str("</tr></table></form>");
    // Status bar: message, indicator, system and state.
    let (msg, ind, cls, state) = match l.state {
        State::Disconnected => (
            "Not connected. Communication &gt; Connect, or Reconnect.".to_string(),
            String::new(),
            "err",
            "Not connected",
        ),
        st => {
            let (r, c) = l.view.screen.cursor;
            let tls = if l.view.tls { "TLS &nbsp; " } else { "" };
            let state = if st == State::SigningOn {
                "Connected, signing on"
            } else {
                "Connected"
            };
            let msg = recording(rq, id)
                .map(|n| format!("Recording macro {}", esc(&n)))
                .unwrap_or_default();
            (msg, format!("{tls}{r:02}/{c:03}"), "ok", state)
        }
    };
    let _ = write!(
        b,
        "<table class=\"list\" cellspacing=\"0\" style=\"margin-top:2px\"><tr><td style=\"width:60%\">{msg}</td><td class=\"num\" style=\"width:120px\">{ind}</td><td class=\"{cls}\" style=\"width:200px\">{}: {state}</td></tr></table>",
        esc(&l.view.system)
    );
    b
}

fn menu_items(rq: &Rq<'_>, s: &Saved, menu: &str, connected: bool) -> String {
    let id = s.id;
    let link =
        |route: &str, label: &str| format!("<a href=\"{}\">{label}</a>", esc(&rq.ctx.url(route)));
    let posts = |name: &str, labels: &[&str], on: bool| -> String {
        let buttons = labels.iter().fold(String::new(), |mut s, l| {
            let _ = write!(
                s,
                "<input type=\"submit\" name=\"{name}\" value=\"{l}\"{}>",
                if on { "" } else { " disabled" }
            );
            s
        });
        format!(
            "<form method=\"post\">{}{buttons}</form>",
            rq.ctx.csrf_field()
        )
    };
    let sep = " &nbsp;|&nbsp; ";
    match menu {
        "edit" => [
            link(&format!("/5250/session/{id}/copy"), "Copy"),
            link(&format!("/5250/session/{id}/paste"), "Paste"),
        ]
        .join(sep),
        "view" => posts("tb", &["Keypad", "Colour map"], true),
        "communication" => posts(
            "tb",
            if connected {
                &["Disconnect", "Reconnect"]
            } else {
                &["Connect", "Reconnect"]
            },
            true,
        ),
        "actions" => [
            posts(
                "k",
                &["Enter", "Reset", "Attn", "SysRq", "Help", "Clear"],
                connected,
            ),
            link(&format!("/5250/{id}/keymap"), "Keyboard map"),
            link(&format!("/5250/{id}/keymap"), "Load keyboard map"),
            link(&format!("/5250/{id}/keymap/export"), "Export keyboard map"),
        ]
        .join(sep),
        _ => [
            link("/5250", "Open session..."),
            format!(
                "<a href=\"{}{}\">Save session</a>",
                esc(&rq.ctx.url(&format!("/5250/session/{id}"))),
                esc(&query(&[("save", "1")]))
            ),
            link("/5250", "Save session as..."),
            format!(
                "<a href=\"{}{}\">Recent sessions</a>",
                esc(&rq.ctx.url("/5250")),
                esc(&query(&[("sort", "l")]))
            ),
            link(&format!("/5250/{id}/properties"), "Properties..."),
            link(&format!("/5250/session/{id}/print"), "Print screen"),
            format!(
                "<a href=\"{}{}\">Close</a>",
                esc(&rq.ctx.url("/5250")),
                esc(&query(&[("close", &id.to_string())]))
            ),
        ]
        .join(sep),
    }
}

/// File > Save session: the session's settings as the display has them
/// now (keypad, colour map).
fn save(rq: &Rq<'_>, s: &Saved) -> Response {
    let title = s.file.display_title();
    if let Err(r) = rq.policy(policy::S5250_SAVE, &["5250 session", &title]) {
        return r;
    }
    let p = prefs(rq, s);
    let mut s = s.clone();
    s.file.session.keypad_visible = p.keypad;
    s.file.session.colour_map = if p.scheme == Scheme::BlackOnWhite {
        ColorMap::BlackOnWhite
    } else {
        ColorMap::Console
    };
    let back = format!("/5250/session/{}", s.id);
    match rq.env().session5250.put(rq.user(), s) {
        Ok(_) => rq.done(
            &back,
            rq.message(Kind::Info, crate::msgs::SESSION_SAVED, &[&title]),
        ),
        Err(m) => rq.done(&back, m),
    }
}

/// The toolbar's and keypad's posts.
fn post(rq: &Rq<'_>, s: &Saved) -> Response {
    let f = rq.form();
    let id = s.id;
    let back = format!("/5250/session/{id}");
    let env = &rq.env().session5250;
    if let Some(label) = f.get("k") {
        let Some((_, func)) = KEYPAD.iter().find(|(l, _)| *l == label) else {
            return rq.redirect(&back);
        };
        if let Err(m) = send(rq, id, Input::Key((*func).to_string())) {
            rq.flash(m);
        }
        return rq.redirect(&back);
    }
    match f.str("tb") {
        "Connect" => {
            if let Err(m) = connect_now(rq, s) {
                rq.flash(m);
            }
        }
        "Disconnect" => env.disconnect(rq.user(), id),
        "Reconnect" => {
            env.disconnect(rq.user(), id);
            if let Err(m) = connect_now(rq, s) {
                rq.flash(m);
            }
        }
        "Copy" => return rq.redirect(&format!("/5250/session/{id}/copy")),
        "Paste" => return rq.redirect(&format!("/5250/session/{id}/paste")),
        "Print Screen" => return rq.redirect(&format!("/5250/session/{id}/print")),
        "Keypad" => {
            let p = prefs(rq, s);
            rq.set(&[(&key(id, "kp"), if p.keypad { "0" } else { "1" })]);
        }
        "Colour map" => {
            let p = prefs(rq, s);
            rq.set(&[(
                &key(id, "col"),
                if p.scheme == Scheme::BlackOnWhite {
                    "console"
                } else {
                    "bw"
                },
            )]);
        }
        "Keyboard Map" => return rq.redirect(&format!("/5250/{id}/keymap")),
        _ => {}
    }
    rq.redirect(&back)
}

/// Send input to the live session, and to the macro being recorded.
pub fn send(rq: &Rq<'_>, id: u32, input: Input) -> Result<(), Message> {
    let env = &rq.env().session5250;
    if let Some(name) = recording(rq, id) {
        let nondisplay = env
            .live(rq.user(), id)
            .is_some_and(|l| in_nondisplay(&l.view.screen));
        let step = match &input {
            Input::Text(_) if nondisplay => Some(Action::Prompt {
                label: "Password".into(),
                hidden: true,
            }),
            Input::Text(t) => Some(Action::Type { text: t.clone() }),
            Input::Key(k) => Some(Action::Key { key: k.clone() }),
            Input::Cursor(..) => None,
        };
        if let Some(a) = step {
            let mut steps = rq.get(&key(id, "recsteps"));
            let mut m = MacroFile::new(&name, crate::s5250::files::qsd::ScreenSize::S24x80);
            m.steps = vec![Step::new(a.clone())];
            if let Input::Key(k) = &input
                && is_aid(k)
            {
                m.steps.push(Step::new(Action::WaitUnlock {
                    timeout_ms: crate::s5250::files::qsm::DEFAULT_TIMEOUT_MS,
                }));
            }
            steps.push_str(&m.to_lines());
            rq.set(&[(&key(id, "recsteps"), &steps)]);
        }
    }
    env.input(rq.user(), id, input)
}

/// Whether the cursor is in a field that does not display.
fn in_nondisplay(s: &Screen) -> bool {
    let (r, c) = s.cursor;
    s.cells
        .get((r.max(1) - 1) * s.cols + c.max(1) - 1)
        .is_some_and(|cell| {
            !cell.is_attr && crate::s5250::look::render::style(cell.attr).nondisplay
        })
}

/// The macro being recorded in session `id`, if one is.
pub fn recording(rq: &Rq<'_>, id: u32) -> Option<String> {
    let n = rq.get(&key(id, "rec"));
    (!n.is_empty()).then_some(n)
}

/// Start recording macro `name`.
pub fn start_recording(rq: &Rq<'_>, id: u32, name: &str) {
    rq.set(&[(&key(id, "rec"), name), (&key(id, "recsteps"), "")]);
}

/// Stop recording: the steps recorded.
pub fn stop_recording(rq: &Rq<'_>, id: u32) -> Option<(String, String)> {
    let name = recording(rq, id)?;
    let steps = rq.get(&key(id, "recsteps"));
    rq.set(&[(&key(id, "rec"), ""), (&key(id, "recsteps"), "")]);
    Some((name, steps))
}

/// `?play=NAME`: run a macro of the session against the live display,
/// step by step; it stops at the first step that fails (`QAD2020`).
fn play(rq: &Rq<'_>, s: &Saved, name: &str) -> Response {
    let back = format!("/5250/session/{}", s.id);
    let Some(m) = s.macros.iter().find(|m| m.file.display_name() == name) else {
        return rq.not_found(&format!("macro {}", esc(name)));
    };
    let env = &rq.env().session5250;
    let stop = |n: usize, why: &str| {
        let n = n.to_string();
        rq.message(Kind::Error, "QAD2020", &[name, &n, why])
    };
    for (i, step) in m.file.steps.iter().enumerate() {
        let r = match &step.action {
            Action::Type { text } => send(rq, s.id, Input::Text(text.clone())).map_err(|e| e.text),
            Action::Key { key } => send(rq, s.id, Input::Key(key.clone())).map_err(|e| e.text),
            Action::Pause { ms } => {
                std::thread::sleep(Duration::from_millis(u64::from(*ms).min(5000)));
                Ok(())
            }
            Action::WaitUnlock { timeout_ms } => wait_for(rq, s.id, *timeout_ms, |l| {
                l.view.screen.keyboard == crate::s5250::look::render::Keyboard::Open
            })
            .then_some(())
            .ok_or_else(|| "the keyboard stayed locked".to_string()),
            Action::WaitText {
                row,
                col,
                text,
                timeout_ms,
            } => wait_for(rq, s.id, *timeout_ms, |l| {
                at(&l.view.screen, *row, *col, text)
            })
            .then_some(())
            .ok_or_else(|| format!("\"{text}\" did not appear at {row},{col}")),
            Action::Prompt { .. } => Err("a prompt needs the page's script to ask".into()),
        };
        if let Err(why) = r {
            return rq.done(&back, stop(i + 1, &why));
        }
    }
    let _ = env;
    rq.redirect(&back)
}

/// Whether `text` is on the screen at a 1-based row and column.
pub fn at(s: &Screen, row: u8, col: u8, text: &str) -> bool {
    let rows = s.text_rows();
    rows.get(usize::from(row).saturating_sub(1))
        .is_some_and(|r| {
            r.chars()
                .skip(usize::from(col).saturating_sub(1))
                .collect::<String>()
                .starts_with(text)
        })
}

fn wait_for(rq: &Rq<'_>, id: u32, timeout_ms: u32, ok: impl Fn(&Live) -> bool) -> bool {
    let env = &rq.env().session5250;
    let end = std::time::Instant::now() + Duration::from_millis(u64::from(timeout_ms).min(60_000));
    let mut l = env.live(rq.user(), id);
    loop {
        match &l {
            Some(x) if ok(x) => return true,
            Some(x) if x.state == State::Disconnected => return false,
            None => return false,
            Some(x) => {
                let now = std::time::Instant::now();
                if now >= end {
                    return false;
                }
                l = env.wait(rq.user(), id, x.version, end - now);
            }
        }
    }
}

/// The session's other addresses.
pub fn part(rq: &Rq<'_>, id: u32, rest: &str) -> Response {
    let Some(s) = saved(rq, id) else {
        return rq.not_found("5250 session");
    };
    let title = s.file.display_title();
    let crumb = ["5250 session", title.as_str()];
    if let Err(r) = start_policy(rq, &crumb) {
        return r;
    }
    let p = prefs(rq, &s);
    // The live screen, or the blank one of a session not connected.
    let current = || {
        live(rq, &s, false).ok().map(|l| {
            let mut v = l.view;
            v.scheme = p.scheme;
            v
        })
    };
    match rest {
        "screen.png" => {
            let scale = if rq.query().get("scale") == Some("2") {
                2
            } else {
                1
            };
            let Some(v) = current() else {
                return rq.not_found("screen");
            };
            Response::new(200, "image/png", v.png(scale)).with("Cache-Control", "no-store")
        }
        "screen.txt" => {
            let Some(v) = current() else {
                return rq.not_found("screen");
            };
            Response::new(
                200,
                "text/plain; charset=utf-8",
                v.screen.text_rows().join("\n").into_bytes(),
            )
        }
        "print" => {
            if let Err(r) = rq.policy(policy::S5250_PRINT, &crumb) {
                return r;
            }
            let Some(v) = current() else {
                return rq.not_found("screen");
            };
            let name: String = title
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect();
            let pdf = pdf::print_screen(
                &v.picture(1),
                v.scheme.background(),
                &format!("{title}: 5250 screen"),
            );
            Response::download(&format!("{name}.pdf"), "application/pdf", pdf)
        }
        "key" => {
            if !rq.post() {
                return rq.redirect(&format!("/5250/session/{id}"));
            }
            let f = rq.form();
            let r = if let Some(k) = f.get("k") {
                let func = KEYPAD.iter().find(|(l, _)| *l == k).map_or(k, |(_, f)| *f);
                send(rq, id, Input::Key(func.to_string()))
            } else if let Some(t) = f.get("t") {
                send(rq, id, Input::Text(t.to_string()))
            } else {
                Ok(())
            };
            if let Err(m) = r {
                rq.flash(m);
            }
            rq.redirect(&format!("/5250/session/{id}"))
        }
        "copy" => text_page(
            rq,
            &s,
            "Copy",
            &current()
                .map(|v| v.screen.copy((1, 1), (v.screen.rows, v.screen.cols)))
                .unwrap_or_default(),
        ),
        "paste" => paste(rq, &s),
        _ => rq.not_found("page"),
    }
}

/// Edit > Copy without script: the screen's text to select and copy.
fn text_page(rq: &Rq<'_>, s: &Saved, what: &str, text: &str) -> Response {
    let title = s.file.display_title();
    let body = format!(
        "<p>The screen's text, one line per row. Select it and copy it with the browser.</p><textarea rows=\"28\" cols=\"134\" class=\"conf\" readonly>{}</textarea><p><a href=\"{}\">Back to the display</a></p>",
        esc(text),
        esc(&rq.ctx.url(&format!("/5250/session/{}", s.id)))
    );
    rq.page(Page {
        title: format!("{what}: {title}"),
        crumb: vec!["5250 session".into(), title.clone(), what.into()],
        h1: format!("{what}: {title}"),
        body,
        help: "5250session".into(),
        nav: Some((GROUP, Some(ITEM))),
        ..Page::default()
    })
}

/// Edit > Paste without script: text typed at the cursor.
fn paste(rq: &Rq<'_>, s: &Saved) -> Response {
    let id = s.id;
    if rq.post() {
        let f = rq.form();
        if !f.has("cancel")
            && let Err(m) = send(rq, id, Input::Text(f.str("t").replace(['\r', '\n'], "")))
        {
            rq.flash(m);
        }
        return rq.redirect(&format!("/5250/session/{id}"));
    }
    let title = s.file.display_title();
    let body = format!(
        "<form method=\"post\">{}<table class=\"props\" cellspacing=\"0\">\n<tr><th>Text</th><td><input type=\"text\" name=\"t\" value=\"\" size=\"60\" class=\"cmd\"> <span class=\"small grey\">typed at the cursor</span></td></tr>\n</table>\n<div class=\"buttons\"><input type=\"submit\" name=\"ok\" value=\"Paste\"><input type=\"submit\" name=\"cancel\" value=\"Cancel\"></div></form>",
        rq.ctx.csrf_field()
    );
    rq.page(Page {
        title: format!("Paste: {title}"),
        crumb: vec!["5250 session".into(), title.clone(), "Paste".into()],
        h1: format!("Paste: {title}"),
        body,
        help: "5250session".into(),
        nav: Some((GROUP, Some(ITEM))),
        ..Page::default()
    })
}

/// One row of the screen for the script: its text and a style letter
/// per cell (`.` an attribute byte's cell, else `@` + the attribute's
/// low five bits). A field that does not display goes as blanks.
pub fn row_json(s: &Screen, r: usize) -> String {
    let cells = &s.cells[r * s.cols..(r + 1) * s.cols];
    let text: String = cells
        .iter()
        .map(|c| {
            if c.is_attr || crate::s5250::look::render::style(c.attr).nondisplay {
                ' '
            } else {
                c.ch
            }
        })
        .collect();
    let styles: String = cells
        .iter()
        .map(|c| {
            if c.is_attr {
                '.'
            } else {
                char::from(b'@' + (c.attr & 0x1F))
            }
        })
        .collect();
    format!("[{r},{},{}]", json(&text), json(&styles))
}

fn json(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(o, "\\u{:04x}", c as u32);
            }
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// A screen message for the script: every row when `prev` is `None`,
/// else the rows that changed.
pub fn screen_message(l: &Live, prev: Option<&Live>) -> String {
    let s = &l.view.screen;
    let same_shape =
        prev.is_some_and(|p| p.view.screen.rows == s.rows && p.view.screen.cols == s.cols);
    let rows: Vec<String> = (0..s.rows)
        .filter(|r| {
            !same_shape
                || prev.is_some_and(|p| {
                    p.view.screen.cells[r * s.cols..(r + 1) * s.cols]
                        != s.cells[r * s.cols..(r + 1) * s.cols]
                })
        })
        .map(|r| row_json(s, r))
        .collect();
    let o = l.view.oia();
    let kb = match o.keyboard {
        Some(crate::s5250::look::render::Keyboard::System) => "X SYSTEM".to_string(),
        Some(crate::s5250::look::render::Keyboard::HostError) => "X II".to_string(),
        Some(crate::s5250::look::render::Keyboard::OperatorError(c)) => format!("X II {c:04}"),
        _ => String::new(),
    };
    format!(
        "{{\"t\":\"{}\",\"v\":{},\"rows\":{},\"cols\":{},\"cursor\":[{},{}],\"state\":\"{}\",\"system\":{},\"tls\":{},\"kb\":{},\"ins\":{},\"mw\":{},\"cells\":[{}]}}",
        if same_shape { "delta" } else { "screen" },
        l.version,
        s.rows,
        s.cols,
        s.cursor.0,
        s.cursor.1,
        match l.state {
            State::Connected => "connected",
            State::SigningOn => "signing-on",
            State::Disconnected => "disconnected",
        },
        json(&o.system),
        o.tls,
        json(&kb),
        o.insert,
        o.message_waiting,
        rows.join(",")
    )
}

/// What the script sends: `{"t":"key","k":"F:3"}`, `{"t":"type","s":".."}`,
/// `{"t":"cursor","r":5,"c":10}`.
pub fn parse_input(msg: &str) -> Option<Input> {
    let field = |name: &str| -> Option<String> {
        let at = msg.find(&format!("\"{name}\":"))? + name.len() + 3;
        let rest = msg[at..].trim_start();
        if let Some(body) = rest.strip_prefix('"') {
            let mut out = String::new();
            let mut it = body.chars();
            while let Some(c) = it.next() {
                match c {
                    '\\' => match it.next()? {
                        'n' => out.push('\n'),
                        'u' => {
                            let h: String = it.by_ref().take(4).collect();
                            out.push(char::from_u32(u32::from_str_radix(&h, 16).ok()?)?);
                        }
                        c => out.push(c),
                    },
                    '"' => return Some(out),
                    c => out.push(c),
                }
            }
            None
        } else {
            Some(rest.chars().take_while(char::is_ascii_digit).collect())
        }
    };
    match field("t")?.as_str() {
        "key" => Some(Input::Key(field("k")?)),
        "type" => Some(Input::Text(field("s")?)),
        "cursor" => Some(Input::Cursor(
            field("r")?.parse().ok()?,
            field("c")?.parse().ok()?,
        )),
        _ => None,
    }
}

/// `GET /5250/ws/<id>`: the WebSocket. Screens go out as the whole
/// screen, then the rows that changed; keys, typing and cursor moves
/// come in. Nothing a field that does not display holds is ever sent.
pub fn ws(rq: &Rq<'_>, id: u32) -> Response {
    let Some(s) = saved(rq, id) else {
        return rq.not_found("5250 session");
    };
    let title = s.file.display_title();
    if let Err(r) = start_policy(rq, &["5250 session", &title]) {
        return r;
    }
    let env = rq.app.env.session5250.clone();
    let user = rq.user().to_string();
    let recording = recording(rq, id).is_some();
    // Short waits, and every message waiting taken in one pass: a key
    // or a click shows as soon as the session has it.
    let poll = Duration::from_millis(10);
    ws::upgrade(rq.req, move |sock: &mut Socket<'_>| {
        let mut last: Option<Live> = None;
        loop {
            let now = match &last {
                None => env.live(&user, id),
                Some(l) => env.wait(&user, id, l.version, poll),
            };
            if let Some(l) = now
                && last.as_ref().is_none_or(|p| p.version != l.version)
            {
                if sock.send(&screen_message(&l, last.as_ref())).is_err() {
                    return;
                }
                last = Some(l);
            }
            let mut got = sock.recv(poll);
            loop {
                match got {
                    Ok(Some(ws::Message::Text(t))) => {
                        // Recording needs the page's own posts; the
                        // socket only types and presses keys.
                        let _ = recording;
                        if let Some(input) = parse_input(&t)
                            && let Err(m) = env.input(&user, id, input)
                        {
                            let _ = sock.send(&format!(
                                "{{\"t\":\"message\",\"id\":{},\"text\":{}}}",
                                json(&m.id),
                                json(&m.text)
                            ));
                        }
                    }
                    Ok(Some(_)) => {}
                    Ok(None) => break,
                    Err(_) => {
                        sock.close();
                        return;
                    }
                }
                got = sock.recv(Duration::from_millis(1));
            }
        }
    })
}
