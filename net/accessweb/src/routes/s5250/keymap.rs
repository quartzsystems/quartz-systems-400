// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/5250/<id>/keymap`: the session's keyboard map, the desktop's
//! dialog: each key and its function, a key added, a `.qkm` loaded or
//! exported, the defaults restored. Kept with the session as the changes
//! from the defaults (`.qsd` `[keymap]`).

use std::fmt::Write as _;

use qsweb::frame::Page;
use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::msg::{Kind, Message};

use super::{GROUP, ITEM, Rq, start_policy};
use crate::env::session5250::Saved;
use crate::policy;
use crate::s5250::files::keymap::{FUNCTIONS, KeyMap, function_label};
use crate::s5250::files::qkm::KeymapFile;

/// Keys shown as one row while each keeps its default: label, keys,
/// what the Function column says.
const GROUPS: &[(&str, &[&str], &str)] = &[
    ("F1 ... F12", &["F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12"], "F1 ... F12"),
    (
        "Shift+F1 ... Shift+F12",
        &["Shift+F1", "Shift+F2", "Shift+F3", "Shift+F4", "Shift+F5", "Shift+F6", "Shift+F7", "Shift+F8", "Shift+F9", "Shift+F10", "Shift+F11", "Shift+F12"],
        "F13 ... F24",
    ),
    ("Left / Right / Up / Down", &["Left", "Right", "Up", "Down"], "Cursor"),
];

/// The rows in the dialog's order: a single key, or a group's label.
const ORDER: &[&str] = &[
    "Enter",
    "Right Ctrl",
    "Keypad +",
    "Keypad -",
    "F1 ... F12",
    "Shift+F1 ... Shift+F12",
    "Page Up",
    "Page Down",
    "Esc",
    "Shift+Esc",
    "Ctrl+R",
    "Insert",
    "Pause",
    "Ctrl+Home",
    "Tab",
    "Shift+Tab",
    "Left / Right / Up / Down",
    "Delete",
    "Backspace",
];

/// One row: its label, the keys it stands for (one unless a group), and
/// the Function column.
fn rows(map: &KeyMap) -> Vec<(String, Vec<String>, String)> {
    let defaults = KeyMap::defaults();
    let label = |k: &str| map.function(k).map_or_else(|| "None".to_string(), |f| function_label(f).to_string());
    let mut out = Vec::new();
    let mut shown: Vec<String> = Vec::new();
    for item in ORDER {
        if let Some((g, keys, what)) = GROUPS.iter().find(|g| g.0 == *item) {
            if keys.iter().all(|k| map.function(k) == defaults.function(k)) {
                out.push(((*g).to_string(), keys.iter().map(ToString::to_string).collect(), (*what).to_string()));
            } else {
                for k in *keys {
                    out.push(((*k).to_string(), vec![(*k).to_string()], label(k)));
                }
            }
            shown.extend(keys.iter().map(ToString::to_string));
        } else {
            out.push(((*item).to_string(), vec![(*item).to_string()], label(item)));
            shown.push((*item).to_string());
        }
    }
    for k in map.keys.keys().filter(|k| !shown.contains(k)) {
        out.push((k.clone(), vec![k.clone()], label(k)));
    }
    out
}

fn options(chosen: Option<&str>) -> String {
    let mut s = String::new();
    if chosen.is_none() {
        s.push_str("<option selected>None</option>");
    }
    for (name, label) in FUNCTIONS {
        let sel = if Some(*name) == chosen { " selected" } else { "" };
        let _ = write!(s, "<option{sel}>{}</option>", esc(label));
    }
    s
}

fn by_label(label: &str) -> Option<&'static str> {
    FUNCTIONS.iter().find(|(_, l)| *l == label).map(|(n, _)| *n)
}

