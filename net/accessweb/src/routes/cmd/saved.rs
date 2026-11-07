// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! My commands (`/cmd/saved`): the user's saved commands with Run (as
//! interactive or batch, on Run command), Prompt, Edit, Delete (asked
//! first) and Share (name, and a group or `*PUBLIC` to share with);
//! the commands others shared with the user's groups or `*PUBLIC`.

use std::fmt::Write as _;

use qsweb::frame::Page;
use qsweb::html::{esc, query};
use qsweb::http::Response;
use qsweb::msg::Kind;
use qsweb::partials::{Cell, Col, List, actions, input, list_table};

use super::GROUP;
use crate::env::cmd::{How, Saved};
use crate::routes::Rq;
use crate::routes::msgs::confirm;
use crate::{PRODUCT, msgs, policy};

/// The breadcrumb.
const CRUMB: &[&str] = &["Commands", "My commands"];

/// `HH:MM` dates of a last run; empty for never.
fn last(rq: &Rq<'_>, s: &Saved) -> String {
    if s.last == 0 {
        return String::new();
    }
    let (d, t) = rq.stamp(s.last);
    format!("{d} {}", t.get(..5).unwrap_or(&t))
}

/// Run and Prompt of `s`.
fn run_links(rq: &Rq<'_>, s: &Saved) -> Vec<(String, &'static str)> {
    let mut v = vec![(
        PRODUCT.url(&format!(
            "/cmd{}",
            query(&[("c", &s.command), ("how", s.how.code())])
        )),
        "Run",
    )];
    if rq.allowed(policy::CMD_PROMPT) {
        v.push((
            PRODUCT.url(&format!("/cmd/prompt{}", query(&[("cmd", &s.command)]))),
            "Prompt",
        ));
    }
    v
}

/// `GET /cmd/saved`; `POST` changes a command's name and sharing.
#[allow(clippy::too_many_lines)]
pub fn page(rq: &Rq<'_>) -> Response {
    if let Err(r) = rq.policy(policy::CMD_SAVED, CRUMB) {
        return r;
    }
    let cmd = &rq.env().cmd;
    let mut all = cmd.saved(rq.user());
    let groups = rq.env().system.groups(rq.user());
    if rq.post() {
        let f = rq.form();
        if f.has("cancel") {
            return rq.redirect("/cmd/saved");
        }
        if let Err(r) = rq.policy(policy::CMD_SAVE, CRUMB) {
            return r;
        }
        let Some(s) = f
            .str("n")
            .parse::<usize>()
            .ok()
            .and_then(|i| all.get_mut(i))
        else {
            return rq.redirect("/cmd/saved");
        };
        let to = f.str("to").trim();
        let to = if to == NOT_SHARED { "" } else { to };
        if !to.is_empty() && to != "*PUBLIC" && !groups.iter().any(|g| g == to) {
            return rq.done(
                "/cmd/saved",
                rq.message(Kind::Error, msgs::BAD_VALUE, &[to, "Shared with"]),
            );
        }
        let name = f.str("name").trim();
        if !name.is_empty() {
            s.name = name.chars().take(50).collect();
        }
        s.share = to.to_string();
        let shown = s.name.clone();
        return match cmd.put_saved(rq.user(), &all) {
            Ok(()) => rq.done(
                "/cmd/saved",
                rq.message(Kind::Done, msgs::CMD_SAVED, &[&shown]),
            ),
            Err(m) => rq.done("/cmd/saved", m),
        };
    }
    let q = rq.query();
    let mut body = String::new();
    if let Some(i) = q.get("share").and_then(|n| n.parse::<usize>().ok())
        && let Some(s) = all.get(i)
    {
        let mut sel = String::from("<select name=\"to\">");
        for o in std::iter::once(NOT_SHARED)
            .chain(groups.iter().map(String::as_str))
            .chain(std::iter::once("*PUBLIC"))
        {
            let chosen = if o == s.share && o != NOT_SHARED {
                " selected"
            } else {
                ""
            };
            let _ = write!(sel, "<option{chosen}>{}</option>", esc(o));
        }
        sel.push_str("</select>");
        let _ = write!(
            body,
            "<form method=\"post\">{}<input type=\"hidden\" name=\"n\" value=\"{i}\"><p>Name {} Shared with {sel} <input type=\"submit\" name=\"ok\" value=\"Share\"> <input type=\"submit\" name=\"cancel\" value=\"Cancel\"></p></form>",
            rq.ctx.csrf_field(),
            input("name", &s.name, 30)
        );
    }
    let mut order: Vec<usize> = (0..all.len()).collect();
    match q.str("sort") {
        "n" => order.sort_by(|a, b| all[*a].name.cmp(&all[*b].name)),
        "l" => order.sort_by_key(|i| std::cmp::Reverse(all[*i].last)),
        _ => {}
    }
    let change = rq.allowed(policy::CMD_SAVE);
    let rows = order
        .iter()
        .map(|&i| {
            let s = &all[i];
            let mut acts = run_links(rq, s);
            if change {
                acts.push((PRODUCT.url(&format!("/cmd?edit={i}")), "Edit"));
                acts.push((PRODUCT.url(&format!("/cmd/saved/{i}/confirm")), "Delete"));
                acts.push((PRODUCT.url(&format!("/cmd/saved?share={i}")), "Share"));
            }
            vec![
                Cell::act(actions(&acts)),
                Cell::text(&s.name),
                Cell::html(format!("<span class=\"mono\">{}</span>", esc(&s.command))),
                Cell::text(s.how.label()),
                Cell::text(&last(rq, s)),
                Cell::text(&s.share),
            ]
        })
        .collect();
    body.push_str(&list_table(&List {
        cols: vec![
            Col::plain("Actions"),
            Col::sorted("Name", "n"),
            Col::plain("Command"),
            Col::plain("Run as"),
            Col::sorted("Last run", "l"),
            Col::plain("Shared with"),
        ],
        rows,
        empty: "You have no saved commands. Save one from Run command or Prompt.".into(),
        keep: Vec::new(),
    }));
    let mut to = groups.clone();
    to.push("*PUBLIC".into());
    let others = cmd.shared_with(rq.user(), &to);
    if !others.is_empty() {
        let rows = others
            .iter()
            .map(|(owner, s)| {
                vec![
                    Cell::act(actions(&run_links(rq, s))),
                    Cell::text(&s.name),
                    Cell::html(format!("<span class=\"mono\">{}</span>", esc(&s.command))),
                    Cell::text(s.how.label()),
                    Cell::mono(owner),
                    Cell::text(&s.share),
                ]
            })
            .collect();
        body.push_str("<h2>Shared with you</h2>");
        body.push_str(&list_table(&List {
            cols: [
                "Actions",
                "Name",
                "Command",
                "Run as",
                "Owner",
                "Shared with",
            ]
            .iter()
            .map(|c| Col::plain(c))
            .collect(),
            rows,
            empty: String::new(),
            keep: Vec::new(),
        }));
    }
    let _ = write!(
        body,
        "<form method=\"post\" action=\"{}\">{}<div class=\"buttons\"><input type=\"submit\" value=\"New command...\"></div></form>",
        esc(&PRODUCT.url("/cmd")),
        rq.ctx.csrf_field()
    );
    rq.page(Page {
        title: "My commands".into(),
        crumb: CRUMB.iter().map(ToString::to_string).collect(),
        h1: "My commands".into(),
        body,
        help: "mycmds".into(),
        nav: Some((GROUP, Some("/cmd/saved"))),
        ..Page::default()
    })
}

