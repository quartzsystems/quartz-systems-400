// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! UIM panels: the panel's help, the key list's, and every place F1
//! can land on.

use uim::model::{
    Area, DataNode, Display, ListColNode, MenuNode, Panel, PanelGroup, Span,
};

use crate::{Category, Groups};

/// Audit every panel of every shipped group.
#[must_use]
pub fn audit(groups: &Groups) -> Category {
    let mut c = Category::new("panel help", 100);
    for (name, (g, dir)) in &groups.by_name {
        for p in &g.panels {
            let at = |what: &str| format!("{dir}/{name}.pnlgrp {} {what}", p.name);
            c.check(groups.has_text(g, p.help.as_deref()), || at("(panel)"));
            if let Some(k) = g.key_lists.iter().find(|k| k.name.eq_ignore_ascii_case(&p.keyl))
                && k.keys.iter().any(|i| !i.text.trim().is_empty())
            {
                c.check(groups.has_text(g, k.help.as_deref()), || {
                    at(&format!("key list {}", k.name))
                });
            }
            items(groups, g, p, &mut c, &at);
        }
    }
    c
}

fn text(spans: &[Span]) -> String {
    Span::plain(spans).trim().to_string()
}

#[allow(clippy::many_single_char_names)] // the model's usual names
fn items(
    groups: &Groups,
    g: &PanelGroup,
    p: &Panel,
    c: &mut Category,
    at: &dyn Fn(&str) -> String,
) {
    for a in &p.areas {
        match a {
            Area::Data(d) => data(groups, g, &d.items, d.help.as_deref(), c, at),
            Area::List(l) => {
                for col in l.columns.iter().flat_map(|n| match n {
                    ListColNode::Column(col) => vec![(col, None)],
                    ListColNode::Group(gr) => {
                        gr.columns.iter().map(|col| (col, gr.help.as_deref())).collect()
                    }
                }) {
                    let (col, grp) = col;
                    if col.display == Display::No {
                        continue;
                    }
                    let ok = [col.help.as_deref(), grp, l.help.as_deref()]
                        .into_iter()
                        .any(|h| groups.has_text(g, h));
                    c.check(ok, || at(&format!("list column {}", col.var)));
                }
                for act in &l.actions {
                    if act.option.is_none() || text(&act.text).is_empty() {
                        continue;
                    }
                    let ok = [act.help.as_deref(), l.help.as_deref()]
                        .into_iter()
                        .any(|h| groups.has_text(g, h));
                    c.check(ok, || at(&format!("option {}", text(&act.text))));
                }
            }
            Area::Menu(m) => {
                for n in &m.items {
                    let (list, grp) = match n {
                        MenuNode::Item(i) => (std::slice::from_ref(i), None),
                        MenuNode::Group { help, items, .. } => (items.as_slice(), help.as_deref()),
                    };
                    for i in list {
                        if text(&i.text).is_empty() {
                            continue;
                        }
                        let ok = [i.help.as_deref(), grp, m.help.as_deref()]
                            .into_iter()
                            .any(|h| groups.has_text(g, h));
                        c.check(ok, || at(&format!("menu item {}", text(&i.text))));
                    }
                }
            }
            _ => {}
        }
    }
}

fn data(
    groups: &Groups,
    g: &PanelGroup,
    nodes: &[DataNode],
    area: Option<&str>,
    c: &mut Category,
    at: &dyn Fn(&str) -> String,
) {
    for n in nodes {
        match n {
            DataNode::Group(gr) => data(groups, g, &gr.items, area, c, at),
            DataNode::Field(f) if f.display != Display::No => {
                let ok = [f.help.as_deref(), area].into_iter().any(|h| groups.has_text(g, h));
                c.check(ok, || at(&format!("data item {}", f.var)));
            }
            DataNode::Select(s) if s.display != Display::No => {
                let ok = [s.help.as_deref(), area].into_iter().any(|h| groups.has_text(g, h));
                c.check(ok, || at(&format!("selection {}", s.var)));
            }
            _ => {}
        }
    }
}
