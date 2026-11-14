// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/dirsrv/inst/<name>/suffixes`: the suffixes by backend, Add (a user
//! suffix of the directory's backend) and `remove?dn=` (confirmed);
//! `/dirsrv/inst/<name>/indexes`: the attribute indexes, Add or change
//! one and `remove?attr=` (confirmed). Both take effect at the
//! instance's next start.

use qsweb::http::Response;
use qsweb::partials::{
    Button, Cell, Col, List, actions, buttons, checkbox, form, input, list_table, mono,
};

use super::{changed, confirm, enc, fail, inst_route, inst_tabs, need, page, yes_no};
use crate::env::dirsrv::{Index, InstanceInfo, SuffixKind};
use crate::routes::Rq;

fn backend(k: SuffixKind) -> &'static str {
    match k {
        SuffixKind::User => "Directory (user)",
        SuffixKind::Local => "Directory (server's own)",
        SuffixKind::Projected => "User profiles (read only)",
        SuffixKind::Schema => "Schema",
        SuffixKind::Config => "Configuration",
    }
}

/// `/dirsrv/inst/<name>/suffixes/...`.
pub fn route(rq: &Rq<'_>, i: &InstanceInfo, rest: &[&str]) -> Response {
    match rest {
        [] => list(rq, i),
        ["remove"] => remove(rq, i),
        _ => rq.not_found(&format!("page {} of suffixes", rest.join("/"))),
    }
}

fn list(rq: &Rq<'_>, i: &InstanceInfo) -> Response {
    let here = inst_route(i, "suffixes");
    let function = "Add directory server suffix";
    if rq.post() {
        let f = rq.form();
        if !f.has("add") {
            return rq.redirect(&here);
        }
        if let Err(r) = need(rq, Some(i), function, &["Instances", &i.name]) {
            return r;
        }
        return match rq.env().dirsrv.add_suffix(rq.user(), &i.name, f.str("dn")) {
            Ok(()) => changed(rq, i, &here),
            Err(e) => fail(rq, &e, function, &["Instances", &i.name], &here),
        };
    }
    let all = match rq.env().dirsrv.suffixes(rq.user(), &i.name) {
        Ok(s) => s,
        Err(e) => return fail(rq, &e, "Directory server suffixes", &[], "/dirsrv"),
    };
    let rows = all
        .iter()
        .map(|s| {
            let act = if s.kind == SuffixKind::User {
                actions(&[(
                    rq.ctx.url(&format!("{here}/remove?dn={}", enc(&s.dn))),
                    "Remove",
                )])
            } else {
                String::new()
            };
            vec![
                Cell::act(act),
                Cell::mono(&s.dn),
                Cell::text(backend(s.kind)),
                Cell::num(&s.entries.map_or_else(String::new, |n| n.to_string())),
            ]
        })
        .collect();
    let list = List {
        cols: ["Actions", "Suffix", "Backend", "Entries"]
            .iter()
            .map(|l| Col::plain(l))
            .collect(),
        rows,
        empty: String::new(),
        keep: Vec::new(),
    };
    let add = form(
        &rq.ctx,
        "",
        &format!(
            "<p>New suffix: {} {}</p>",
            input("dn", "", 50),
            buttons(&[Button::new("add", "Add suffix")])
        ),
    );
    let title = format!("Suffixes of {}", i.name);
    page(
        rq,
        &["Instances", &i.name, "Suffixes"],
        &title,
        format!("{}{}{add}", inst_tabs(rq, i, 1), list_table(&list)),
        "dssuffixes",
        false,
    )
}

fn remove(rq: &Rq<'_>, i: &InstanceInfo) -> Response {
    let dn = rq.query().str("dn").to_string();
    let back = inst_route(i, "suffixes");
    let function = "Remove directory server suffix";
    if rq.post() {
        if rq.form().has("cancel") {
            return rq.redirect(&back);
        }
        if let Err(r) = need(rq, Some(i), function, &["Instances", &i.name]) {
            return r;
        }
        return match rq.env().dirsrv.remove_suffix(rq.user(), &i.name, &dn) {
            Ok(()) => changed(rq, i, &back),
            Err(e) => fail(rq, &e, function, &["Instances", &i.name], &back),
        };
    }
    confirm(
        rq,
        "Remove Suffix",
        &["Instances", &i.name],
        "The suffix is taken out of the instance's configuration; its entries stay in the instance's store and come back if the suffix is added again. The instance stops serving it at its next start. Press OK to remove the suffix listed below, or Cancel to return.",
        &["Suffix", "Instance"],
        vec![vec![mono(&dn), mono(&i.name)]],
    )
}

