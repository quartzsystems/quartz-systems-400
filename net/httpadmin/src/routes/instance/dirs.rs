// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/instance/<name>/directories` (`instance-directories.html`) and
//! `/instance/<name>/directory?path=` (`instance-directory-props.html`):
//! `<Directory>` containers, `Alias` and `ScriptAlias`, their options,
//! access rules and authentication; removing one through a
//! confirmation (`/instance/<name>/remove?path=`).
//!
//! A `<Directory>` under the document root is reached at the URL below
//! it; one elsewhere through an `Alias` naming it, shown on the same row.
//! An alias's own settings live in a `<Location>` for its URL. The
//! server's `<Directory />` guard is not listed.

use std::collections::BTreeSet;

use httpd::conf::{Conf, Directive, Node, Scope, Section};
use qsweb::frame::Page;
use qsweb::html::{esc, query};
use qsweb::http::Response;
use qsweb::msg::Message;
use qsweb::partials::{
    Button, Cell, Col, List, Prop, PropGroup, actions, checkbox, form, input, input_cmd,
    list_table, props,
};

use super::{Rq, bad_value, edit, pick, tab_page};
use crate::routes::instances::{self, View};

/// The options the page offers.
const OPTIONS: &[(&str, &str)] = &[
    ("idx", "Indexes"),
    ("fsl", "FollowSymLinks"),
    ("cgi", "ExecCGI"),
    ("inc", "Includes"),
];

/// What a row is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind2 {
    Directory,
    Alias,
    ScriptAlias,
}

impl Kind2 {
    fn label(self) -> &'static str {
        match self {
            Kind2::Directory => "<Directory>",
            Kind2::Alias => "Alias",
            Kind2::ScriptAlias => "ScriptAlias",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "Directory" => Some(Kind2::Directory),
            "Alias" => Some(Kind2::Alias),
            "ScriptAlias" => Some(Kind2::ScriptAlias),
            _ => None,
        }
    }
}

/// One row.
#[derive(Clone, Debug)]
struct Entry {
    kind: Kind2,
    /// The key in `?path=`: a directory's path, an alias's URL.
    key: String,
    url: String,
    fs: String,
    /// The container holding its settings, if any.
    container: Option<Section>,
}

fn trim(p: &str) -> &str {
    let t = p.trim_end_matches('/');
    if t.is_empty() { "/" } else { t }
}

fn is_alias(d: &Directive) -> bool {
    d.is("Alias") || d.is("ScriptAlias")
}

/// The URL a directory under the document root is reached at.
fn under_docroot(c: &Conf, fs: &str) -> Option<String> {
    let doc = c.value("DocumentRoot");
    let doc = trim(&doc);
    let fs = trim(fs);
    if fs == doc {
        return Some("/".into());
    }
    fs.strip_prefix(doc)
        .and_then(|r| r.strip_prefix('/'))
        .map(|r| format!("/{r}/"))
}

/// The alias naming directory `fs`.
fn alias_of<'a>(c: &'a Conf, fs: &str) -> Option<&'a Directive> {
    c.items.iter().find_map(|n| match n {
        Node::Directive(d) if is_alias(d) && d.args.get(1).is_some_and(|t| trim(t) == trim(fs)) => {
            Some(d)
        }
        _ => None,
    })
}