/// The Shared with select's first option.
const NOT_SHARED: &str = "(not shared)";

/// `/cmd/saved/<n>/confirm`: Delete, asked first.
pub fn delete(rq: &Rq<'_>, n: &str) -> Response {
    if let Err(r) = rq.policy(policy::CMD_SAVE, CRUMB) {
        return r;
    }
    let cmd = &rq.env().cmd;
    let mut all = cmd.saved(rq.user());
    let Some(i) = n.parse::<usize>().ok().filter(|i| *i < all.len()) else {
        return rq.redirect("/cmd/saved");
    };
    if rq.post() {
        if rq.form().has("cancel") {
            return rq.redirect("/cmd/saved");
        }
        let gone = all.remove(i);
        return match cmd.put_saved(rq.user(), &all) {
            Ok(()) => rq.done(
                "/cmd/saved",
                rq.message(Kind::Done, msgs::CMD_DELETED, &[&gone.name]),
            ),
            Err(m) => rq.done("/cmd/saved", m),
        };
    }
    let s = &all[i];
    let how = if s.how == How::Batch {
        "Batch"
    } else {
        "Interactive"
    };
    let table = format!(
        "<table class=\"list\" cellspacing=\"0\" cellpadding=\"0\">\n<tr>\n<th>Name</th>\n<th>Command</th>\n<th>Run as</th>\n</tr>\n<tr>\n<td>{}</td>\n<td><span class=\"mono\">{}</span></td>\n<td>{how}</td>\n</tr>\n</table>",
        esc(&s.name),
        esc(&s.command)
    );
    confirm(
        rq,
        "Confirm Delete Saved Command",
        CRUMB,
        "Press OK to delete the saved command listed below, or Cancel to keep it. Users it is shared with no longer see it.",
        &table,
        "",
        (GROUP, Some("/cmd/saved")),
    )
}
