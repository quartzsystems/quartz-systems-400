// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/dirsrv/inst/<name>/schema`: attribute types or object classes
//! (`?kind=Attribute types|Object classes`), those added since the
//! schema was shipped unless a name is searched for or All is chosen;
//! `schema/attr/<id>` and `schema/class/<id>` one definition, with
//! `remove` (confirmed); `schema/add` a new definition, typed as its
//! description.

use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::partials::{
    Button, Cell, Col, FilterField, List, Paging, Prop, PropGroup, actions, buttons, filter, form,
    list_table, mono, paging, props_table,
};

use super::{changed, confirm, enc, fail, inst_route, inst_tabs, need, page, yes_no};
use crate::env::dirsrv::{InstanceInfo, SchemaDef, SchemaKind, SchemaQuery};
use crate::routes::Rq;
use crate::routes::common::{pick, strings};

const KINDS: &[&str] = &["Attribute types", "Object classes"];
const SHOW: &[&str] = &["Added", "All"];

fn kind_of(s: &str) -> SchemaKind {
    if s == KINDS[1] || s == "class" {
        SchemaKind::Class
    } else {
        SchemaKind::Attribute
    }
}

fn seg(k: SchemaKind) -> &'static str {
    match k {
        SchemaKind::Attribute => "attr",
        SchemaKind::Class => "class",
    }
}

/// `/dirsrv/inst/<name>/schema/...`.
pub fn route(rq: &Rq<'_>, i: &InstanceInfo, rest: &[&str]) -> Response {
    match rest {
        [] => list(rq, i),
        ["add"] => add(rq, i),
        [k @ ("attr" | "class"), id] => one(rq, i, kind_of(k), id, false),
        [k @ ("attr" | "class"), id, "remove"] => one(rq, i, kind_of(k), id, true),
        _ => rq.not_found(&format!("page {} of the schema", rest.join("/"))),
    }
}

#[allow(clippy::many_single_char_names, clippy::too_many_lines)]
fn list(rq: &Rq<'_>, i: &InstanceInfo) -> Response {
    let q = rq.query();
    let kind_text = if q.str("kind") == KINDS[1] {
        KINDS[1]
    } else {
        KINDS[0]
    };
    let kind = kind_of(kind_text);
    let name = q.str("q").trim().to_string();
    let show = if q.str("show") == SHOW[1] {
        SHOW[1]
    } else {
        SHOW[0]
    };
    let custom_only = show == SHOW[0] && name.is_empty();
    let d = &rq.env().dirsrv;
    let total = match d.schema(
        rq.user(),
        &i.name,
        &SchemaQuery {
            kind,
            contains: name.clone(),
            custom_only,
            start: 0,
            max: 1,
        },
    ) {
        Ok(p) => p.total as usize,
        Err(e) => return fail(rq, &e, "Directory server schema", &[], "/dirsrv"),
    };
    let keep = vec![
        ("kind".to_string(), kind_text.to_string()),
        ("q".to_string(), name.clone()),
        ("show".to_string(), show.to_string()),
    ];
    let pg = Paging::from_query(q.str("page"), q.str("rows"), total, keep);
    let r = pg.range();
    let defs = d
        .schema(
            rq.user(),
            &i.name,
            &SchemaQuery {
                kind,
                contains: name.clone(),
                custom_only,
                start: u32::try_from(r.start).unwrap_or(u32::MAX),
                max: u32::try_from(r.len()).unwrap_or(u32::MAX).max(1),
            },
        )
        .map(|p| p.defs)
        .unwrap_or_default();
    let base = inst_route(i, "schema");
    let rows = defs
        .iter()
        .take(r.len())
        .map(|x| {
            let u = format!("{base}/{}/{}", seg(kind), enc(&x.oid));
            let mut acts = vec![(rq.ctx.url(&u), "Properties")];
            if !x.protected {
                acts.push((rq.ctx.url(&format!("{u}/remove")), "Remove"));
            }
            let detail = match kind {
                SchemaKind::Attribute => x.syntax.clone(),
                SchemaKind::Class => x.class_kind.clone(),
            };
            vec![
                Cell::act(actions(&acts)),
                Cell::mono(x.names.first().map_or(&x.oid, |n| n)),
                Cell::mono(&x.oid),
                Cell::mono(&detail),
                Cell::text(&x.desc),
                Cell::text(yes_no(x.custom)),
            ]
        })
        .collect();
    let third = match kind {
        SchemaKind::Attribute => "Syntax",
        SchemaKind::Class => "Kind",
    };
    let list = List {
        cols: ["Actions", "Name", "OID", third, "Description", "Added"]
            .iter()
            .map(|l| Col::plain(l))
            .collect(),
        rows,
        empty: if custom_only {
            "Nothing has been added to the shipped schema. Choose All to see every definition."
                .into()
        } else {
            "No definitions match.".into()
        },
        keep: Vec::new(),
    };
    let body = format!(
        "{}{}{}{}{}",
        inst_tabs(rq, i, 3),
        filter(
            &[
                FilterField::Select(
                    "Show".into(),
                    "kind".into(),
                    strings(KINDS),
                    kind_text.into()
                ),
                FilterField::Text("Name contains".into(), "q".into(), name, 20),
                FilterField::Select(
                    "Definitions".into(),
                    "show".into(),
                    strings(SHOW),
                    show.into()
                ),
            ],
            false
        ),
        list_table(&list),
        if total > pg.rows {
            paging(&pg)
        } else {
            String::new()
        },
        form(
            &rq.ctx,
            &rq.ctx.url(&format!("{base}/add")),
            &buttons(&[Button::new("new", "Add definition...")])
        )
    );
    let title = format!("Schema of {}", i.name);
    page(
        rq,
        &["Instances", &i.name, "Schema"],
        &title,
        body,
        "dsschema",
        false,
    )
}