fn entries(c: &Conf) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut merged = BTreeSet::new();
    for n in &c.items {
        if let Node::Section(s) = n
            && s.name.eq_ignore_ascii_case("Directory")
            && let Some(fs) = s.args.first()
            && trim(fs) != "/"
            && under_docroot(c, fs).is_none()
            && let Some(a) = alias_of(c, fs)
        {
            merged.insert(a.args[0].clone());
        }
    }
    for n in &c.items {
        match n {
            Node::Section(s) if s.name.eq_ignore_ascii_case("Directory") => {
                let Some(fs) = s.args.first() else { continue };
                if trim(fs) == "/" {
                    continue;
                }
                let url = under_docroot(c, fs)
                    .or_else(|| alias_of(c, fs).map(|a| a.args[0].clone()))
                    .unwrap_or_default();
                out.push(Entry {
                    kind: Kind2::Directory,
                    key: fs.clone(),
                    url,
                    fs: fs.clone(),
                    container: Some(s.clone()),
                });
            }
            Node::Directive(d)
                if is_alias(d) && d.args.len() == 2 && !merged.contains(&d.args[0]) =>
            {
                out.push(Entry {
                    kind: if d.is("ScriptAlias") {
                        Kind2::ScriptAlias
                    } else {
                        Kind2::Alias
                    },
                    key: d.args[0].clone(),
                    url: d.args[0].clone(),
                    fs: d.args[1].clone(),
                    container: c.section("Location", &d.args[0]).cloned(),
                });
            }
            _ => {}
        }
    }
    out
}

fn find(c: &Conf, key: &str) -> Option<Entry> {
    entries(c).into_iter().find(|e| e.key == key)
}

/// The options an entry's container states, else what it inherits.
fn options_text(c: &Conf, e: &Entry) -> String {
    let own = e
        .container
        .as_ref()
        .map(|s| s.value("Options"))
        .unwrap_or_default();
    match e.kind {
        _ if !own.is_empty() => own,
        Kind2::Directory => c.value("Options"),
        Kind2::ScriptAlias => "ExecCGI".into(),
        Kind2::Alias => String::new(),
    }
}

/// Apply an `Options` value to a set.
fn apply_options(set: &mut BTreeSet<String>, value: &str) {
    let words: Vec<&str> = value.split_whitespace().collect();
    if words.iter().any(|w| !w.starts_with(['+', '-'])) {
        set.clear();
    }
    for w in words {
        let (on, name) = match w.split_at(usize::from(w.starts_with(['+', '-']))) {
            ("-", n) => (false, n),
            (_, n) => (true, n),
        };
        let names: Vec<&str> = match name {
            "All" => OPTIONS
                .iter()
                .map(|o| o.1)
                .filter(|o| *o != "Includes")
                .collect(),
            "None" => {
                set.clear();
                continue;
            }
            n => vec![n],
        };
        for n in names {
            if on {
                set.insert(n.to_string());
            } else {
                set.remove(n);
            }
        }
    }
}

/// What an entry inherits: the server's options, `ExecCGI` for a
/// script alias.
fn inherited(c: &Conf, kind: Kind2) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    if kind == Kind2::Directory {
        apply_options(&mut set, &c.value("Options"));
    } else if kind == Kind2::ScriptAlias {
        set.insert("ExecCGI".into());
    }
    set
}

fn effective(c: &Conf, e: &Entry) -> BTreeSet<String> {
    let mut set = inherited(c, e.kind);
    if let Some(s) = &e.container {
        apply_options(&mut set, &s.value("Options"));
    }
    set
}

/// `Allow from ...; Deny from ...` in file order.
fn access(e: &Entry) -> String {
    let rules: Vec<String> = e
        .container
        .iter()
        .flat_map(|s| s.items.iter())
        .filter_map(|n| match n {
            Node::Directive(d) if d.is("Allow") || d.is("Deny") => Some(format!(
                "{} from {}",
                if d.is("Allow") { "Allow" } else { "Deny" },
                d.args[1..].join(" ")
            )),
            _ => None,
        })
        .collect();
    if rules.is_empty() {
        "Allow from all".into()
    } else {
        rules.join("; ")
    }
}

fn props_url(name: &str, key: &str) -> String {
    format!(
        "{}{}",
        instances::url(name, "directory"),
        query(&[("path", key)])
    )
}

fn remove_url(name: &str, key: &str) -> String {
    format!(
        "{}{}",
        instances::url(name, "remove"),
        query(&[("path", key)])
    )
}