/// `GET/POST /5250/<id>/keymap`.
pub fn page(rq: &Rq<'_>, mut s: Saved) -> Response {
    let title = s.file.display_title();
    let crumb = ["5250 session", title.as_str(), "Keyboard map"];
    if let Err(r) = start_policy(rq, &crumb) {
        return r;
    }
    let back = format!("/5250/session/{}", s.id);
    if !rq.post() {
        return show(rq, &s, &s.file.keymap(), Vec::new());
    }
    let f = rq.form();
    if f.has("cancel") {
        return rq.redirect(&back);
    }
    if let Err(r) = rq.policy(policy::S5250_SAVE, &crumb) {
        return r;
    }
    let mut map = s.file.keymap();
    // The rows as posted: each single key gets its function.
    for (i, (_, keys, _)) in rows(&map).iter().enumerate() {
        if keys.len() != 1 {
            continue;
        }
        if let Some(func) = f.get(&format!("m{i}")).and_then(by_label) {
            map.assign(&keys[0], func);
        }
    }
    let mut messages = Vec::new();
    if f.has("restore_defaults") {
        map = KeyMap::defaults();
    } else if f.has("add") {
        let k = f.str("key").trim();
        match by_label(f.str("fn")) {
            Some(func) if !k.is_empty() => map.assign(k, func),
            _ => return show(rq, &s, &map, vec![rq.message(Kind::Error, crate::msgs::BAD_VALUE, &[k, "Add a key"])]),
        }
    } else if f.has("load") {
        let parts = rq.req.multipart().unwrap_or_default();
        let Some(p) = parts.iter().find(|p| p.name == "qkm" && p.filename.as_deref().is_some_and(|n| !n.is_empty())) else {
            return show(rq, &s, &map, vec![rq.message(Kind::Error, crate::msgs::BAD_VALUE, &["", "Load keyboard map"])]);
        };
        let name = p.filename.clone().unwrap_or_default();
        match KeymapFile::parse(&String::from_utf8_lossy(&p.data)) {
            Ok(k) => {
                map = k.map;
                let stem = name.trim_end_matches(".qkm").trim_end_matches(".QKM");
                let _ = rq.env().session5250.put_keymap(rq.user(), stem, &map);
                messages.push(rq.message(Kind::Info, "QAD2010", &[&name]));
            }
            Err(e) => {
                let (id, data) = e.message(&name);
                let data: Vec<&str> = data.iter().map(String::as_str).collect();
                return show(rq, &s, &map, vec![rq.message(Kind::Error, id, &data)]);
            }
        }
    } else if f.has("export") {
        return file(&s, &map);
    }
    s.file.set_keymap(&map);
    let done = !f.has("add") && !f.has("load") && !f.has("restore_defaults");
    match rq.env().session5250.put(rq.user(), s.clone()) {
        Ok(_) if done => rq.done(&back, rq.message(Kind::Info, crate::msgs::SESSION_SAVED, &[&title])),
        Ok(_) => {
            for m in messages {
                rq.flash(m);
            }
            rq.redirect(&format!("/5250/{}/keymap", s.id))
        }
        Err(m) => show(rq, &s, &map, vec![m]),
    }
}

fn file(s: &Saved, map: &KeyMap) -> Response {
    let name: String = s.file.display_title().chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    Response::download(&format!("{name}.qkm"), "application/toml; charset=utf-8", KeymapFile::new(map.clone()).to_text().into_bytes())
}

/// `GET /5250/<id>/keymap/export`: the map as a `.qkm` file.
#[allow(clippy::needless_pass_by_value)] // the area hands every page its session
pub fn export(rq: &Rq<'_>, s: Saved) -> Response {
    if let Err(r) = start_policy(rq, &["5250 session"]) {
        return r;
    }
    file(&s, &s.file.keymap())
}

/// The map for the page's script: key name to function.
pub fn json(map: &KeyMap) -> String {
    let pairs: Vec<String> = map
        .keys
        .iter()
        .map(|(k, f)| format!("\"{}\":\"{}\"", k.replace('\\', "\\\\").replace('"', "\\\""), f))
        .collect();
    format!("{{{}}}", pairs.join(","))
}

fn show(rq: &Rq<'_>, s: &Saved, map: &KeyMap, messages: Vec<Message>) -> Response {
    let title = s.file.display_title();
    let mut t = String::from("<table class=\"list\" width=\"100%\" cellspacing=\"0\" cellpadding=\"0\">\n<tr>\n<th>Key</th>\n<th>Function</th>\n<th>Change to</th>\n</tr>\n");
    for (i, (label, keys, what)) in rows(map).iter().enumerate() {
        t.push_str(if i % 2 == 1 { "<tr class=\"alt\">\n" } else { "<tr>\n" });
        let change = if keys.len() == 1 {
            format!("<select name=\"m{i}\">{}</select>", options(map.function(&keys[0])))
        } else {
            String::new()
        };
        let _ = write!(t, "<td class=\"mono\">{}</td>\n<td>{}</td>\n<td>{change}</td>\n</tr>\n", esc(label), esc(what));
    }
    t.push_str("</table>");
    let all = FUNCTIONS.iter().fold(String::new(), |mut s, (_, l)| {
        let _ = write!(s, "<option>{}</option>", esc(l));
        s
    });
    let body = format!(
        "<form method=\"post\" enctype=\"multipart/form-data\">{}{t}<table class=\"props\"><tr><th>Add a key</th><td><input type=\"text\" name=\"key\" value=\"\" size=\"20\"> &rarr; <select name=\"fn\">{all}</select> <input type=\"submit\" name=\"add\" value=\"Add\"></td></tr><tr><th>Load keyboard map</th><td><input type=\"file\" name=\"qkm\" size=\"30\"> <input type=\"submit\" name=\"load\" value=\"Load\"></td></tr><tr><th>Export</th><td><input type=\"submit\" name=\"export\" value=\"Export keyboard map (.qkm)\"></td></tr></table><div class=\"buttons\"><input type=\"submit\" name=\"ok\" value=\"OK\"><input type=\"submit\" name=\"restore_defaults\" value=\"Restore defaults\"><input type=\"submit\" name=\"cancel\" value=\"Cancel\"></div></form>",
        rq.ctx.csrf_field()
    );
    rq.page(Page {
        title: format!("Keyboard map: {title}"),
        crumb: vec!["5250 session".into(), title.clone(), "Keyboard map".into()],
        h1: format!("Keyboard map: {title}"),
        messages,
        body,
        help: "5250keymap".into(),
        nav: Some((GROUP, Some(ITEM))),
        ..Page::default()
    })
}
