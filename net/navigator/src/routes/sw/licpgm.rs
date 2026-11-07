// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/licpgm?s=&c=&sort=`: the product directory (`GO LICPGM` 10), the
//! products on an image catalog (11), Install (`RSTLICPGM`) and Delete
//! (`DLTLICPGM`, 12) through a confirmation, the license keys and one
//! product option's attributes.

use qsweb::html::{esc, query};
use qsweb::http::Response;
use qsweb::partials::{
    Button, Cell, Col, FilterField, List, Prop, PropGroup, actions, buttons, filter, form, grey,
    list_table, mono, props_table,
};

use super::{GROUP, need, page, run_all};
use crate::PRODUCT;
use crate::env::Target;
use crate::env::sw::Product;
use crate::routes::Rq;
use crate::routes::common::{Confirm, confirm_in};

/// The Show choices.
const SHOW: [&str; 3] = [
    "Installed",
    "All available on image catalog",
    "Not installed",
];

fn status(p: &Product) -> String {
    if p.installed {
        esc(&p.status)
    } else {
        grey("Not installed")
    }
}

/// The product option's route: `/licpgm/<id>/<option>`.
fn base(p: &Product) -> String {
    format!("/licpgm/{}/{}", p.id, p.option)
}

/// `/licpgm`.
#[allow(clippy::too_many_lines)]
pub fn list(rq: &Rq<'_>) -> Response {
    let sw = &rq.env().sw;
    let user = rq.user();
    let catalogs = sw.catalogs(user);
    let q = rq.query();
    if rq.post() {
        let f = rq.form();
        let c = q
            .get("c")
            .or(catalogs.first().map(|c| c.name.as_str()))
            .unwrap_or("")
            .to_string();
        if f.has("install") {
            return rq.redirect(&format!("/licpgm{}", query(&[("s", SHOW[1]), ("c", &c)])));
        }
        if f.has("keys") {
            return rq.redirect("/licpgm/keys");
        }
        return run_all(rq, &["CHKPRDOPT PRDID(*ALL)".to_string()], "/licpgm");
    }
    let show = SHOW
        .iter()
        .find(|s| **s == q.str("s"))
        .copied()
        .unwrap_or(SHOW[0]);
    let cat = catalogs
        .iter()
        .find(|c| c.name == q.str("c"))
        .or(catalogs.first())
        .map(|c| c.name.clone())
        .unwrap_or_default();
    let directory = sw.products(user);
    let mut messages = Vec::new();
    let on_catalog = || match sw.catalog_products(user, &cat) {
        Ok(ps) => (ps, None),
        Err(m) => (Vec::new(), Some(m)),
    };
    let mut ps: Vec<Product> = if show == SHOW[0] {
        directory.clone()
    } else {
        {
            let (ps, m) = on_catalog();
            messages.extend(m);
            // A catalog's product shows the directory's state of it.
            let mut ps: Vec<Product> = ps
                .into_iter()
                .map(|p| {
                    directory
                        .iter()
                        .find(|d| d.id == p.id && d.option == p.option)
                        .cloned()
                        .unwrap_or(p)
                })
                .collect();
            if show == SHOW[2] {
                for d in directory.iter().filter(|d| !d.installed) {
                    if !ps.iter().any(|p| p.id == d.id && p.option == d.option) {
                        ps.push(d.clone());
                    }
                }
                ps.retain(|p| !p.installed);
            }
            ps
        }
    };
    let key = q.str("sort");
    match key {
        "p" => ps.sort_by(|a, b| a.id.cmp(&b.id)),
        "o" => ps.sort_by(|a, b| a.option.cmp(&b.option)),
        "d" => ps.sort_by(|a, b| a.text.cmp(&b.text)),
        "s" => ps.sort_by_key(|p| !p.installed),
        _ => {}
    }
    let chosen = |v: &str, first: &str| {
        if v == first {
            String::new()
        } else {
            v.to_string()
        }
    };
    let first_cat = catalogs.first().map(|c| c.name.clone()).unwrap_or_default();
    let f = filter(
        &[
            FilterField::Select(
                "Show".into(),
                "s".into(),
                SHOW.iter().map(ToString::to_string).collect(),
                chosen(show, SHOW[0]),
            ),
            FilterField::Select(
                "Image catalog".into(),
                "c".into(),
                catalogs.iter().map(|c| c.name.clone()).collect(),
                chosen(&cat, &first_cat),
            ),
        ],
        false,
    );
    let mut keep = Vec::new();
    if show != SHOW[0] {
        keep.push(("s".to_string(), show.to_string()));
    }
    if cat != first_cat {
        keep.push(("c".to_string(), cat.clone()));
    }
    let rows = ps
        .iter()
        .map(|p| {
            let b = base(p);
            let act = if p.installed {
                actions(&[
                    (rq.ctx.url(&format!("{b}/confirm?op=delete")), "Delete"),
                    (rq.ctx.url(&b), "Properties"),
                ])
            } else {
                actions(&[(
                    rq.ctx.url(&format!(
                        "{b}/confirm{}",
                        query(&[("op", "install"), ("c", &cat)])
                    )),
                    "Install",
                )])
            };
            vec![
                Cell::act(act),
                Cell::mono(&p.id),
                Cell::mono(&p.option),
                Cell::mono(&p.release),
                Cell::text(&p.text),
                Cell::html(status(p)),
                Cell::text(&p.license),
            ]
        })
        .collect();
    let list = List {
        cols: vec![
            Col::plain("Actions"),
            Col::sorted("Product", "p"),
            Col::sorted("Option", "o"),
            Col::plain("Release"),
            Col::sorted("Description", "d"),
            Col::sorted("Installed status", "s"),
            Col::plain("License"),
        ],
        rows,
        empty: "No products match the filter.".into(),
        keep,
    };
    let bs = form(
        &rq.ctx,
        "",
        &buttons(&[
            Button::new("install", "Install from image catalog..."),
            Button::new("check", "Check compatibility"),
            Button::new("keys", "Display license keys"),
        ]),
    );
    let body = format!("{f}{}{bs}", list_table(&list));
    page(
        rq,
        &["Licensed programs"],
        "Licensed programs",
        Some("Licensed programs installed on the system (GO LICPGM option 10)."),
        messages,
        body,
        "licpgm",
        Some("/licpgm"),
    )
}

