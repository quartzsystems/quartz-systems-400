// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/5250`: the saved sessions (Start, Properties, Download `.qsd`,
//! Delete) and Start 5250 display, the desktop's dialog with the same
//! fields; `POST /5250/session/new` starts or saves it, or opens an
//! uploaded `.qsd`.

use qsweb::frame::Page;
use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::msg::{Kind, Message};
use qsweb::partials::{Cell, Col, List, checkbox, input, list_table, radio};

use super::{GROUP, ITEM, Rq, select, start_policy, when_short, yes};
use crate::env::session5250::Saved;
use crate::policy;
use crate::s5250::files::qsd::{CCSIDS, ColorMap, HOTSPOT_FUNCTION_KEYS, HOTSPOT_PAGING, ScreenSize, SessionFile, check_device};

/// The web session's key for a session started without saving.
const UNSAVED: &str = "5250.new";

/// The session started without saving, if there is one.
pub fn unsaved(rq: &Rq<'_>) -> Option<Saved> {
    let text = rq.get(UNSAVED);
    let file = SessionFile::parse(&text).ok()?;
    Some(Saved {
        id: 0,
        file,
        last_used: 0,
        macros: Vec::new(),
    })
}

/// The device name as the lists show it.
pub fn device(f: &SessionFile) -> String {
    if f.session.device_name.is_empty() { "*DFT".into() } else { f.session.device_name.clone() }
}

/// `GET /5250`; `?close=N` ends live session N first.
pub fn page(rq: &Rq<'_>) -> Response {
    let crumb = ["5250 session"];
    if let Err(r) = start_policy(rq, &crumb) {
        return r;
    }
    let q = rq.query();
    if let Some(n) = q.get("close").and_then(|n| n.parse::<u32>().ok()) {
        rq.env().session5250.disconnect(rq.user(), n);
        return rq.redirect("/5250");
    }
    show(rq, Vec::new())
}

fn show(rq: &Rq<'_>, messages: Vec<Message>) -> Response {
    let q = rq.query();
    let mut sessions = rq.env().session5250.sessions(rq.user());
    match q.get("sort") {
        Some("l") => sessions.sort_by_key(|s| std::cmp::Reverse(s.last_used)),
        Some("n") => sessions.sort_by_key(|s| s.file.display_title().to_ascii_uppercase()),
        _ => {}
    }
    let save = rq.allowed(policy::S5250_SAVE);
    let rows = sessions
        .iter()
        .map(|s| {
            let id = s.id;
            let mut acts = vec![
                format!("<a href=\"{}\">Start</a>", esc(&rq.ctx.url(&format!("/5250/session/{id}")))),
                format!("<a href=\"{}\">Properties</a>", esc(&rq.ctx.url(&format!("/5250/{id}/properties")))),
                format!("<a href=\"{}\">Download .qsd</a>", esc(&rq.ctx.url(&format!("/5250/{id}/download")))),
            ];
            if save {
                acts.push(format!("<a href=\"{}\">Delete</a>", esc(&rq.ctx.url(&format!("/5250/{id}/delete/confirm")))));
            }
            let f = &s.file;
            vec![
                Cell::act(acts.join(" | ")),
                Cell::text(&f.display_title()),
                Cell::mono(&f.system.name),
                Cell::text(f.session.screen_size.label()),
                Cell::mono(&f.session.ccsid.to_string()),
                Cell::mono(&device(f)),
                Cell::text(yes(f.system.tls)),
                Cell::text(f.session.colour_map.label()),
                Cell::text(yes(f.session.keypad_visible)),
                Cell::text(yes(f.session.auto_connect)),
                Cell::text(yes(f.session.auto_sign_on)),
                Cell::text(&when_short(rq, s.last_used)),
            ]
        })
        .collect();
    let list = List {
        cols: vec![
            Col::plain("Actions"),
            Col::sorted("Session", "n"),
            Col::plain("System"),
            Col::plain("Screen size"),
            Col::plain("CCSID"),
            Col::plain("Device name"),
            Col::plain("TLS"),
            Col::plain("Colour map"),
            Col::plain("Keypad"),
            Col::plain("Auto connect"),
            Col::plain("Auto sign-on"),
            Col::sorted("Last used", "l"),
        ],
        rows,
        empty: "You have no saved sessions.".into(),
        keep: Vec::new(),
    };
    // The form starts from the session used last.
    let recent = sessions.iter().max_by_key(|s| s.last_used).cloned();
    let body = format!("<h2>Saved sessions</h2>{}{}", list_table(&list), start_form(rq, recent.as_ref(), &sessions));
    let page = Page {
        title: "5250 session".into(),
        crumb: vec!["5250 session".into()],
        h1: "5250 session".into(),
        desc: Some("Saved sessions and the settings for a new 5250 display session.".into()),
        messages,
        body,
        help: "5250".into(),
        nav: Some((GROUP, Some(ITEM))),
        ..Page::default()
    };
    rq.page(page)
}

