// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/users/create/step/<n>`: the Create user wizard. The answers live in
//! the session (`cu.` keys): Create user... on Users starts a blank
//! wizard, Back and Next keep them, Cancel and Create forget them. The
//! summary shows the `CRTUSRPRF` that Create runs, then optionally
//! `CRTDIR` and `CHGOWN` for the home directory and `CRTMSGQ`.

use std::fmt::Write as _;

use qsweb::frame::Page;
use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::msg::{Kind, Message};
use qsweb::partials::{Button, Prop, PropGroup, checkbox, input, input_cmd, props_table, wizard};

use super::list::CLASSES;
use super::{GROUP, secadm};
use crate::env::users::SPECIAL;
use crate::routes::Rq;
use crate::routes::common::{pick, quote, strings};

/// The steps.
const STEPS: &[&str] = &[
    "Profile and password",
    "User class and groups",
    "Job attributes",
    "Special authorities",
    "Summary",
];

/// The fields each step posts.
const FIELDS: &[&[&str]] = &[
    &["u", "t", "s"],
    &["c", "g", "sg"],
    &["ip", "im", "cl", "lc", "jd", "oq", "hd"],
    &[],
    &[],
];

/// Checkboxes each step posts (absent is off).
const BOXES: &[&[&str]] = &[
    &["e"],
    &[],
    &[],
    &["a1", "a2", "a3", "a4", "a5", "a6", "a7", "a8"],
    &["md", "mq"],
];

/// A field's default.
fn default(k: &str) -> &'static str {
    match k {
        "e" | "md" | "mq" => "on",
        "s" => "*ENABLED",
        "c" => "*USER",
        "g" | "sg" | "ip" => "*NONE",
        "im" => "MAIN",
        "cl" => "*CRTDFT",
        "lc" => "*NO",
        "jd" => "QGPL/QDFTJOBD",
        "oq" => "*WRKSTN",
        _ => "",
    }
}

/// The wizard's answers so far.
struct Answers<'a> {
    rq: &'a Rq<'a>,
}

impl Answers<'_> {
    fn get(&self, k: &str) -> String {
        let v = self.rq.get(&format!("cu.{k}"));
        let set = !self.rq.get(&format!("cu.has.{k}")).is_empty();
        if set || k == "p1" || k == "p2" {
            return v;
        }
        match k {
            "hd" => format!("/home/{}", self.get("u")),
            _ => default(k).to_string(),
        }
    }

    fn on(&self, k: &str) -> bool {
        self.get(k) == "on"
    }

    /// The special authorities chosen.
    fn special(&self) -> Vec<&'static str> {
        SPECIAL
            .iter()
            .enumerate()
            .filter(|(i, _)| self.on(&format!("a{}", i + 1)))
            .map(|(_, s)| *s)
            .collect()
    }

    /// `CRTUSRPRF`; `shown` leaves the password out.
    fn command(&self, shown: bool) -> Vec<String> {
        let u = self.get("u");
        let pw = self.get("p1");
        let pass = if pw.is_empty() {
            "*NONE".to_string()
        } else if shown {
            String::new()
        } else {
            quote(&pw)
        };
        let mut c = vec![
            "CRTUSRPRF".to_string(),
            format!("USRPRF({u})"),
            format!("PASSWORD({pass})"),
            format!("PWDEXP({})", if self.on("e") { "*YES" } else { "*NO" }),
            format!("STATUS({})", self.get("s")),
            format!("USRCLS({})", self.get("c")),
        ];
        for (k, kw) in [("g", "GRPPRF"), ("sg", "SUPGRPPRF"), ("ip", "INLPGM")] {
            let v = self.get(k);
            if !v.is_empty() && v != "*NONE" {
                c.push(format!("{kw}({v})"));
            }
        }
        for (k, kw) in [
            ("im", "INLMNU"),
            ("cl", "CURLIB"),
            ("lc", "LMTCPB"),
            ("jd", "JOBD"),
            ("oq", "OUTQ"),
        ] {
            c.push(format!("{kw}({})", self.get(k)));
        }
        let sp = self.special();
        c.push(format!(
            "SPCAUT({})",
            if sp.is_empty() {
                "*NONE".into()
            } else {
                sp.join(" ")
            }
        ));
        let hd = self.get("hd");
        if !hd.is_empty() {
            c.push(format!("HOMEDIR({})", quote(&hd)));
        }
        let t = self.get("t");
        if !t.is_empty() {
            c.push(format!("TEXT({})", quote(&t)));
        }
        c
    }
}

