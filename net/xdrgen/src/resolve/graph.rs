// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the types' containment says: which optionals need a box, which
//! types contain themselves with no optional between (an error), the
//! fewest bytes each type encodes to, and arrays of empty elements (an
//! error: a count could then cost no bytes).

use std::collections::{HashMap, HashSet};

use super::ir::{Base, Kind, Model, Shape};
use crate::error::{At, Pos, Result};

/// Boxes, cycles, sizes.
pub fn finish(model: &mut Model) -> Result<()> {
    let edges = edges(model);
    let line: HashMap<String, u32> = model
        .types
        .iter()
        .map(|t| (t.name.clone(), t.line))
        .collect();
    // A type that reaches itself through inline items only is infinite.
    for t in &model.types {
        if reaches(&edges, &t.name, &t.name, false) {
            return Err(At::new(
                Pos {
                    line: t.line,
                    col: 1,
                },
                format!("`{}` contains itself without an optional", t.name),
            ));
        }
    }
    for t in &mut model.types {
        for s in shapes_mut(&mut t.kind) {
            box_optionals(s, &edges);
        }
    }
    let mut mins = HashMap::new();
    for i in 0..model.types.len() {
        let name = model.types[i].name.clone();
        let m = min_of(model, &name, &mut mins);
        model.types[i].min = m;
    }
    for t in &model.types {
        for s in shapes(&t.kind) {
            if let Some(e) = empty_element(s, &mins) {
                let line = line.get(&t.name).copied().unwrap_or(0);
                return Err(At::new(
                    Pos { line, col: 1 },
                    format!("an array of `{e}`, which encodes to no bytes"),
                ));
            }
        }
    }
    Ok(())
}

/// Each type's inline references: (target, through an optional).
type Edges = HashMap<String, Vec<(String, bool)>>;

fn edges(model: &Model) -> Edges {
    let mut out = Edges::new();
    for t in &model.types {
        let mut v = Vec::new();
        for s in shapes(&t.kind) {
            inline_refs(s, false, &mut v);
        }
        out.insert(t.name.clone(), v);
    }
    out
}

fn inline_refs(s: &Shape, opt: bool, out: &mut Vec<(String, bool)>) {
    match s {
        Shape::Named(n) => out.push((n.clone(), opt)),
        Shape::Optional(inner, _) => inline_refs(inner, true, out),
        _ => {}
    }
}

/// Whether `to` is reached from `from` in one step or more; through
/// optionals too when `opt`.
fn reaches(edges: &Edges, from: &str, to: &str, opt: bool) -> bool {
    let mut seen = HashSet::new();
    let mut stack = vec![from.to_string()];
    while let Some(n) = stack.pop() {
        for (m, o) in edges.get(&n).map(Vec::as_slice).unwrap_or_default() {
            if *o && !opt {
                continue;
            }
            if m == to {
                return true;
            }
            if seen.insert(m.clone()) {
                stack.push(m.clone());
            }
        }
    }
    false
}

fn box_optionals(s: &mut Shape, edges: &Edges) {
    match s {
        Shape::Optional(inner, boxed) => {
            if let Shape::Named(n) = inner.as_ref() {
                *boxed = reaches(edges, n, n, true);
            }
            box_optionals(inner, edges);
        }
        Shape::FixedArray(e, _) | Shape::VarArray(e, _) => box_optionals(e, edges),
        _ => {}
    }
}

fn shapes(k: &Kind) -> Vec<&Shape> {
    match k {
        Kind::Enum(_) => Vec::new(),
        Kind::Struct(f) => f.iter().map(|(_, s)| s).collect(),
        Kind::Union { arms, default, .. } => arms
            .iter()
            .filter_map(|a| a.shape.as_ref())
            .chain(default.iter().flatten())
            .collect(),
        Kind::Alias(s) | Kind::Newtype(s) => vec![s],
    }
}

fn shapes_mut(k: &mut Kind) -> Vec<&mut Shape> {
    match k {
        Kind::Enum(_) => Vec::new(),
        Kind::Struct(f) => f.iter_mut().map(|(_, s)| s).collect(),
        Kind::Union { arms, default, .. } => arms
            .iter_mut()
            .filter_map(|a| a.shape.as_mut())
            .chain(default.iter_mut().flatten())
            .collect(),
        Kind::Alias(s) | Kind::Newtype(s) => vec![s],
    }
}

fn pad(n: u32) -> u64 {
    u64::from(n).div_ceil(4) * 4
}

fn min_of(model: &Model, name: &str, mins: &mut HashMap<String, u64>) -> u64 {
    if let Some(m) = mins.get(name) {
        return *m;
    }
    // Cycles go through optionals, which cost 4 bytes without looking in.
    let Some(t) = model.ty(name) else { return 0 };
    let m = match &t.kind {
        Kind::Enum(_) => 4,
        Kind::Struct(f) => f
            .iter()
            .map(|(_, s)| min_shape(model, s, mins))
            .fold(0u64, u64::saturating_add),
        Kind::Union { arms, default, .. } => {
            let arm = arms
                .iter()
                .map(|a| a.shape.as_ref())
                .chain(default.iter().map(Option::as_ref))
                .map(|s| s.map_or(0, |s| min_shape(model, s, mins)))
                .min()
                .unwrap_or(0);
            4 + arm
        }
        Kind::Alias(s) | Kind::Newtype(s) => min_shape(model, s, mins),
    };
    mins.insert(name.to_string(), m);
    m
}

fn min_shape(model: &Model, s: &Shape, mins: &mut HashMap<String, u64>) -> u64 {
    match s {
        Shape::Base(Base::Hyper | Base::UHyper) => 8,
        Shape::Base(_)
        | Shape::VarOpaque(_)
        | Shape::VarArray(..)
        | Shape::Optional(..)
        | Shape::List(_) => 4,
        Shape::Named(n) => min_of(model, n, mins),
        Shape::FixedOpaque(n) => pad(*n),
        Shape::FixedArray(e, n) => min_shape(model, e, mins).saturating_mul(u64::from(*n)),
    }
}

fn elem_min(s: &Shape, mins: &HashMap<String, u64>) -> u64 {
    match s {
        Shape::Named(n) => mins.get(n).copied().unwrap_or(0),
        Shape::Base(Base::Hyper | Base::UHyper) => 8,
        Shape::FixedOpaque(n) => pad(*n),
        _ => 4,
    }
}

fn empty_element(s: &Shape, mins: &HashMap<String, u64>) -> Option<String> {
    match s {
        Shape::FixedArray(e, _) | Shape::VarArray(e, _) if elem_min(e, mins) == 0 => {
            Some(format!("{e:?}"))
        }
        Shape::List(n) if mins.get(n).copied().unwrap_or(0) == 0 => Some(n.clone()),
        _ => None,
    }
}