fn kinds(x: &Index) -> String {
    let mut v = Vec::new();
    for (on, w) in [
        (x.presence, "presence"),
        (x.equality, "equality"),
        (x.substring, "substring"),
    ] {
        if on {
            v.push(w);
        }
    }
    v.join(", ")
}

/// `/dirsrv/inst/<name>/indexes/...`.
pub fn indexes(rq: &Rq<'_>, i: &InstanceInfo, rest: &[&str]) -> Response {
    let here = inst_route(i, "indexes");
    match rest {
        [] => {}
        ["remove"] => return remove_index(rq, i),
        _ => return rq.not_found(&format!("page {} of indexes", rest.join("/"))),
    }
    let function = "Add directory server index";
    if rq.post() {
        let f = rq.form();
        if !f.has("add") {
            return rq.redirect(&here);
        }
        if let Err(r) = need(rq, Some(i), function, &["Instances", &i.name]) {
            return r;
        }
        let x = Index {
            attr: f.str("attr").trim().to_string(),
            presence: f.has("p"),
            equality: f.has("e"),
            substring: f.has("s"),
            always: false,
        };
        return match rq.env().dirsrv.put_index(rq.user(), &i.name, &x) {
            Ok(()) => changed(rq, i, &here),
            Err(e) => fail(rq, &e, function, &["Instances", &i.name], &here),
        };
    }
    let all = match rq.env().dirsrv.indexes(rq.user(), &i.name) {
        Ok(s) => s,
        Err(e) => return fail(rq, &e, "Directory server indexes", &[], "/dirsrv"),
    };
    let rows = all
        .iter()
        .map(|x| {
            let act = if x.always {
                String::new()
            } else {
                actions(&[(
                    rq.ctx.url(&format!("{here}/remove?attr={}", enc(&x.attr))),
                    "Remove",
                )])
            };
            vec![
                Cell::act(act),
                Cell::mono(&x.attr),
                Cell::text(&kinds(x)),
                Cell::text(yes_no(x.always)),
            ]
        })
        .collect();
    let list = List {
        cols: ["Actions", "Attribute", "Keys", "Always"]
            .iter()
            .map(|l| Col::plain(l))
            .collect(),
        rows,
        empty: String::new(),
        keep: Vec::new(),
    };
    let add = form(
        &rq.ctx,
        "",
        &format!(
            "<p>Attribute: {} &nbsp; {} presence &nbsp; {} equality &nbsp; {} substring &nbsp; {}</p>",
            input("attr", "", 30),
            checkbox("p", true),
            checkbox("e", true),
            checkbox("s", false),
            buttons(&[Button::new("add", "Add or change index")])
        ),
    );
    let title = format!("Indexes of {}", i.name);
    page(
        rq,
        &["Instances", &i.name, "Indexes"],
        &title,
        format!("{}{}{add}", inst_tabs(rq, i, 2), list_table(&list)),
        "dsindexes",
        false,
    )
}

fn remove_index(rq: &Rq<'_>, i: &InstanceInfo) -> Response {
    let attr = rq.query().str("attr").to_string();
    let back = inst_route(i, "indexes");
    let function = "Remove directory server index";
    if rq.post() {
        if rq.form().has("cancel") {
            return rq.redirect(&back);
        }
        if let Err(r) = need(rq, Some(i), function, &["Instances", &i.name]) {
            return r;
        }
        return match rq.env().dirsrv.remove_index(rq.user(), &i.name, &attr) {
            Ok(()) => changed(rq, i, &back),
            Err(e) => fail(rq, &e, function, &["Instances", &i.name], &back),
        };
    }
    confirm(
        rq,
        "Remove Index",
        &["Instances", &i.name],
        "Searches on the attribute walk the entries instead of the index once the instance starts again. Press OK to remove the index listed below, or Cancel to return.",
        &["Attribute", "Instance"],
        vec![vec![mono(&attr), mono(&i.name)]],
    )
}