fn find(rq: &Rq<'_>, id: &str, opt: &str) -> Option<Product> {
    let sw = &rq.env().sw;
    let user = rq.user();
    sw.products(user)
        .into_iter()
        .find(|p| p.id == id && p.option == opt)
        .or_else(|| {
            sw.catalogs(user)
                .iter()
                .filter_map(|c| sw.catalog_products(user, &c.name).ok())
                .flatten()
                .find(|p| p.id == id && p.option == opt)
        })
}

/// `/licpgm/<id>/<option>`: one product option.
pub fn props(rq: &Rq<'_>, id: &str, opt: &str) -> Response {
    let Some(p) = find(rq, id, opt) else {
        return rq.not_found(&format!("product {id} option {opt}"));
    };
    let installed = if p.installed_at == 0 {
        String::new()
    } else {
        esc(&rq.when(p.installed_at))
    };
    let rows = vec![
        Prop::new("Product", mono(&p.id)),
        Prop::new("Option", mono(&p.option)),
        Prop::new("Release", mono(&p.release)),
        Prop::new("Description", esc(&p.text)),
        Prop::new("Installed status", status(&p)),
        Prop::new("Product library", mono(&p.library)),
        Prop::new("Installed", installed),
        Prop::new("License", esc(&p.license)),
    ];
    let body = format!(
        "{}<p class=\"small\"><a href=\"{}\">Licensed programs</a></p>",
        props_table(&PRODUCT, &[PropGroup { title: None, rows }], true),
        esc(&rq.ctx.url("/licpgm"))
    );
    let h1 = format!("Product {} option {}", p.id, p.option);
    page(
        rq,
        &["Licensed programs", &h1],
        &h1,
        None,
        Vec::new(),
        body,
        "licpgm",
        None,
    )
}

/// `/licpgm/keys`: the license keys (`DSPLICKEY`).
pub fn keys(rq: &Rq<'_>) -> Response {
    let rows = rq
        .env()
        .sw
        .license_keys(rq.user())
        .iter()
        .map(|k| {
            vec![
                Cell::mono(&k.product),
                Cell::mono(&k.term),
                Cell::mono(&k.feature),
                Cell::mono(&k.usage_limit),
                Cell::mono(&k.expires),
                Cell::mono(&k.processor_group),
            ]
        })
        .collect();
    let list = List {
        cols: vec![
            Col::plain("Product"),
            Col::plain("License term"),
            Col::plain("Feature"),
            Col::plain("Usage limit"),
            Col::plain("Expires"),
            Col::plain("Processor group"),
        ],
        rows,
        empty: "The system holds no license keys.".into(),
        keep: Vec::new(),
    };
    page(
        rq,
        &["Licensed programs", "License keys"],
        "License keys",
        Some("License keys installed on the system (DSPLICKEY)."),
        Vec::new(),
        list_table(&list),
        "licpgm",
        None,
    )
}

/// `/licpgm/<id>/<option>/confirm?op=delete|install&c=`.
pub fn confirm(rq: &Rq<'_>, id: &str, opt: &str) -> Response {
    let Some(p) = find(rq, id, opt) else {
        return rq.not_found(&format!("product {id} option {opt}"));
    };
    let q = rq.query();
    let install = q.str("op") == "install";
    let (what, command, sentence) = if install {
        let cats = rq.env().sw.catalogs(rq.user());
        let Some(c) = cats.iter().find(|c| c.name == q.str("c")).or(cats.first()) else {
            return rq.not_found("image catalog");
        };
        (
            "Install Licensed Program",
            format!(
                "RSTLICPGM LICPGM({}) OPTION({}) DEV({})",
                p.id, p.option, c.device
            ),
            format!(
                "Press OK to install the product option listed below from image catalog {} on device {}.",
                c.name, c.device
            ),
        )
    } else {
        (
            "Delete Licensed Program",
            format!("DLTLICPGM LICPGM({}) OPTION({})", p.id, p.option),
            "Deleting a licensed program removes its objects from the system; programs that need it stop working. Press OK to delete the product option listed below, or Cancel to return without deleting.".to_string(),
        )
    };
    if rq.post() {
        if rq.form().has("cancel") {
            return rq.redirect("/licpgm");
        }
        let t = Target::obj("QSYS", &p.id, "*PRDDFN");
        if let Err(r) = need(rq, what, &t, &[GROUP, "Licensed programs"]) {
            return r;
        }
        return run_all(rq, &[command], "/licpgm");
    }
    confirm_in(
        rq,
        GROUP,
        &Confirm {
            what,
            crumb: &["Licensed programs"],
            sentence,
            cols: &[
                "Product",
                "Option",
                "Release",
                "Description",
                "Installed status",
            ],
            rows: vec![vec![
                mono(&p.id),
                mono(&p.option),
                mono(&p.release),
                esc(&p.text),
                status(&p),
            ]],
            options: String::new(),
            command,
        },
    )
}
