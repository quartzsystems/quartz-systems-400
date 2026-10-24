// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The variables a debug view names for `EVAL`: at the start of each
//! procedure, every field it can see (the globals, and a
//! subprocedure's own) by its RPG name, data structure subfields as
//! `DS.SUB` (and by their own name when the structure is not
//! qualified), and array elements as `ARR(n)`.

use super::Gen;
use crate::names::mangle;
use crate::syms::{Kind, Storage, SymId};

/// Elements of an array named one by one, at most.
const MAX_ELEMENTS: u32 = 1000;

impl Gen<'_> {
    /// The `debug.var` lines for a scope (0 for the main procedure).
    pub fn debug_vars(&mut self, scope: usize) {
        if !self.debug {
            return;
        }
        let ids: Vec<SymId> = (0..self.t.syms.len())
            .filter(|&i| {
                let s = &self.t.syms[i];
                (s.scope == 0 || s.scope == scope)
                    && s.parent.is_none()
                    && !s.template
                    && !s.name.starts_with('_')
                    && !s.ir.is_empty()
                    && matches!(s.kind, Some(Kind::Field | Kind::Ds(_) | Kind::Param))
                    // A based field's pointer may be null on entry.
                    && !matches!(s.storage, Storage::Based(_))
            })
            .collect();
        for id in ids {
            let s = self.t.syms[id].clone();
            let place = super::place_of(self.t, id);
            self.var(&s.name, &place);
            if s.dim > 0 && s.dim <= MAX_ELEMENTS && !matches!(s.kind, Some(Kind::Ds(_))) {
                for i in 0..s.dim {
                    self.var(&format!("{}({})", s.name, i + 1), &format!("{place}[{i}]"));
                }
            }
            if let Some(Kind::Ds(di)) = s.kind
                && s.dim == 0
            {
                let ds = self.t.ds[di].clone();
                for &sub in &ds.subfields {
                    let ss = self.t.syms[sub].clone();
                    let sp = format!("{place}.{}", mangle(&ss.name));
                    self.var(&format!("{}.{}", s.name, ss.name), &sp);
                    if !ds.qualified {
                        self.var(&ss.name, &sp);
                    }
                    if ss.dim > 0 && ss.dim <= MAX_ELEMENTS {
                        for i in 0..ss.dim {
                            self.var(&format!("{}.{}({})", s.name, ss.name, i + 1), &format!("{sp}[{i}]"));
                        }
                    }
                }
            }
        }
    }

    fn var(&mut self, name: &str, place: &str) {
        self.b.ins(format!("debug.var \"{name}\", {place}"));
    }
}