fn list_of(v: &[String]) -> String {
    if v.is_empty() {
        "-".into()
    } else {
        v.join(", ")
    }
}

fn one(rq: &Rq<'_>, i: &InstanceInfo, kind: SchemaKind, id: &str, remove: bool) -> Response {
    let d = &rq.env().dirsrv;
    let found = d
        .schema(
            rq.user(),
            &i.name,
            &SchemaQuery {
                kind,
                contains: id.to_string(),
                ..SchemaQuery::default()
            },
        )
        .map(|p| {
            p.defs.into_iter().find(|x| {
                x.oid.eq_ignore_ascii_case(id) || x.names.iter().any(|n| n.eq_ignore_ascii_case(id))
            })
        });
    let x: SchemaDef = match found {
        Ok(Some(x)) => x,
        Ok(None) => return rq.not_found(&format!("schema definition {id}")),
        Err(e) => return fail(rq, &e, "Directory server schema", &[], "/dirsrv"),
    };
    let back = inst_route(i, "schema");
    let name = x.names.first().cloned().unwrap_or_else(|| x.oid.clone());
    if remove {
        let function = "Remove schema definition";
        if rq.post() {
            if rq.form().has("cancel") {
                return rq.redirect(&back);
            }
            if let Err(r) = need(rq, Some(i), function, &["Instances", &i.name]) {
                return r;
            }
            return match d.remove_schema(rq.user(), &i.name, kind, &x.oid) {
                Ok(()) => changed(rq, i, &back),
                Err(e) => fail(rq, &e, function, &["Instances", &i.name], &back),
            };
        }
        return confirm(
            rq,
            "Remove Schema Definition",
            &["Instances", &i.name, "Schema"],
            "The definition leaves the schema at once; it is refused while another definition or an entry uses it. Press OK to remove the definition listed below, or Cancel to return.",
            &["Name", "OID"],
            vec![vec![mono(&name), mono(&x.oid)]],
        );
    }
    let mut rows = vec![
        Prop::new("Name", mono(&name)),
        Prop::new(
            "Other names",
            esc(&list_of(&x.names[x.names.len().min(1)..])),
        ),
        Prop::new("OID", mono(&x.oid)),
        Prop::new("Description", esc(&x.desc)),
        Prop::new(
            match kind {
                SchemaKind::Attribute => "Supertype",
                SchemaKind::Class => "Superclasses",
            },
            mono(&list_of(&x.sup)),
        ),
    ];
    match kind {
        SchemaKind::Attribute => {
            rows.push(Prop::new("Syntax", mono(&x.syntax)));
            rows.push(Prop::new("Single value", esc(yes_no(x.single_value))));
            rows.push(Prop::new("Access class", mono(&x.access)));
        }
        SchemaKind::Class => {
            rows.push(Prop::new("Kind", mono(&x.class_kind)));
            rows.push(Prop::new("Required attributes", mono(&list_of(&x.must))));
            rows.push(Prop::new("Allowed attributes", mono(&list_of(&x.may))));
        }
    }
    rows.push(Prop::new("Added since shipped", esc(yes_no(x.custom))));
    rows.push(Prop::new("Needed by the server", esc(yes_no(x.protected))));
    rows.push(Prop::new("Description as published", mono(&x.text)));
    let body = format!(
        "{}{}",
        inst_tabs(rq, i, 3),
        props_table(rq.ctx.product, &[PropGroup { title: None, rows }], true)
    );
    page(
        rq,
        &["Instances", &i.name, "Schema", &name],
        &format!("Schema definition {name}"),
        body,
        "dsschema",
        false,
    )
}

fn add(rq: &Rq<'_>, i: &InstanceInfo) -> Response {
    let back = inst_route(i, "schema");
    let function = "Add schema definition";
    let f = rq.form();
    if rq.post() && !f.has("new") {
        if f.has("cancel") {
            return rq.redirect(&back);
        }
        if let Err(r) = need(rq, Some(i), function, &["Instances", &i.name]) {
            return r;
        }
        let kind = kind_of(f.str("kind"));
        return match rq
            .env()
            .dirsrv
            .add_schema(rq.user(), &i.name, kind, f.str("text"))
        {
            Ok(()) => changed(rq, i, &back),
            Err(e) => fail(
                rq,
                &e,
                function,
                &["Instances", &i.name],
                &format!("{back}/add"),
            ),
        };
    }
    let rows = vec![
        Prop::new("Kind", pick("kind", &strings(KINDS), KINDS[0])),
        Prop::new(
            "Description",
            "<textarea name=\"text\" rows=\"4\" cols=\"90\" class=\"conf\"></textarea>".into(),
        )
        .required(),
    ];
    let body = form(
        &rq.ctx,
        "",
        &format!(
            "<p>{}</p>{}{}",
            esc(
                "Type the definition as cn=schema publishes it, for example ( 1.3.6.1.4.1.99999.1.1 NAME 'badgeNumber' SYNTAX 1.3.6.1.4.1.1466.115.121.1.15 SINGLE-VALUE ). Use an OID of your organization's own arc."
            ),
            props_table(rq.ctx.product, &[PropGroup { title: None, rows }], true),
            buttons(&[Button::new("ok", "OK"), Button::new("cancel", "Cancel")])
        ),
    );
    page(
        rq,
        &["Instances", &i.name, "Schema", "Add"],
        "Add schema definition",
        body,
        "dsschema",
        false,
    )
}
