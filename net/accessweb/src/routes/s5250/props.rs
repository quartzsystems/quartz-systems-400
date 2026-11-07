// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/5250/<id>/properties`: a saved session's settings, the desktop's
//! Properties dialog. System, screen size, CCSID, device name and TLS
//! take effect at the next connection; the rest at once.

use qsweb::frame::Page;
use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::msg::{Kind, Message};
use qsweb::partials::{checkbox, input, radio};

use super::config::{ccsid_options, device, from_form, systems};
use super::{GROUP, ITEM, Rq, select, start_policy};
use crate::env::session5250::Saved;
use crate::policy;
use crate::s5250::files::qsd::{CCSIDS, ColorMap, HOTSPOT_FUNCTION_KEYS, HOTSPOT_PAGING, ScreenSize};

/// `GET/POST /5250/<id>/properties`.
pub fn page(rq: &Rq<'_>, s: Saved) -> Response {
    let title = s.file.display_title();
    let crumb = ["5250 session", title.as_str(), "Properties"];
    if let Err(r) = start_policy(rq, &crumb) {
        return r;
    }
    if rq.post() {
        let f = rq.form();
        let back = if s.id == 0 { "/5250/session/0".to_string() } else { "/5250".to_string() };
        if f.has("cancel") {
            return rq.redirect(&back);
        }
        if let Err(r) = rq.policy(policy::S5250_SAVE, &crumb) {
            return r;
        }
        let file = match from_form(rq, s.file.clone()) {
            Ok(file) => file,
            Err(m) => return show(rq, &s, vec![m]),
        };
        if let Err(e) = file.validate() {
            let (id, data) = e.message(&title);
            let data: Vec<&str> = data.iter().map(String::as_str).collect();
            return show(rq, &s, vec![rq.message(Kind::Error, id, &data)]);
        }
        let new_title = file.display_title();
        let saved = Saved { file, ..s };
        return match rq.env().session5250.put(rq.user(), saved) {
            Ok(_) => rq.done(&back, rq.message(Kind::Info, crate::msgs::SESSION_SAVED, &[&new_title])),
            Err(m) => show(rq, &s_placeholder(rq, &new_title), vec![m]),
        };
    }
    show(rq, &s, Vec::new())
}

/// The session again by title, for a refusal after the change.
fn s_placeholder(rq: &Rq<'_>, title: &str) -> Saved {
    rq.env()
        .session5250
        .sessions(rq.user())
        .into_iter()
        .find(|x| x.file.display_title() == title)
        .unwrap_or(Saved {
            id: 0,
            file: crate::s5250::files::qsd::SessionFile::new(title),
            last_used: 0,
            macros: Vec::new(),
        })
}

fn show(rq: &Rq<'_>, s: &Saved, messages: Vec<Message>) -> Response {
    let f = &s.file;
    let ss = &f.session;
    let title = f.display_title();
    let sessions = rq.env().session5250.sessions(rq.user());
    let sys = systems(rq, &sessions);
    let chosen = sys.iter().position(|n| n.eq_ignore_ascii_case(&f.system.name)).unwrap_or(0);
    let cp = CCSIDS.iter().position(|c| c.0 == ss.ccsid).unwrap_or(0);
    let wide = ss.screen_size == ScreenSize::S27x132;
    let bw = ss.colour_map == ColorMap::BlackOnWhite;
    let row = |label: &str, value: String| format!("<tr><th>{label}</th><td>{value}</td></tr>\n");
    let mut t = String::from("<table class=\"props\" cellspacing=\"0\">\n<tr class=\"subrow\"><td colspan=\"2\">Session</td></tr>\n");
    t += &row("Title", input("t", &title, 30));
    t += &row("System", select("sys", &sys, chosen));
    t += &row(
        "Screen size",
        format!("{} 24 x 80 &nbsp; {} 27 x 132", radio("sz", "24x80", !wide), radio("sz", "27x132", wide)),
    );
    t += &row("CCSID", select("cp", &ccsid_options(), cp));
    t += &row("Device name", input("dev", &device(f), 10));
    t += &row("Use TLS", checkbox("tls", f.system.tls));
    t += "<tr class=\"subrow\"><td colspan=\"2\">Behaviour</td></tr>\n";
    t += &row(
        "Colour map",
        format!("{} QS/400 console &nbsp; {} Black on white", radio("col", "console", !bw), radio("col", "bw", bw)),
    );
    t += &row("Connect automatically", checkbox("ac", ss.auto_connect));
    let aso = if rq.allowed(policy::S5250_AUTOSIGNON) {
        checkbox("aso", ss.auto_sign_on)
    } else {
        "<input type=\"checkbox\" name=\"aso\" disabled>".into()
    };
    t += &row("Sign on automatically", aso);
    t += &row(
        "Hotspots",
        format!(
            "{} Function-key labels {} More... / Bottom",
            checkbox("h1", f.hotspots.on(HOTSPOT_FUNCTION_KEYS)),
            checkbox("h2", f.hotspots.on(HOTSPOT_PAGING))
        ),
    );
    t += &row("Keypad visible", checkbox("kp", ss.keypad_visible));
    t += "</table>";
    let ok = if rq.allowed(policy::S5250_SAVE) {
        "<input type=\"submit\" name=\"ok\" value=\"OK\">"
    } else {
        "<input type=\"submit\" name=\"ok\" value=\"OK\" disabled>"
    };
    let body = format!(
        "<form method=\"post\">{}{t}\n<div class=\"buttons\">{ok}<input type=\"submit\" name=\"cancel\" value=\"Cancel\"></div></form><p class=\"small grey\">System, screen size, CCSID, device name and TLS take effect at the next connection; the rest at once.</p>",
        rq.ctx.csrf_field()
    );
    let page = Page {
        title: format!("Session properties: {title}"),
        crumb: vec!["5250 session".into(), title.clone(), "Properties".into()],
        h1: format!("Session properties: {title}"),
        messages,
        body,
        help: "5250props".into(),
        nav: Some((GROUP, Some(ITEM))),
        ..Page::default()
    };
    let _ = esc;
    rq.page(page)
}