/// The systems a session may name: this one first, then those of the
/// saved sessions.
pub fn systems(rq: &Rq<'_>, sessions: &[Saved]) -> Vec<String> {
    let me = rq.env().system.name();
    let mut v = vec![me.clone()];
    for s in sessions {
        let n = s.file.system.name.to_ascii_uppercase();
        if !v.iter().any(|x| x.eq_ignore_ascii_case(&n)) {
            v.push(n);
        }
    }
    v
}

/// The CCSID choices as the selects show them.
pub fn ccsid_options() -> Vec<String> {
    CCSIDS.iter().map(|(c, t)| format!("{c} - {t}")).collect()
}

fn start_form(rq: &Rq<'_>, from: Option<&Saved>, sessions: &[Saved]) -> String {
    let me = rq.env().system.name();
    let f = from.map_or_else(|| SessionFile::new(&me), |s| s.file.clone());
    let s = &f.session;
    let mut sys: Vec<String> = systems(rq, sessions);
    let chosen = sys.iter().position(|n| n.eq_ignore_ascii_case(&f.system.name)).unwrap_or(0);
    sys[0] = format!("{me} (this system)");
    sys.push("Other...".into());
    let cp = CCSIDS.iter().position(|c| c.0 == s.ccsid).unwrap_or(0);
    let mut km = vec!["Default".to_string()];
    km.extend(rq.env().session5250.keymaps(rq.user()).into_iter().map(|(n, _)| format!("{n} (.qkm)")));
    km.push("Upload...".into());
    let wide = s.screen_size == ScreenSize::S27x132;
    let bw = s.colour_map == ColorMap::BlackOnWhite;
    let aso = rq.allowed(policy::S5250_AUTOSIGNON);
    let row = |label: &str, value: String| format!("<tr><th>{label}</th><td>{value}</td></tr>\n");
    let mut t = String::from("<table class=\"props\" cellspacing=\"0\">\n<tr class=\"subrow\"><td colspan=\"2\">Start 5250 display</td></tr>\n");
    t += &row("Session name", input("n", &f.display_title(), 30));
    t += &row("System", select("sys", &sys, chosen));
    t += &row(
        "Screen size",
        format!("{} 24 x 80 &nbsp; {} 27 x 132", radio("sz", "24x80", !wide), radio("sz", "27x132", wide)),
    );
    t += &row("Code page (CCSID)", select("cp", &ccsid_options(), cp));
    t += &row(
        "Device name",
        format!("{} <span class=\"small grey\">*DFT lets the system name it</span>", input("dev", &device(&f), 10)),
    );
    t += &row("Use TLS", format!("{} Telnet TLS, port 992", checkbox("tls", f.system.tls)));
    t += &row(
        "Colour map",
        format!("{} QS/400 console &nbsp; {} Black on white", radio("col", "console", !bw), radio("col", "bw", bw)),
    );
    t += &row("Keypad", format!("{} Show the keypad at the right of the screen", checkbox("kp", s.keypad_visible)));
    t += &row("Connect automatically", format!("{} Connect when the session opens", checkbox("ac", s.auto_connect)));
    let aso_box = if aso {
        checkbox("aso", s.auto_sign_on)
    } else {
        "<input type=\"checkbox\" name=\"aso\" disabled>".to_string()
    };
    t += &row(
        "Sign on automatically",
        format!("{aso_box} With a one-time ticket for the signed-on profile (needs TLS)"),
    );
    t += &row(
        "Hotspots",
        format!(
            "{} Function-key labels {} More... / Bottom paging",
            checkbox("h1", f.hotspots.on(HOTSPOT_FUNCTION_KEYS)),
            checkbox("h2", f.hotspots.on(HOTSPOT_PAGING))
        ),
    );
    t += &row("Keyboard map", format!("{} <input type=\"file\" name=\"qkm\" size=\"20\">", select("km", &km, 0)));
    t += "<tr class=\"subrow\"><td colspan=\"2\">Session file</td></tr>\n";
    t += &row(
        "Open a session file",
        format!(
            "<input type=\"file\" name=\"qsd\" size=\"30\"> {}",
            "<span class=\"small grey\">.qsd from QS/400 Access for Desktop; the same file works in both</span>"
        ),
    );
    t += "</table>";
    let save = if rq.allowed(policy::S5250_SAVE) {
        "<input type=\"submit\" name=\"save\" value=\"Save session\">"
    } else {
        "<input type=\"submit\" name=\"save\" value=\"Save session\" disabled>"
    };
    format!(
        "<form method=\"post\" action=\"{}\" enctype=\"multipart/form-data\">{}{t}<div class=\"buttons\"><input type=\"submit\" name=\"start\" value=\"Start session\">{save}<input type=\"submit\" name=\"verify\" value=\"Verify connection...\" formaction=\"{}\"></div></form>",
        esc(&rq.ctx.url("/5250/session/new")),
        rq.ctx.csrf_field(),
        esc(&rq.ctx.url("/5250/verify")),
    )
}

