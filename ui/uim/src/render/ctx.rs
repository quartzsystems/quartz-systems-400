// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The rendering context: the group, the dialog and the options, with
//! variable substitution, field widths and condition evaluation.

use super::Options;
use super::dialog::{Dialog, ListState};
use crate::model::{BaseType, Class, CmpOp, Expr, PanelGroup, Span};

/// What every area renderer reads.
pub(super) struct Ctx<'a> {
    /// The group.
    pub group: &'a PanelGroup,
    /// The dialog.
    pub dialog: &'a dyn Dialog,
    /// The options.
    pub opts: &'a Options,
}

impl Ctx<'_> {
    /// A variable's value: the dialog's, else the `VALUE` of the
    /// declaration, else blank.
    pub fn value(&self, name: &str) -> String {
        if let Some(v) = self.dialog.var(name) {
            return v;
        }
        self.group
            .variable(name)
            .and_then(|v| v.value.clone())
            .unwrap_or_default()
    }

    /// A variable's value as shown: translated through the class's
    /// `:TL.` when it has one.
    pub fn shown(&self, name: &str) -> String {
        let v = self.value(name);
        self.translate(name, &v)
    }

    /// The `:TI.` text for a value, or the value.
    pub fn translate(&self, name: &str, value: &str) -> String {
        if let Some(c) = self.class_of(name) {
            let t = value.trim_end();
            if let Some(ti) = c.translations.iter().find(|t2| t2.value == t) {
                return ti.text.clone();
            }
        }
        value.to_string()
    }

    /// The class of a variable.
    pub fn class_of(&self, var: &str) -> Option<&Class> {
        let v = self.group.variable(var)?;
        self.group.class(&v.class)
    }

    /// The display width of a variable: the class's (wide enough for
    /// its `:TL.` texts), or 10 when nothing says.
    pub fn width_of(&self, var: &str) -> usize {
        let Some(c) = self.class_of(var) else {
            return 10;
        };
        let texts = c
            .translations
            .iter()
            .map(|t| t.text.chars().count())
            .max()
            .unwrap_or(0);
        c.display_width().map_or(10, |w| w as usize).max(texts)
    }

    /// Whether a variable holds a number (right-aligned when shown).
    pub fn numeric(&self, var: &str) -> bool {
        self.class_of(var).is_some_and(|c| {
            matches!(
                c.basetype,
                BaseType::Bin(_)
                    | BaseType::Packed(..)
                    | BaseType::Zoned(..)
                    | BaseType::Float(_)
                    | BaseType::Dec
            )
        })
    }

    /// Plain text of spans with variables substituted.
    pub fn text(&self, spans: &[Span]) -> String {
        let mut out = String::new();
        for s in spans {
            match s {
                Span::Text(t) => out.push_str(t),
                Span::Var(v) => out.push_str(self.shown(v).trim_end()),
                Span::Hp(_, inner) | Span::Pk(inner) | Span::Pv(inner) | Span::Rt(inner) => {
                    out.push_str(&self.text(inner));
                }
                Span::Link { text, .. } => out.push_str(&self.text(text)),
            }
        }
        out
    }

    /// A condition by name: the group's, evaluated; else the dialog's.
    pub fn cond(&self, name: Option<&str>) -> bool {
        let Some(name) = name else {
            return true;
        };
        match self.group.condition(name) {
            Some(c) => self.eval(&c.expr),
            None => self.dialog.cond(name),
        }
    }

    /// A list's state, empty when the dialog has none.
    pub fn list(&self, name: &str) -> ListState {
        self.dialog.list(name).unwrap_or_default()
    }

    /// Evaluate an expression.
    pub fn eval(&self, e: &Expr) -> bool {
        match e {
            Expr::And(a, b) => self.eval(a) && self.eval(b),
            Expr::Or(a, b) => self.eval(a) || self.eval(b),
            Expr::Not(a) => !self.eval(a),
            Expr::Cmp(op, lhs, rhs) => {
                let (lhs, rhs) = (self.operand(lhs), self.operand(rhs));
                let ord = match (lhs.parse::<f64>(), rhs.parse::<f64>()) {
                    (Ok(l), Ok(r)) => l.partial_cmp(&r).unwrap_or(std::cmp::Ordering::Equal),
                    _ => lhs.trim_end().cmp(rhs.trim_end()),
                };
                match op {
                    CmpOp::Eq => ord.is_eq(),
                    CmpOp::Ne => ord.is_ne(),
                    CmpOp::Gt => ord.is_gt(),
                    CmpOp::Lt => ord.is_lt(),
                    CmpOp::Ge => ord.is_ge(),
                    CmpOp::Le => ord.is_le(),
                }
            }
            Expr::Node(l) => self.list(l).count() > 0,
            Expr::List(_) | Expr::Var(_) | Expr::Str(_) | Expr::Num(_) => {
                let v = self.operand(e);
                !v.trim().is_empty() && v.trim() != "0"
            }
        }
    }

    /// An operand's text.
    fn operand(&self, e: &Expr) -> String {
        match e {
            Expr::Var(v) => self.value(v),
            Expr::Str(s) => s.clone(),
            Expr::Num(n) => n.clone(),
            Expr::List(l) => self.list(l).count().to_string(),
            Expr::Node(l) => u8::from(self.list(l).count() > 0).to_string(),
            _ => u8::from(self.eval(e)).to_string(),
        }
    }
}