/// `GET /directories` lists; `POST` (Add...) opens a new entry's page.
pub fn list(rq: &Rq<'_>, v: &View) -> Response {
    let n = v.name();
    if rq.post() {
        let kind = Kind2::parse(rq.form().str("newtype")).unwrap_or(Kind2::Directory);
        let t = match kind {
            Kind2::Directory => "Directory",
            Kind2::Alias => "Alias",
            Kind2::ScriptAlias => "ScriptAlias",
        };
        return Response::redirect(&format!(
            "{}{}",
            instances::url(n, "directory"),
            query(&[("new", t)])
        ));
    }
    let empty = Conf::default();
    let c = v.conf.as_ref().unwrap_or(&empty);
    let mut es = entries(c);
    match rq.query().str("sort") {
        "type" => es.sort_by_key(|e| e.kind.label()),
        "path" => es.sort_by(|a, b| a.url.cmp(&b.url)),
        "fs" => es.sort_by(|a, b| a.fs.cmp(&b.fs)),
        _ => {}
    }
    let rows = es
        .iter()
        .map(|e| {
            vec![
                Cell::act(actions(&[
                    (props_url(n, &e.key), "Properties"),
                    (remove_url(n, &e.key), "Remove"),
                ])),
                Cell::text(e.kind.label()),
                Cell::mono(&e.url),
                Cell::mono(&e.fs),
                Cell::mono(&options_text(c, e)),
                Cell::text(&access(e)),
            ]
        })
        .collect();
    let table = list_table(&List {
        cols: vec![
            Col::plain("Actions"),
            Col::sorted("Type", "type"),
            Col::sorted("URL path / name", "path"),
            Col::sorted("File system path", "fs"),
            Col::plain("Options"),
            Col::plain("Access"),
        ],
        rows,
        empty: "The instance has no directories or aliases.".into(),
        keep: Vec::new(),
    });
    let add = form(
        &rq.ctx,
        "",
        &format!(
            "<div class=\"buttons\">Add: {} <input type=\"submit\" value=\"Add...\"></div>",
            pick("newtype", &["Directory", "Alias", "ScriptAlias"], "")
        ),
    );
    rq.page(tab_page(
        v,
        "directories",
        Some("Directory containers, Alias and ScriptAlias directives."),
        &format!("{table}{add}"),
    ))
}

/// The form's view of an entry (existing or new).
struct Fields {
    kind: Kind2,
    fs: String,
    url: String,
    opts: BTreeSet<String>,
    index: String,
    over: String,
    order: String,
    allow: String,
    deny: String,
    auth: bool,
    realm: String,
    vldl: String,
}

fn hosts(s: Option<&Section>, name: &str) -> String {
    s.map(|s| {
        s.all(name)
            .iter()
            .map(|d| d.args[1..].join(" "))
            .collect::<Vec<_>>()
            .join(" ")
    })
    .unwrap_or_default()
}

fn fields(c: &Conf, e: &Entry) -> Fields {
    let s = e.container.as_ref();
    let own = |n: &str| s.map(|s| s.value(n)).unwrap_or_default();
    let or = |a: String, b: String| if a.is_empty() { b } else { a };
    Fields {
        kind: e.kind,
        fs: e.fs.clone(),
        url: e.url.clone(),
        opts: effective(c, e),
        index: or(own("DirectoryIndex"), c.value("DirectoryIndex")),
        over: or(own("AllowOverride"), "None".into()),
        order: or(own("Order"), "Deny,Allow".into()),
        allow: hosts(s, "Allow"),
        deny: hosts(s, "Deny"),
        auth: !own("AuthType").is_empty(),
        realm: own("AuthName"),
        vldl: own("PasswdFile"),
    }
}