/// The session the form describes, from `base` (a saved one or new).
pub fn from_form(rq: &Rq<'_>, mut file: SessionFile) -> Result<SessionFile, Message> {
    let f = rq.form();
    let bad = |what: &str, v: &str| rq.message(Kind::Error, crate::msgs::BAD_VALUE, &[v, what]);
    if let Some(n) = f.get("n").or(f.get("t")) {
        file.session.title = n.trim().to_string();
    }
    if let Some(sys) = f.get("sys") {
        let sys = sys.trim_end_matches(" (this system)").trim();
        if sys == "Other..." || sys.is_empty() || !crate::s5250::files::qsd::check_name(sys) {
            return Err(bad("System", sys));
        }
        if !sys.eq_ignore_ascii_case(&file.system.name) {
            file.system.name = sys.to_string();
            file.system.host = String::new();
        }
    }
    match f.get("sz") {
        Some("27x132") => file.session.screen_size = ScreenSize::S27x132,
        Some("24x80") | None => file.session.screen_size = ScreenSize::S24x80,
        Some(v) => return Err(bad("Screen size", v)),
    }
    if let Some(cp) = f.get("cp") {
        let n = cp.split(' ').next().and_then(|c| c.parse::<u16>().ok()).filter(|c| CCSIDS.iter().any(|x| x.0 == *c));
        file.session.ccsid = n.ok_or_else(|| bad("Code page (CCSID)", cp))?;
    }
    if let Some(d) = f.get("dev") {
        file.session.device_name = check_device(d).map_err(|_| bad("Device name", d))?;
    }
    file.system.tls = f.has("tls");
    file.system.telnet_port = if file.system.tls { 992 } else { 23 };
    file.session.colour_map = if f.get("col") == Some("bw") { ColorMap::BlackOnWhite } else { ColorMap::Console };
    file.session.keypad_visible = f.has("kp");
    file.session.auto_connect = f.has("ac");
    file.session.auto_sign_on = f.has("aso") && rq.allowed(policy::S5250_AUTOSIGNON);
    file.hotspots.enabled = true;
    file.hotspots.set(HOTSPOT_FUNCTION_KEYS, f.has("h1"));
    file.hotspots.set(HOTSPOT_PAGING, f.has("h2"));
    Ok(file)
}

