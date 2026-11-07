// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Policies (`/policies?p=`): the effective view of one profile, group
//! or `*PUBLIC`, and its own settings changed. Needs `*SECADM` (policy
//! "Change policies", which the system sets). `/policies/copy?p=`
//! gives the profile another one's settings.

use std::fmt::Write as _;

use qsweb::frame::Page;
use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::msg::Kind;
use qsweb::partials::{Button, buttons, form};

use super::GROUP;
use crate::env::Profile;
use crate::env::policy::Setting;
use crate::msgs;
use crate::policy::{self, Effective, Level, PUBLIC, SECTIONS};
use crate::routes::Rq;

const CRUMB: &[&str] = &[GROUP, "Policies"];

/// The profiles the select offers: users of class `*USER`, then the
/// groups, then `*PUBLIC`; `shown` first when it is another profile.
fn subjects(rq: &Rq<'_>, shown: Option<&str>) -> Vec<(String, &'static str)> {
    let all: Vec<Profile> = rq.env().system.profiles(rq.user());
    let mut out: Vec<(String, &'static str)> = Vec::new();
    if let Some(s) = shown.filter(|s| *s != PUBLIC)
        && let Some(p) = all.iter().find(|p| p.name == s)
        && !p.group
        && p.class != "*USER"
    {
        out.push((p.name.clone(), "user"));
    }
    out.extend(
        all.iter()
            .filter(|p| !p.group && p.class == "*USER")
            .map(|p| (p.name.clone(), "user")),
    );
    out.extend(
        all.iter()
            .filter(|p| p.group)
            .map(|p| (p.name.clone(), "group")),
    );
    out.push((PUBLIC.into(), ""));
    out
}

/// `WEBUSER (user)`, `*PUBLIC`.
fn label(s: &(String, &str)) -> String {
    if s.1.is_empty() {
        s.0.clone()
    } else {
        format!("{} ({})", s.0, s.1)
    }
}

/// The subject a select's option or the query names.
fn named(v: &str) -> String {
    v.split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_uppercase()
}

/// The setting of every function for `subject`, and what it inherits
/// without its own settings.
fn view(rq: &Rq<'_>, subject: &str, group: bool) -> (Vec<Level>, bool) {
    let env = rq.env();
    let lv = policy::levels(env, subject, group);
    let secadm = subject != PUBLIC && policy::secadm(env, subject);
    (lv, secadm)
}

fn describe(rq: &Rq<'_>, subject: &str, group: bool) -> String {
    if subject == PUBLIC {
        return "Policies of *PUBLIC: every profile without a setting of its own.".into();
    }
    if group {
        return format!("Effective policies for group {subject}, including *PUBLIC.");
    }
    let groups = rq.env().system.groups(subject);
    match groups.as_slice() {
        [] => format!("Effective policies for {subject}."),
        [g] => format!("Effective policies for {subject}, including group {g}."),
        [first @ .., last] => format!(
            "Effective policies for {subject}, including groups {} and {last}.",
            first.join(", ")
        ),
    }
}

fn radio(name: &str, value: char, checked: bool) -> String {
    format!(
        "<input type=\"radio\" name=\"{name}\" value=\"{value}\"{}> ",
        if checked { " checked" } else { "" }
    )
}

/// The page for `subject`.
fn show(rq: &Rq<'_>, subject: &str) -> Response {
    let opts = subjects(rq, Some(subject));
    let Some(at) = opts.iter().position(|o| o.0 == subject) else {
        return rq.not_found(&format!("user profile {subject}"));
    };
    let group = opts[at].1 == "group";
    let (lv, secadm) = view(rq, subject, group);
    let mut body = String::from(
        "<table class=\"msg\"><tr><td class=\"info\">Policies are checked in this order: user profile, group profiles, *PUBLIC. A Deny at any level denies. The user's object authority on the system still applies.</td></tr></table>",
    );
    let mut inner = String::from(
        "<table class=\"filter\"><tr><td>Profile or group:</td><td><select name=\"p\">",
    );
    for (i, o) in opts.iter().enumerate() {
        let sel = if i > 0 && i == at { " selected" } else { "" };
        let _ = write!(inner, "<option{sel}>{}</option>", esc(&label(o)));
    }
    let _ = write!(
        inner,
        "</select></td><td><input type=\"submit\" value=\"Go\"></td><td class=\"small grey\">&nbsp; {}</td></tr></table>",
        esc(&describe(rq, subject, group))
    );
    inner.push_str("<table class=\"list\" width=\"100%\" cellspacing=\"0\" cellpadding=\"0\">\n<tr>\n<th>Function</th>\n<th>Allow</th>\n<th>Deny</th>\n<th>Set by</th>\n</tr>\n");
    let mut n = 0usize;
    let mut row = |s: &mut String, cells: [String; 4]| {
        s.push_str(if n % 2 == 1 {
            "<tr class=\"alt\">\n"
        } else {
            "<tr>\n"
        });
        let _ = write!(
            s,
            "<td>{}</td>\n<td class=\"radio\">{}</td>\n<td class=\"radio\">{}</td>\n<td>{}</td>\n</tr>\n",
            cells[0], cells[1], cells[2], cells[3]
        );
        n += 1;
    };
    for sec in SECTIONS {
        row(
            &mut inner,
            [
                format!("<b>{}</b>", esc(sec.label)),
                String::new(),
                String::new(),
                String::new(),
            ],
        );
        for (i, (key, text)) in sec.functions.iter().enumerate() {
            let e: Effective = policy::resolve(&lv, key, secadm);
            let name = format!("p_{}_{i}", sec.key);
            let allow = e.setting == Setting::Allow;
            row(
                &mut inner,
                [
                    esc(text),
                    radio(&name, 'A', allow),
                    radio(&name, 'D', !allow),
                    format!("<span class=\"small grey\">{}</span>", esc(&e.by.shown())),
                ],
            );
        }
    }
    inner.push_str("</table>");
    inner.push_str(&buttons(&[
        Button::new("apply", "Apply"),
        Button::new("ok", "OK"),
        Button::new("cancel", "Cancel"),
        Button::new("copy", "Copy policies from..."),
    ]));
    body.push_str(&form(&rq.ctx, "", &inner));
    rq.page(Page {
        title: "Policies".into(),
        crumb: CRUMB.iter().map(ToString::to_string).collect(),
        h1: "Policies".into(),
        desc: Some(
            "Administrator: allow or deny each Access for Web function per profile or group."
                .into(),
        ),
        body,
        help: "policies".into(),
        nav: Some((GROUP, Some("/policies"))),
        ..Page::default()
    })
}

/// The settings to keep for `subject` from the posted radios: those
/// that differ from what it inherits.
fn posted(rq: &Rq<'_>, subject: &str) -> Vec<(String, Setting)> {
    let group = subjects(rq, Some(subject))
        .iter()
        .any(|o| o.0 == subject && o.1 == "group");
    let (mut lv, secadm) = view(rq, subject, group);
    if subject == PUBLIC {
        lv.clear();
    } else {
        lv.remove(0);
    }
    let f = rq.form();
    let mut out = Vec::new();
    for sec in SECTIONS {
        for (i, (key, _)) in sec.functions.iter().enumerate() {
            if *key == policy::POLICIES_CHANGE {
                continue;
            }
            let Some(v) = Setting::from_code(f.str(&format!("p_{}_{i}", sec.key))) else {
                continue;
            };
            if policy::resolve(&lv, key, secadm).setting != v {
                out.push(((*key).to_string(), v));
            }
        }
    }
    out
}

fn copy(rq: &Rq<'_>, subject: &str) -> Response {
    let opts: Vec<(String, &str)> = subjects(rq, Some(subject))
        .into_iter()
        .filter(|o| o.0 != subject)
        .collect();
    if rq.post() {
        let f = rq.form();
        if f.has("ok") {
            let from = named(f.str("from"));
            if !opts.iter().any(|o| o.0 == from) {
                return rq.not_found(&format!("user profile {from}"));
            }
            let settings = rq.env().policy.settings(&from);
            if let Err(m) = rq.env().policy.put(subject, &settings) {
                return rq.done(&format!("/policies?p={subject}"), m);
            }
            let m = rq.message(Kind::Done, msgs::POLICIES_CHANGED, &[subject]);
            return rq.done(&format!("/policies?p={subject}"), m);
        }
        return rq.redirect(&format!("/policies?p={subject}"));
    }
    let mut sel = String::from("<select name=\"from\">");
    for o in &opts {
        let _ = write!(sel, "<option>{}</option>", esc(&label(o)));
    }
    sel.push_str("</select>");
    let inner = format!(
        "<table class=\"props\" cellspacing=\"0\">\n<tr><th>Copy to</th><td><span class=\"mono\">{}</span></td></tr>\n<tr><th>Copy from</th><td>{sel}</td></tr>\n</table>\n{}",
        esc(subject),
        buttons(&[Button::new("ok", "OK"), Button::new("cancel", "Cancel")])
    );
    let body = format!(
        "<p>The settings of the profile chosen replace those of {}. Settings it inherits from its groups and *PUBLIC stay as they are.</p>{}",
        esc(subject),
        form(&rq.ctx, "", &inner)
    );
    let mut crumb: Vec<String> = CRUMB.iter().map(ToString::to_string).collect();
    crumb.push("Copy policies".into());
    rq.page(Page {
        title: "Copy policies".into(),
        crumb,
        h1: "Copy policies".into(),
        body,
        help: "policies".into(),
        nav: Some((GROUP, None)),
        ..Page::default()
    })
}

/// `/policies`, `/policies/copy`.
pub fn route(rq: &Rq<'_>, segs: &[&str]) -> Response {
    let mut crumb = CRUMB.to_vec();
    if segs.len() > 1 {
        crumb.push("Copy policies");
    }
    if let Err(r) = rq.policy(policy::POLICIES_CHANGE, &crumb) {
        return r;
    }
    let q = rq.query();
    let subject = match q.get("p").map(named).filter(|s| !s.is_empty()) {
        Some(s) => s,
        None => subjects(rq, None)
            .first()
            .map_or_else(|| PUBLIC.to_string(), |s| s.0.clone()),
    };
    match segs {
        ["policies"] => {}
        ["policies", "copy"] => return copy(rq, &subject),
        _ => return rq.not_found("page"),
    }
    if !rq.post() {
        return show(rq, &subject);
    }
    let f = rq.form();
    if f.has("cancel") {
        return rq.redirect("/home");
    }
    if f.has("copy") {
        return rq.redirect(&format!("/policies/copy?p={subject}"));
    }
    if !(f.has("apply") || f.has("ok")) {
        // Go: another profile.
        return rq.redirect(&format!("/policies?p={}", named(f.str("p"))));
    }
    let entries = posted(rq, &subject);
    if let Err(m) = rq.env().policy.put(&subject, &entries) {
        return rq.done(&format!("/policies?p={subject}"), m);
    }
    let m = rq.message(Kind::Done, msgs::POLICIES_CHANGED, &[&subject]);
    let back = if f.has("ok") {
        "/home".to_string()
    } else {
        format!("/policies?p={subject}")
    };
    rq.done(&back, m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::SetBy;

    #[test]
    fn names() {
        assert_eq!(named("WEBUSERS (group)"), "WEBUSERS");
        assert_eq!(named("*public"), "*PUBLIC");
        assert_eq!(label(&("A".into(), "user")), "A (user)");
        assert_eq!(SetBy::Group("G".into()).shown(), "Group G");
    }
}