/// `/directory?path=` or `?new=Directory|Alias|ScriptAlias`.
#[allow(clippy::too_many_lines)]
pub fn props_page(rq: &Rq<'_>, v: &View) -> Response {
    let q = rq.query();
    let empty = Conf::default();
    let c = v.conf.as_ref().unwrap_or(&empty);
    let (f, key) = if let Some(kind) = q.get("new").and_then(Kind2::parse) {
        let e = Entry {
            kind,
            key: String::new(),
            url: String::new(),
            fs: String::new(),
            container: None,
        };
        (fields(c, &e), None)
    } else {
        let key = q.str("path").to_string();
        let Some(e) = find(c, &key) else {
            return rq.not_found(&format!(
                "directory or alias {key} in instance {}",
                v.name()
            ));
        };
        (fields(c, &e), Some(key))
    };
    if rq.post() {
        return post(rq, v, key.as_deref(), f.kind);
    }
    let shown = if f.url.is_empty() {
        f.fs.clone()
    } else {
        f.url.clone()
    };
    let shown = if key.is_none() {
        format!("New {}", f.kind.label())
    } else {
        shown
    };
    let opts = OPTIONS
        .iter()
        .map(|(n, l)| format!("{} {l}", checkbox(n, f.opts.contains(*l))))
        .collect::<Vec<_>>()
        .join("<br>");
    let auth = format!(
        "{} Realm: {}<br>Validation list: {}",
        checkbox("auth", f.auth),
        input("realm", &f.realm, 30),
        input_cmd("vldl", &f.vldl, 20)
    );
    let group = match f.kind {
        Kind2::Directory => "Directory",
        Kind2::Alias => "Alias",
        Kind2::ScriptAlias => "Script alias",
    };
    let groups = [
        PropGroup {
            title: Some(group.into()),
            rows: vec![
                Prop::new("Type", esc(f.kind.label())),
                Prop::new("File system path", input_cmd("fs", &f.fs, 45)),
                Prop::new("URL path", input_cmd("url", &f.url, 30)),
            ],
        },
        PropGroup {
            title: Some("Options".into()),
            rows: vec![
                Prop::new("Options", opts),
                Prop::new("Directory index", input_cmd("di", &f.index, 30)),
                Prop::new(
                    "AllowOverride",
                    pick("ao", &["None", "All", "AuthConfig", "Limit"], &f.over),
                ),
            ],
        },
        PropGroup {
            title: Some("Access".into()),
            rows: vec![
                Prop::new(
                    "Order",
                    pick("order", &["Deny,Allow", "Allow,Deny"], &f.order),
                ),
                Prop::new("Allow from", input_cmd("allow", &f.allow, 30)),
                Prop::new("Deny from", input_cmd("deny", &f.deny, 30)),
                Prop::new("Require authentication", auth),
            ],
        },
    ];
    let body = props(
        &rq.ctx,
        &groups,
        &[
            Button::new("apply", "Apply"),
            Button::new("ok", "OK"),
            Button::new("cancel", "Cancel"),
        ],
    );
    let title = format!("Directory: {shown}");
    rq.page(Page {
        title: title.clone(),
        crumb: vec![
            "Manage".into(),
            "Server instances".into(),
            v.name().into(),
            "Directories & aliases".into(),
            shown,
        ],
        h1: title,
        body,
        help: "instance-directory".into(),
        ..Page::default()
    })
}

/// The section `name` `arg` at top level, to change.
fn section_mut<'a>(c: &'a mut Conf, name: &str, arg: &str) -> Option<&'a mut Section> {
    c.items.iter_mut().find_map(|n| match n {
        Node::Section(s)
            if s.name.eq_ignore_ascii_case(name)
                && s.args.first().is_some_and(|a| trim(a) == trim(arg)) =>
        {
            Some(s)
        }
        _ => None,
    })
}

/// The alias directive for URL `url`, to change.
fn alias_mut<'a>(c: &'a mut Conf, url: &str) -> Option<&'a mut Directive> {
    c.items.iter_mut().find_map(|n| match n {
        Node::Directive(d) if is_alias(d) && d.args.first().map(String::as_str) == Some(url) => {
            Some(d)
        }
        _ => None,
    })
}

/// What the form asks for, checked.
struct Wanted {
    fs: String,
    url: String,
    opts: BTreeSet<String>,
    index: String,
    over: String,
    order: String,
    allow: Vec<String>,
    deny: Vec<String>,
    auth: Option<(String, String)>,
}