/// Parameters as the summary prints a command: lines of at most 90
/// characters, continued 10 columns in.
fn wrap(words: &[String]) -> String {
    let mut out = String::new();
    let mut line = String::new();
    for w in words {
        if line.is_empty() {
            line.clone_from(w);
        } else if line.len() + 1 + w.len() > 90 {
            let _ = writeln!(out, "{line}");
            line = format!("          {w}");
        } else {
            line.push(' ');
            line.push_str(w);
        }
    }
    out.push_str(&line);
    out
}

/// The route of step `n`.
fn at(n: usize) -> String {
    format!("/users/create/step/{n}")
}

/// `/users/create/step/<n>`.
pub fn step(rq: &Rq<'_>, n: &str) -> Response {
    let Some(n) = n
        .parse::<usize>()
        .ok()
        .filter(|n| (1..=STEPS.len()).contains(n))
    else {
        return rq.not_found(&format!("wizard step {n}"));
    };
    if rq.post() {
        return post(rq, n);
    }
    let reached = rq.get("cu.reached").parse::<usize>().unwrap_or(1).max(1);
    if n > reached {
        return rq.redirect(&at(reached));
    }
    let a = Answers { rq };
    let body = match n {
        1 => step1(rq, &a),
        2 => step2(rq, &a),
        3 => step3(rq, &a),
        4 => step4(&a),
        _ => step5(&a),
    };
    let bs = match n {
        1 => vec![
            Button::new("back", "< Back").disabled(),
            Button::new("next", "Next >"),
            Button::new("cancel", "Cancel"),
        ],
        5 => vec![
            Button::new("back", "< Back"),
            Button::new("cancel", "Cancel"),
            Button::new("finish", "Create"),
        ],
        _ => vec![
            Button::new("back", "< Back"),
            Button::new("next", "Next >"),
            Button::new("cancel", "Cancel"),
        ],
    };
    let desc = if n == 1 {
        "Step 1 of 5 (CRTUSRPRF).".to_string()
    } else {
        format!("Step {n} of {}.", STEPS.len())
    };
    rq.page(Page {
        title: "Create user".into(),
        crumb: vec![GROUP.into(), "Users".into(), "Create user".into()],
        h1: "Create user".into(),
        desc: Some(desc),
        body: wizard(&rq.ctx, STEPS, n, &body, &bs),
        help: "createuser".into(),
        ..Page::default()
    })
}

/// A step's heading and properties.
fn section(rq: &Rq<'_>, title: &str, rows: Vec<Prop>) -> String {
    format!(
        "<h2>{}</h2>{}",
        esc(title),
        props_table(rq.ctx.product, &[PropGroup { title: None, rows }], false)
    )
}

fn step1(rq: &Rq<'_>, a: &Answers<'_>) -> String {
    section(
        rq,
        STEPS[0],
        vec![
            Prop::new(
                "User profile",
                format!(
                    "{} <span class=\"small grey\">1-10 characters</span>",
                    input("u", &a.get("u"), 10)
                ),
            )
            .required(),
            Prop::new("Text description", input("t", &a.get("t"), 40)),
            Prop::new(
                "Password",
                "<input type=\"password\" name=\"p1\" size=\"12\">".into(),
            ),
            Prop::new(
                "Confirm password",
                "<input type=\"password\" name=\"p2\" size=\"12\">".into(),
            ),
            Prop::new("Set password to expired", checkbox("e", a.on("e"))),
            Prop::new(
                "Status",
                pick("s", &strings(&["*ENABLED", "*DISABLED"]), &a.get("s")),
            ),
        ],
    )
}

