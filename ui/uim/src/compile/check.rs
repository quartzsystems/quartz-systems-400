// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Checks over the finished model: every recorded name resolves,
//! imports name what the other group exports, and input fields have a
//! width to show.

use std::collections::BTreeSet;

use super::{Build, Imports};
use crate::diag::ids;
use crate::model::{Area, BaseType, DataNode, PanelGroup, Usage};

/// Run every check.
pub(super) fn run(b: &mut Build, g: &PanelGroup, imports: &dyn Imports) {
    let refs = std::mem::take(&mut b.refs);
    let imported = imported_names(b, g, imports);
    let mut reported = BTreeSet::new();
    for r in &refs {
        let found = match r.what {
            "Class" => g.class(&r.name).is_some(),
            "Variable" | "Symbol" => g.variable(&r.name).is_some(),
            "List" => g.list_def(&r.name).is_some(),
            "Condition" => {
                g.condition(&r.name).is_some() || g.truth_tables.iter().any(|t| t.name == r.name)
            }
            "Key list" => g.key_list(&r.name).is_some(),
            "Help module" => {
                r.name.contains('/')
                    || g.help(&r.name).is_some()
                    || imported.as_ref().is_none_or(|set| set.contains(&r.name))
            }
            "Panel" => {
                g.panel(&r.name).is_some()
                    || g.print_panels.iter().any(|p| p.name == r.name)
                    || imported.as_ref().is_none_or(|set| set.contains(&r.name))
            }
            "Variable record" => g.var_records.iter().any(|v| v.name == r.name),
            _ => true,
        };
        if found || !reported.insert((r.what, r.name.clone(), r.line)) {
            continue;
        }
        if r.what == "Symbol" {
            b.diags.push(ids::unknown_symbol(r.line, &r.name));
        } else {
            b.diags.push(ids::unresolved(r.line, r.what, &r.name));
        }
    }
    input_widths(b, g);
}

/// The names imports bring in, or `None` when an import cannot be
/// checked and every name must be taken on trust.
fn imported_names(
    b: &mut Build,
    g: &PanelGroup,
    imports: &dyn Imports,
) -> Option<BTreeSet<String>> {
    let mut set = BTreeSet::new();
    let mut complete = true;
    for i in &g.imports {
        match imports.exports(&i.pnlgrp) {
            Some(exports) => {
                if i.name == "*" {
                    set.extend(exports);
                } else if exports.contains(&i.name) {
                    set.insert(i.name.clone());
                } else {
                    b.diags.push(ids::import_not_exported(
                        i.line as usize,
                        &i.name,
                        &i.pnlgrp,
                    ));
                }
            }
            None => {
                if i.name == "*" {
                    complete = false;
                } else {
                    set.insert(i.name.clone());
                }
            }
        }
    }
    complete.then_some(set)
}

/// An input field or column needs a width from its class, its own
/// `BASETYPE` or `MAXWIDTH`.
fn input_widths(b: &mut Build, g: &PanelGroup) {
    let mut fields: Vec<(String, Option<u32>, usize)> = Vec::new();
    for p in &g.panels {
        collect(&p.areas, &mut fields);
    }
    for p in &g.print_panels {
        collect(&p.areas, &mut fields);
    }
    for (var, own, line) in fields {
        if own.is_some() {
            continue;
        }
        let Some(v) = g.variable(&var) else { continue };
        let Some(c) = g.class(&v.class) else { continue };
        if c.display_width().is_none() {
            b.diags.push(ids::input_without_width(line, &var, &c.name));
        }
    }
}

fn collect(areas: &[Area], out: &mut Vec<(String, Option<u32>, usize)>) {
    for a in areas {
        match a {
            Area::Data(d) => collect_data(&d.items, out),
            Area::List(l) => {
                for c in l.all_columns() {
                    if c.usage.is_input() {
                        out.push((c.var.clone(), c.maxwidth, c.line as usize));
                    }
                }
            }
            _ => {}
        }
    }
}

fn collect_data(items: &[DataNode], out: &mut Vec<(String, Option<u32>, usize)>) {
    for i in items {
        match i {
            DataNode::Group(g) => collect_data(&g.items, out),
            DataNode::Field(f) => {
                if f.usage.is_input() {
                    let own = f
                        .maxwidth
                        .or_else(|| f.basetype.as_ref().and_then(BaseType::width));
                    out.push((f.var.clone(), own, f.line as usize));
                }
                for x in &f.extenders {
                    if x.usage == Usage::Input || x.usage == Usage::Inout {
                        out.push((x.var.clone(), x.maxwidth, x.line as usize));
                    }
                }
            }
            DataNode::Const(_) | DataNode::Select(_) | DataNode::Column(_) => {}
        }
    }
}
