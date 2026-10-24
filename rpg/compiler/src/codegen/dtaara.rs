// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Data areas: `IN`, `OUT` and `UNLOCK` of a field or structure that
//! `DTAARA` ties to a data area, or of every one with `*DTAARA`.

use rpgfront::ast::{Expr, Op};

use super::Gen;
use super::emit::quote;
use crate::diag::ids;
use crate::syms::SymId;

impl Gen<'_> {
    /// Whether an operation's operand names a data area field.
    #[must_use]
    pub fn names_dtaara(&self, op: &Op) -> bool {
        op.f2.first().or_else(|| op.result.first()).is_some_and(|e| match e {
            Expr::Special { word, .. } => word == "*DTAARA",
            _ => e.name().and_then(|n| self.t.find(self.scope, n)).is_some_and(|id| self.t.dtaaras.contains_key(&id)),
        })
    }

    /// `IN [*LOCK] name`, `OUT [*LOCK] name`, `UNLOCK name`.
    pub fn dtaara_op(&mut self, op: &Op, base: &str) {
        let lock = op.f1.iter().any(|e| matches!(e, Expr::Special { word, .. } if word == "*LOCK"));
        let Some(target) = op.f2.first().or_else(|| op.result.first()) else {
            self.err(ids::BAD_EXPRESSION_TYPE, 30, op.code.clone());
            return;
        };
        let ids: Vec<SymId> = match target {
            Expr::Special { word, .. } if word == "*DTAARA" => {
                let mut v: Vec<SymId> = self.t.dtaaras.keys().copied().collect();
                v.sort_unstable();
                v
            }
            e => match e.name().and_then(|n| self.t.find(self.scope, n)) {
                Some(id) if self.t.dtaaras.contains_key(&id) => vec![id],
                _ => {
                    self.err(ids::NOT_DEFINED, 30, e.name().unwrap_or_default().to_string());
                    return;
                }
            },
        };
        for id in ids {
            let area = self.t.dtaaras[&id].clone();
            let (lib, name) = match area.split_once('/') {
                Some((l, n)) => (l.to_string(), n.to_string()),
                None => ("*LIBL".to_string(), area.clone()),
            };
            let (n, l) = (quote(&name.to_ascii_uppercase()), quote(&lib.to_ascii_uppercase()));
            let place = super::place_of(self.t, id);
            self.t.refer(id, self.at.line, base == "IN");
            match base {
                "IN" => {
                    if lock {
                        self.b.ins(format!("dtaara.lock {n}, {l}"));
                    }
                    self.b.ins(format!("dtaara.read {n}, {l}, {place}"));
                }
                "OUT" => {
                    self.b.ins(format!("dtaara.write {n}, {l}, {place}"));
                    // Written without *LOCK, the area is released.
                    if !lock {
                        self.b.ins(format!("dtaara.unlock {n}, {l}"));
                    }
                }
                _ => self.b.ins(format!("dtaara.unlock {n}, {l}")),
            }
        }
    }
}