fn wanted(rq: &Rq<'_>, kind: Kind2) -> Result<Wanted, Message> {
    let f = rq.form();
    let fs = f.str("fs").trim().to_string();
    if !fs.starts_with('/') {
        return Err(bad_value(
            &fs,
            "File system path",
            "an absolute path name starting with /",
        ));
    }
    let url = f.str("url").trim().to_string();
    if (!url.is_empty() || kind != Kind2::Directory) && !url.starts_with('/') {
        return Err(bad_value(&url, "URL path", "a path starting with /"));
    }
    let over = f.str("ao");
    let over = if ["None", "All", "AuthConfig", "Limit"].contains(&over) {
        over
    } else {
        "None"
    };
    let order = if f.str("order") == "Allow,Deny" {
        "Allow,Deny"
    } else {
        "Deny,Allow"
    };
    let auth = if f.has("auth") {
        let realm = f.str("realm").trim().to_string();
        let vldl = f.str("vldl").trim().to_ascii_uppercase();
        if realm.is_empty() {
            return Err(bad_value(
                "",
                "Realm",
                "the name shown when the browser asks for a password",
            ));
        }
        let ok = vldl.split_once('/').is_some_and(|(l, n)| {
            [l, n].iter().all(|p| {
                !p.is_empty()
                    && p.len() <= 10
                    && p.chars()
                        .all(|c| c.is_ascii_alphanumeric() || "$#@_".contains(c))
            })
        });
        if !ok {
            return Err(bad_value(
                &vldl,
                "Validation list",
                "a qualified object name, LIBRARY/NAME",
            ));
        }
        Some((realm, vldl))
    } else {
        None
    };
    let words = |n: &str| {
        f.str(n)
            .split_whitespace()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    };
    Ok(Wanted {
        fs,
        url,
        opts: OPTIONS
            .iter()
            .filter(|(n, _)| f.has(n))
            .map(|(_, l)| (*l).to_string())
            .collect(),
        index: f.str("di").split_whitespace().collect::<Vec<_>>().join(" "),
        over: over.into(),
        order: order.into(),
        allow: words("allow"),
        deny: words("deny"),
        auth,
    })
}

/// Write `w`'s settings into container `s`; `inherit` is what it gets
/// without them, `index` the server's directory index.
fn settle(s: &mut Section, w: &Wanted, inherit: &BTreeSet<String>, index: &str) {
    let mut opts: Vec<String> = Vec::new();
    for (_, o) in OPTIONS {
        match (w.opts.contains(*o), inherit.contains(*o)) {
            (true, false) => opts.push(format!("+{o}")),
            (false, true) => opts.push(format!("-{o}")),
            _ => {}
        }
    }
    if opts.is_empty() {
        s.unset("Options");
    } else {
        s.set(
            "Options",
            &opts.iter().map(String::as_str).collect::<Vec<_>>(),
        );
    }
    if w.index.is_empty() || w.index == index {
        s.unset("DirectoryIndex");
    } else {
        s.set("DirectoryIndex", &w.index.split(' ').collect::<Vec<_>>());
    }
    if w.over == "None" {
        s.unset("AllowOverride");
    } else {
        s.set("AllowOverride", &[&w.over]);
    }
    s.unset("Allow");
    s.unset("Deny");
    if w.allow.is_empty() && w.deny.is_empty() {
        s.unset("Order");
    } else {
        s.set("Order", &[&w.order]);
    }
    let first_deny = w.order == "Deny,Allow";
    let add = |s: &mut Section, name: &str, hosts: &[String]| {
        if !hosts.is_empty() {
            let mut args = vec!["From"];
            args.extend(hosts.iter().map(String::as_str));
            s.add(Directive::new(name, &args));
        }
    };
    if first_deny {
        add(s, "Deny", &w.deny);
        add(s, "Allow", &w.allow);
    } else {
        add(s, "Allow", &w.allow);
        add(s, "Deny", &w.deny);
    }
    match &w.auth {
        Some((realm, vldl)) => {
            s.set("AuthType", &["Basic"]);
            s.set("AuthName", &[realm]);
            s.set("PasswdFile", &[vldl]);
            if s.get("Require").is_none() {
                s.set("Require", &["valid-user"]);
            }
        }
        None => {
            for d in ["AuthType", "AuthName", "PasswdFile", "Require"] {
                s.unset(d);
            }
        }
    }
}

