// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `:PNLGRP.` element into a [`PanelGroup`]: classes, variables,
//! records, lists, conditions, truth tables, key lists, help modules and
//! imports. Panels are `panel.rs`.

use super::{Build, line_u32};
use crate::diag::ids;
use crate::expr;
use crate::model::{
    Check, Class, Condition, HelpModule, Import, Key, KeyItem, KeyList, ListDef, PanelGroup,
    SearchTopic, Span, Synonyms, TranslateItem, TruthTable, TtValue, VarRecord, Variable,
};
use crate::tree::Element;

impl Build {
    /// Build the group from its element.
    pub(super) fn group(&mut self, el: &Element) -> PanelGroup {
        let mut g = PanelGroup {
            schidx: self.upper(el, "SCHIDX"),
            dftmsgf: self.upper(el, "DFTMSGF"),
            submsgf: self.upper(el, "SUBMSGF"),
            txtchrid: self.upper(el, "TXTCHRID"),
            hlpshelf: self.upper(el, "HLPSHELF"),
            ..PanelGroup::default()
        };
        for c in el.elements() {
            match c.name.as_str() {
                "CLASS" => {
                    if let Some(x) = self.class(c) {
                        g.classes.push(x);
                    }
                }
                "VAR" => {
                    if let Some(x) = self.variable(c) {
                        g.variables.push(x);
                    }
                }
                "VARRCD" => {
                    if let Some(x) = self.var_record(c) {
                        g.var_records.push(x);
                    }
                }
                "LISTDEF" => {
                    if let Some(x) = self.list_def(c) {
                        g.list_defs.push(x);
                    }
                }
                "COND" => {
                    if let Some(x) = self.condition(c) {
                        g.conditions.push(x);
                    }
                }
                "TT" => {
                    if let Some(x) = self.truth_table(c) {
                        g.truth_tables.push(x);
                    }
                }
                "KEYL" => {
                    if let Some(x) = self.key_list(c) {
                        g.key_lists.push(x);
                    }
                }
                "PANEL" => {
                    if let Some(x) = self.panel(c) {
                        g.panels.push(x);
                    }
                }
                "PRTPNL" => {
                    if let Some(x) = self.print_panel(c) {
                        g.print_panels.push(x);
                    }
                }
                "HELP" => {
                    if let Some(x) = self.help(c) {
                        g.helps.push(x);
                    }
                }
                "IMPORT" => {
                    if let (Some(name), Some(pnlgrp)) =
                        (self.upper(c, "NAME"), self.upper(c, "PNLGRP"))
                    {
                        g.imports.push(Import {
                            name,
                            pnlgrp,
                            line: line_u32(c.line),
                        });
                    }
                }
                "COPYR" => {
                    let s = self.spans(&c.children);
                    g.copyright = Some(Span::plain(&s));
                }
                "HLPSHELF" => g.hlpshelf = self.upper(c, "NAME"),
                "KEYLSHLF" => {
                    if let Some(n) = self.upper(c, "NAME") {
                        g.keylshlf.push(n);
                    }
                }
                "ISCHSYN" => {
                    if let Some(root) = self.upper(c, "ROOT") {
                        let s = self.spans(&c.children);
                        let words = Span::plain(&s)
                            .split_whitespace()
                            .map(str::to_ascii_uppercase)
                            .collect();
                        g.synonyms.push(Synonyms { root, words });
                    }
                }
                _ => {}
            }
        }
        g
    }

    fn class(&mut self, el: &Element) -> Option<Class> {
        let name = self.name(el, "NAME", "Class")?;
        self.define("Class", &name, el.line);
        let basetype = self.basetype(el, "BASETYPE")?;
        let mut translations = Vec::new();
        let mut tl_msg = None;
        for tl in el.named("TL") {
            tl_msg = self.upper(tl, "MSG");
            for ti in tl.named("TI") {
                if let Some(value) = self.text(ti, "VALUE") {
                    let s = self.spans(&ti.children);
                    translations.push(TranslateItem {
                        value,
                        text: Span::plain(&s),
                    });
                }
            }
        }
        let mut checks = Vec::new();
        for ck in el.named("CHECK") {
            if let Some(kind) = self.upper(ck, "TYPE") {
                let var = self.upper(ck, "VAR");
                self.refer_opt("Variable", var.as_ref(), ck.line);
                checks.push(Check {
                    kind,
                    msg: self.upper(ck, "MSG"),
                    var,
                    msgid: self.upper(ck, "MSGID"),
                });
            }
        }
        Some(Class {
            name,
            basetype,
            width: self.u32(el, "WIDTH"),
            shift: self.shift(el, "SHIFT").unwrap_or_default(),
            case: self.case(el, "CASE"),
            chrid: self.upper(el, "CHRID"),
            ecparam: self.text(el, "ECPARAM"),
            translations,
            tl_msg,
            checks,
            line: line_u32(el.line),
        })
    }

    fn variable(&mut self, el: &Element) -> Option<Variable> {
        let name = self.name(el, "NAME", "Variable")?;
        self.define("Variable", &name, el.line);
        let class = self.upper(el, "CLASS")?;
        self.refer("Class", &class, el.line);
        Some(Variable {
            name,
            class,
            value: self.text(el, "VALUE"),
            size: self.u32(el, "SIZE"),
            line: line_u32(el.line),
        })
    }