fn step2(rq: &Rq<'_>, a: &Answers<'_>) -> String {
    let mut groups = vec!["*NONE".to_string()];
    groups.extend(
        rq.env()
            .users
            .profiles(rq.user())
            .into_iter()
            .filter(|p| p.is_group)
            .map(|p| p.name),
    );
    section(
        rq,
        STEPS[1],
        vec![
            Prop::new("User class", pick("c", &strings(CLASSES), &a.get("c"))),
            Prop::new("Group profile", pick("g", &groups, &a.get("g"))),
            Prop::new("Supplemental groups", input("sg", &a.get("sg"), 30)),
        ],
    )
}

fn step3(rq: &Rq<'_>, a: &Answers<'_>) -> String {
    section(
        rq,
        STEPS[2],
        vec![
            Prop::new("Initial program", input_cmd("ip", &a.get("ip"), 20)),
            Prop::new("Initial menu", input_cmd("im", &a.get("im"), 20)),
            Prop::new("Current library", input_cmd("cl", &a.get("cl"), 10)),
            Prop::new(
                "Limit capabilities",
                pick("lc", &strings(&["*NO", "*PARTIAL", "*YES"]), &a.get("lc")),
            ),
            Prop::new("Job description", input_cmd("jd", &a.get("jd"), 20)),
            Prop::new("Output queue", input_cmd("oq", &a.get("oq"), 20)),
            Prop::new("Home directory", input_cmd("hd", &a.get("hd"), 30)),
        ],
    )
}

fn step4(a: &Answers<'_>) -> String {
    let mut s = String::from("<h2>Special authorities</h2><table class=\"props\">");
    for (i, sp) in SPECIAL.iter().enumerate() {
        let _ = write!(
            s,
            "<tr><th>{sp}</th><td>{}</td></tr>",
            checkbox(&format!("a{}", i + 1), a.on(&format!("a{}", i + 1)))
        );
    }
    s.push_str(
        "</table><p class=\"small grey\">You can give only the special authorities you have.</p>",
    );
    s
}

fn step5(a: &Answers<'_>) -> String {
    let u = a.get("u");
    format!(
        "<h2>Summary</h2><p class=\"small grey\">The following command will be run:</p><pre class=\"out\">{}</pre><table class=\"props\"><tr><th>Also</th><td>{} Create home directory {}<br>{} Create message queue QUSRSYS/{}</td></tr></table>",
        esc(&wrap(&a.command(true))),
        checkbox("md", a.on("md")),
        esc(&a.get("hd")),
        checkbox("mq", a.on("mq")),
        esc(&u)
    )
}

/// Keep step `n`'s posted answers in the session.
fn save(rq: &Rq<'_>, n: usize) {
    let f = rq.form();
    let mut pairs: Vec<(String, String)> = Vec::new();
    for k in FIELDS[n - 1] {
        if f.has(k) {
            let v = f.str(k).trim();
            let v = if *k == "t" || *k == "hd" {
                v.to_string()
            } else {
                v.to_ascii_uppercase()
            };
            pairs.push((format!("cu.{k}"), v));
            pairs.push((format!("cu.has.{k}"), "1".into()));
        }
    }
    for k in BOXES[n - 1] {
        pairs.push((
            format!("cu.{k}"),
            if f.has(k) { "on" } else { "off" }.into(),
        ));
        pairs.push((format!("cu.has.{k}"), "1".into()));
    }
    // An empty password field keeps the one typed before.
    if n == 1 && !(f.str("p1").is_empty() && f.str("p2").is_empty()) {
        pairs.push(("cu.p1".into(), f.str("p1").into()));
        pairs.push(("cu.p2".into(), f.str("p2").into()));
    }
    let refs: Vec<(&str, &str)> = pairs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    rq.set(&refs);
}