/// `POST /5250/session/new`: open an uploaded `.qsd`, start the form's
/// session, or save it.
pub fn start(rq: &Rq<'_>) -> Response {
    let crumb = ["5250 session"];
    if let Err(r) = start_policy(rq, &crumb) {
        return r;
    }
    if !rq.post() {
        return rq.redirect("/5250");
    }
    let env = rq.env();
    let parts = rq.req.multipart().unwrap_or_default();
    let upload = |name: &str| parts.iter().find(|p| p.name == name && p.filename.as_deref().is_some_and(|n| !n.is_empty()));
    let f = rq.form();
    // An uploaded session file is kept as it is.
    if let Some(p) = upload("qsd") {
        let name = p.filename.clone().unwrap_or_default();
        let text = String::from_utf8_lossy(&p.data).into_owned();
        return match SessionFile::parse(&text) {
            Ok(file) => {
                if let Err(r) = rq.policy(policy::S5250_SAVE, &crumb) {
                    return r;
                }
                let title = file.display_title();
                match env.session5250.put(rq.user(), Saved { id: 0, file, last_used: 0, macros: Vec::new() }) {
                    Ok(_) => rq.done("/5250", rq.message(Kind::Info, crate::msgs::SESSION_SAVED, &[&title])),
                    Err(m) => show(rq, vec![m]),
                }
            }
            Err(e) => {
                let (id, data) = e.message(&name);
                let data: Vec<&str> = data.iter().map(String::as_str).collect();
                show(rq, vec![rq.message(Kind::Error, id, &data)])
            }
        };
    }
    let sessions = env.session5250.sessions(rq.user());
    let title = f.str("n").trim().to_string();
    let existing = sessions.iter().find(|s| s.file.display_title().eq_ignore_ascii_case(&title)).cloned();
    let base = existing.as_ref().map_or_else(|| SessionFile::new(&env.system.name()), |s| s.file.clone());
    let mut file = match from_form(rq, base) {
        Ok(file) => file,
        Err(m) => return show(rq, vec![m]),
    };
    // A keyboard map: an upload, or one of the user's.
    if let Some(p) = upload("qkm") {
        match crate::s5250::files::qkm::KeymapFile::parse(&String::from_utf8_lossy(&p.data)) {
            Ok(k) => file.set_keymap(&k.map),
            Err(e) => {
                let (id, data) = e.message(p.filename.as_deref().unwrap_or(""));
                let data: Vec<&str> = data.iter().map(String::as_str).collect();
                return show(rq, vec![rq.message(Kind::Error, id, &data)]);
            }
        }
    } else if let Some(name) = f.get("km").and_then(|k| k.strip_suffix(" (.qkm)"))
        && let Some((_, map)) = env.session5250.keymaps(rq.user()).into_iter().find(|(n, _)| n == name)
    {
        file.set_keymap(&map);
    }
    if let Err(e) = file.validate() {
        let (id, data) = e.message(&title);
        let data: Vec<&str> = data.iter().map(String::as_str).collect();
        return show(rq, vec![rq.message(Kind::Error, id, &data)]);
    }
    if f.has("save") {
        if let Err(r) = rq.policy(policy::S5250_SAVE, &crumb) {
            return r;
        }
        let mut s = existing.unwrap_or(Saved { id: 0, file: file.clone(), last_used: 0, macros: Vec::new() });
        s.file = file;
        let title = s.file.display_title();
        return match env.session5250.put(rq.user(), s) {
            Ok(_) => rq.done("/5250", rq.message(Kind::Info, crate::msgs::SESSION_SAVED, &[&title])),
            Err(m) => show(rq, vec![m]),
        };
    }
    // Start: a saved session of that name as the form has it now, else
    // one kept only for this sign-on.
    let id = match existing {
        Some(s) if s.file == file => s.id,
        _ => {
            rq.set(&[(UNSAVED, &file.to_text())]);
            0
        }
    };
    rq.redirect(&format!("/5250/session/{id}?connect=1"))
}

/// `GET /5250/<id>/download`: the `.qsd` file.
#[allow(clippy::needless_pass_by_value)] // the area hands every page its session
pub fn download(rq: &Rq<'_>, s: Saved) -> Response {
    if let Err(r) = start_policy(rq, &["5250 session"]) {
        return r;
    }
    let name: String = s.file.display_title().chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    Response::download(&format!("{name}.qsd"), "application/toml; charset=utf-8", s.file.to_text().into_bytes())
}

/// `/5250/<id>/delete/confirm`: delete a saved session after asking.
#[allow(clippy::needless_pass_by_value)] // the area hands every page its session
pub fn delete(rq: &Rq<'_>, s: Saved) -> Response {
    let title = s.file.display_title();
    let crumb = ["5250 session", title.as_str(), "Delete"];
    if let Err(r) = rq.policy(policy::S5250_SAVE, &crumb) {
        return r;
    }
    if rq.post() {
        if rq.form().has("cancel") {
            return rq.redirect("/5250");
        }
        rq.env().session5250.disconnect(rq.user(), s.id);
        return match rq.env().session5250.delete(rq.user(), s.id) {
            Ok(()) => rq.done("/5250", rq.message(Kind::Info, crate::msgs::SESSION_DELETED, &[&title])),
            Err(m) => show(rq, vec![m]),
        };
    }
    let table = format!(
        "<table class=\"list\" cellspacing=\"0\" cellpadding=\"0\">\n<tr>\n<th>Session</th>\n<th>System</th>\n<th>Last used</th>\n</tr>\n<tr>\n<td>{}</td>\n<td class=\"mono\">{}</td>\n<td>{}</td>\n</tr>\n</table>",
        esc(&title),
        esc(&s.file.system.name),
        esc(&when_short(rq, s.last_used))
    );
    let page = Page {
        title: "Confirm delete".into(),
        crumb: crumb.iter().map(ToString::to_string).collect(),
        h1: "Confirm delete".into(),
        body: qsweb::pages::confirm_body(
            &rq.ctx,
            "Delete this saved session? Its keyboard map and macros go with it; a live display of it is ended.",
            &table,
            "",
        ),
        help: "confirm".into(),
        nav: Some((GROUP, Some(ITEM))),
        ..Page::default()
    };
    rq.page(page)
}