    fn var_record(&mut self, el: &Element) -> Option<VarRecord> {
        let name = self.name(el, "NAME", "Variable record")?;
        self.define("Variable record", &name, el.line);
        let vars = self.names(el, "VARS");
        for v in &vars {
            self.refer("Variable", v, el.line);
        }
        let noget = self.names(el, "NOGET");
        let noput = self.names(el, "NOPUT");
        for v in noget.iter().chain(&noput) {
            self.refer("Variable", v, el.line);
        }
        Some(VarRecord {
            name,
            vars,
            noget,
            noput,
            extvars: self.names(el, "EXTVARS"),
            line: line_u32(el.line),
        })
    }

    fn list_def(&mut self, el: &Element) -> Option<ListDef> {
        let name = self.name(el, "NAME", "List")?;
        self.define("List", &name, el.line);
        let vars = self.names(el, "VARS");
        for v in &vars {
            self.refer("Variable", v, el.line);
        }
        Some(ListDef {
            name,
            vars,
            msgid: self.upper(el, "MSGID"),
            line: line_u32(el.line),
        })
    }

    fn condition(&mut self, el: &Element) -> Option<Condition> {
        let name = self.name(el, "NAME", "Condition")?;
        self.define("Condition", &name, el.line);
        let text = self.text(el, "EXPR")?;
        let expr = match expr::parse(&text) {
            Ok(e) => e,
            Err(why) => {
                self.diags.push(ids::bad_expr(el.line, &name, why));
                return None;
            }
        };
        for v in expr.variables() {
            self.refer("Variable", v, el.line);
        }
        for l in expr.lists() {
            self.refer("List", l, el.line);
        }
        Some(Condition {
            name,
            expr,
            msg: self.upper(el, "MSG"),
            line: line_u32(el.line),
        })
    }

    fn truth_table(&mut self, el: &Element) -> Option<TruthTable> {
        let name = self.name(el, "NAME", "Truth table")?;
        self.define("Truth table", &name, el.line);
        let conds = self.names(el, "CONDS");
        for c in &conds {
            self.refer("Condition", c, el.line);
        }
        let mut rows = Vec::new();
        for row in el.named("TTROW") {
            let Some(values) = self.upper(row, "VALUES") else {
                continue;
            };
            let mut cells = Vec::new();
            for v in values.split_whitespace() {
                match v {
                    "1" | "Y" | "YES" => cells.push(TtValue::True),
                    "0" | "N" | "NO" => cells.push(TtValue::False),
                    "X" | "*" => cells.push(TtValue::Any),
                    other => self.diags.push(ids::bad_attr(
                        row.line,
                        "TTROW",
                        "VALUES",
                        other,
                        "1, 0 or X",
                    )),
                }
            }
            if cells.len() != conds.len() {
                self.diags
                    .push(ids::tt_row_width(row.line, cells.len(), conds.len()));
            }
            rows.push(cells);
        }
        Some(TruthTable {
            name,
            conds,
            rows,
            line: line_u32(el.line),
        })
    }

    fn key_list(&mut self, el: &Element) -> Option<KeyList> {
        let name = self.name(el, "NAME", "Key list")?;
        self.define("Key list", &name, el.line);
        let help = self.upper(el, "HELP");
        self.refer_opt("Help module", help.as_ref(), el.line);
        let mut keys: Vec<KeyItem> = Vec::new();
        for ki in el.named("KEYI") {
            let Some(ktext) = self.upper(ki, "KEY") else {
                continue;
            };
            let Some(key) = Key::parse(&ktext) else {
                self.diags.push(ids::bad_key(ki.line, &ktext));
                continue;
            };
            // A key may come back under a condition: the first item
            // whose condition holds is the key's.
            let conditioned = ki.attr("COND").is_some();
            if keys.iter().any(|k| k.key == key && (k.cond.is_none() || !conditioned)) {
                self.diags.push(ids::duplicate_key(ki.line, &key.text()));
            }
            let Some(action) = self.action(ki, "ACTION") else {
                continue;
            };
            let help = self.upper(ki, "HELP");
            self.refer_opt("Help module", help.as_ref(), ki.line);
            let cond = self.upper(ki, "COND");
            self.refer_opt("Condition", cond.as_ref(), ki.line);
            let text = self.spans(&ki.children);
            keys.push(KeyItem {
                key,
                help,
                action,
                varupd: self.varupd(ki, "VARUPD"),
                cond,
                text: Span::plain(&text),
                line: line_u32(ki.line),
            });
        }
        Some(KeyList {
            name,
            help,
            keys,
            line: line_u32(el.line),
        })
    }

    fn help(&mut self, el: &Element) -> Option<HelpModule> {
        let name = self.upper(el, "NAME")?;
        if name.is_empty()
            || name.len() > 32
            || !name.chars().all(|c| {
                c.is_ascii_alphanumeric() || matches!(c, '_' | '@' | '#' | '$' | '/' | '.')
            })
        {
            self.diags
                .push(ids::bad_name(el.line, "Help module", &name));
            return None;
        }
        self.define("Help module", &name, el.line);
        let mut title = self.leading_spans(el);
        if let Some(t) = self.text(el, "TITLE") {
            title = vec![Span::Text(t)];
        }
        let topics = el
            .named("ISCH")
            .map(|i| {
                let roots = self.names(i, "ROOTS");
                let text = self.spans(&i.children);
                SearchTopic { roots, text }
            })
            .collect();
        // The title is the leading text; the body starts at the first block.
        let body_el = Element {
            children: el.children[el.leading().len()..].to_vec(),
            ..el.clone()
        };
        let body = self.document(&body_el);
        if body.is_empty() {
            self.diags.push(ids::empty_help(el.line, &name));
        }
        Some(HelpModule {
            name,
            title,
            width: self.u32(el, "WIDTH"),
            depth: self.u32(el, "DEPTH"),
            body,
            topics,
            line: line_u32(el.line),
        })
    }
}