/// Step 1's answers checked: the message saying what is wrong.
fn check1(rq: &Rq<'_>, a: &Answers<'_>) -> Option<Message> {
    let u = a.get("u");
    let valid = (1..=10).contains(&u.len())
        && u.starts_with(|c: char| c.is_ascii_uppercase() || "$#@".contains(c))
        && u.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || "$#@_".contains(c));
    if !valid {
        return Some(rq.message(Kind::Error, "CPF2203", &[&u]));
    }
    if rq
        .env()
        .users
        .profiles(rq.user())
        .iter()
        .any(|p| p.name == u)
    {
        return Some(rq.message(Kind::Error, "CPF2214", &[&u]));
    }
    if rq.get("cu.p1") != rq.get("cu.p2") {
        return Some(rq.message(Kind::Error, "CPF2361", &[]));
    }
    None
}

fn post(rq: &Rq<'_>, n: usize) -> Response {
    let f = rq.form();
    if f.has("create") {
        rq.clear("cu.");
        return rq.redirect(&at(1));
    }
    if f.has("refresh") {
        return rq.redirect("/users");
    }
    if f.has("cancel") {
        rq.clear("cu.");
        return rq.redirect("/users");
    }
    save(rq, n);
    if f.has("back") {
        return rq.redirect(&at(n.saturating_sub(1).max(1)));
    }
    let a = Answers { rq };
    if f.has("finish") && n == STEPS.len() {
        return finish(rq, &a);
    }
    if n == 1
        && let Some(m) = check1(rq, &a)
    {
        return rq.done(&at(1), m);
    }
    let next = (n + 1).min(STEPS.len());
    let reached = rq.get("cu.reached").parse::<usize>().unwrap_or(1).max(next);
    rq.set(&[("cu.reached", &reached.to_string())]);
    rq.redirect(&at(next))
}

/// Create: `CRTUSRPRF`, then the home directory and message queue
/// asked for. The wizard is forgotten whatever the outcome.
fn finish(rq: &Rq<'_>, a: &Answers<'_>) -> Response {
    let u = a.get("u");
    if let Err(r) = secadm(
        rq,
        "Create user profile",
        &u,
        &[GROUP, "Users", "Create user"],
    ) {
        return r;
    }
    let users = &rq.env().users;
    let made = users.run(rq.user(), &a.command(false).join(" "));
    let mut m = match made {
        Ok(m) => m,
        Err(m) => return rq.done(&at(STEPS.len()), m),
    };
    let hd = a.get("hd");
    let mut extra = Vec::new();
    if a.on("md") && !hd.is_empty() {
        extra.push(format!("CRTDIR DIR({})", quote(&hd)));
        extra.push(format!("CHGOWN OBJ({}) NEWOWN({u})", quote(&hd)));
    }
    if a.on("mq") {
        extra.push(format!("CRTMSGQ MSGQ(QUSRSYS/{u})"));
    }
    for c in extra {
        if let Err(e) = users.run(rq.user(), &c) {
            m = e;
            break;
        }
    }
    rq.clear("cu.");
    rq.done("/users", m)
}

#[cfg(test)]
mod tests {
    use super::wrap;

    #[test]
    fn summary_lines() {
        let w: Vec<String> = ["CRTUSRPRF", "USRPRF(A)", "TEXT('x')"]
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(wrap(&w), "CRTUSRPRF USRPRF(A) TEXT('x')");
        let long: Vec<String> = (0..12).map(|i| format!("KEYWORD{i}(VALUE)")).collect();
        assert!(wrap(&long).lines().all(|l| l.len() <= 90));
        assert!(
            wrap(&long)
                .lines()
                .nth(1)
                .unwrap()
                .starts_with("          KEYWORD")
        );
    }
}