fn has_directives(s: &Section) -> bool {
    s.items
        .iter()
        .any(|n| !matches!(n, Node::Blank(_) | Node::Comment(_)))
}

fn post(rq: &Rq<'_>, v: &View, key: Option<&str>, kind: Kind2) -> Response {
    let form = rq.form();
    let here = format!("{}?{}", instances::url(v.name(), "directory"), rq.req.query);
    if !(form.has("apply") || form.has("ok")) {
        return Response::redirect(&instances::url(v.name(), "directories"));
    }
    if let Err(refusal) = super::may_change(rq, v, "Change HTTP server directories") {
        return refusal;
    }
    let want = match wanted(rq, kind) {
        Ok(want) => want,
        Err(msg) => {
            rq.flash(msg);
            return Response::redirect(&here);
        }
    };
    let new_key = if kind == Kind2::Directory {
        want.fs.clone()
    } else {
        want.url.clone()
    };
    match edit(rq, v, |c| change(c, key, kind, &want)) {
        Ok(msg) if form.has("ok") => rq.done(&format!("/instance/{}/directories", v.name()), msg),
        Ok(msg) => {
            rq.flash(msg);
            Response::redirect(&props_url(v.name(), &new_key))
        }
        Err(msg) => {
            rq.flash(msg);
            Response::redirect(&here)
        }
    }
}

fn exists(c: &Conf, kind: Kind2, key: &str) -> bool {
    match kind {
        Kind2::Directory => c.section("Directory", key).is_some() || section_has(c, key),
        _ => c.items.iter().any(|n| matches!(n, Node::Directive(d) if is_alias(d) && d.args.first().map(String::as_str) == Some(key))),
    }
}

fn section_has(c: &Conf, fs: &str) -> bool {
    c.sections("Directory")
        .iter()
        .any(|s| s.args.first().is_some_and(|a| trim(a) == trim(fs)))
}

/// Make the change to the configuration.
fn change(c: &mut Conf, key: Option<&str>, kind: Kind2, w: &Wanted) -> Result<(), Message> {
    let new_key = if kind == Kind2::Directory {
        w.fs.as_str()
    } else {
        w.url.as_str()
    };
    if key.is_none_or(|k| trim(k) != trim(new_key)) && exists(c, kind, new_key) {
        return Err(bad_value(
            new_key,
            "File system path or URL path",
            "a path no other entry of the instance uses",
        ));
    }
    let server_index = c.value("DirectoryIndex");
    let inherit = inherited(c, kind);
    match kind {
        Kind2::Directory => {
            let old_fs = key.map(ToString::to_string);
            let old_alias = old_fs
                .as_deref()
                .and_then(|fs| alias_of(c, fs))
                .map(|a| a.args[0].clone());
            match old_fs
                .as_deref()
                .and_then(|fs| section_mut(c, "Directory", fs))
            {
                Some(s) => {
                    if trim(&s.args[0]) != trim(&w.fs) {
                        s.args = vec![w.fs.clone()];
                        s.raw = None;
                    }
                }
                None => {
                    c.items
                        .push(Node::Section(Section::new("Directory", &[&w.fs])));
                }
            }
            if let Some(s) = section_mut(c, "Directory", &w.fs) {
                settle(s, w, &inherit, &server_index);
            }
            // The URL: the document root's, or an alias.
            let derived = under_docroot(c, &w.fs);
            let want_alias = !w.url.is_empty() && derived.as_deref() != Some(w.url.as_str());
            let target = format!("{}/", trim(&w.fs));
            match (old_alias, want_alias) {
                (Some(u), true) => {
                    if let Some(d) = alias_mut(c, &u) {
                        d.args = vec![w.url.clone(), target];
                        d.raw = None;
                    }
                }
                (Some(u), false) => {
                    c.remove_where(|d| is_alias(d) && d.args.first() == Some(&u));
                }
                (None, true) => c.add(Directive::new("Alias", &[&w.url, &target])),
                (None, false) => {}
            }
        }
        Kind2::Alias | Kind2::ScriptAlias => {
            if let Some(d) = key.and_then(|k| alias_mut(c, k)) {
                d.args = vec![w.url.clone(), w.fs.clone()];
                d.raw = None;
            } else {
                let name = if kind == Kind2::Alias {
                    "Alias"
                } else {
                    "ScriptAlias"
                };
                c.add(Directive::new(name, &[&w.url, &w.fs]));
            }
            let old = key.unwrap_or(&w.url).to_string();
            let mut loc = section_mut(c, "Location", &old)
                .cloned()
                .unwrap_or_else(|| Section::new("Location", &[&w.url]));
            if loc.args[0] != w.url {
                loc.args = vec![w.url.clone()];
                loc.raw = None;
            }
            settle(&mut loc, w, &inherit, &server_index);
            c.remove_section("Location", &old);
            if has_directives(&loc) {
                c.items.push(Node::Section(loc));
            }
        }
    }
    Ok(())
}

/// `/remove?path=`: the confirmation, and on OK the removal.
pub fn remove(rq: &Rq<'_>, v: &View) -> Response {
    let key = rq.query().str("path").to_string();
    let empty = Conf::default();
    let c = v.conf.as_ref().unwrap_or(&empty);
    let Some(e) = find(c, &key) else {
        return rq.not_found(&format!(
            "directory or alias {key} in instance {}",
            v.name()
        ));
    };
    let back = format!("/instance/{}/directories", v.name());
    if rq.post() {
        if !rq.form().has("ok") {
            return rq.redirect(&back);
        }
        if let Err(r) = super::may_change(rq, v, "Change HTTP server directories") {
            return r;
        }
        let m = edit(rq, v, |c| {
            let url = e.url.clone();
            if e.kind == Kind2::Directory {
                c.remove_section("Directory", &e.key);
                if trim(&e.key) != e.key {
                    c.remove_section("Directory", trim(&e.key));
                }
                c.remove_where(|d| {
                    is_alias(d)
                        && d.args.first() == Some(&url)
                        && d.args.get(1).is_some_and(|t| trim(t) == trim(&e.fs))
                });
            } else {
                c.remove_where(|d| is_alias(d) && d.args.first() == Some(&url));
                c.remove_section("Location", &url);
            }
            Ok(())
        })
        .unwrap_or_else(|e| e);
        return rq.done(&back, m);
    }
    let table = list_table(&List {
        cols: ["Type", "URL path / name", "File system path"]
            .into_iter()
            .map(Col::plain)
            .collect(),
        rows: vec![vec![
            Cell::text(e.kind.label()),
            Cell::mono(&e.url),
            Cell::mono(&e.fs),
        ]],
        ..List::default()
    });
    let body = format!(
        "<p>{}</p>{table}{}",
        esc(
            "Removing an entry deletes its directives from the configuration file. Press OK to remove the entry listed below, or Cancel to return without removing it."
        ),
        form(
            &rq.ctx,
            "",
            &qsweb::partials::buttons(&[Button::new("ok", "OK"), Button::new("cancel", "Cancel")])
        )
    );
    rq.page(Page {
        title: "Confirm Remove".into(),
        crumb: vec![
            "Manage".into(),
            "Server instances".into(),
            v.name().into(),
            "Directories & aliases".into(),
            "Confirm Remove".into(),
        ],
        h1: "Confirm Remove".into(),
        body,
        help: "instance-directories".into(),
        ..Page::default()
    })
}
