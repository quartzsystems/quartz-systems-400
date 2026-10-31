// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Orders (`docs/UPDATE_PROTOCOL.md` §4): the request and the answer as
//! text, and the resolution both the service and the client run.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use crate::catalog::Catalog;
use crate::{Error, Result};

/// `ORDER`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OrderMode {
    /// `*REQUIRED`: requisites too, superseded ones replaced.
    #[default]
    Required,
    /// `*PTFID`: only what is named.
    PtfId,
}

/// An order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OrderRequest {
    /// The system's release.
    pub release: String,
    /// `ORDER`.
    pub order: OrderMode,
    /// `REORDER(*YES)`.
    pub reorder: bool,
    /// `PTFPART(*CVRLTR)`.
    pub cover_only: bool,
    /// PTF IDs, group names, `*CUMPKG`, `*HIPER`.
    pub items: Vec<String>,
    /// PTFs the system has loaded or applied.
    pub have: Vec<String>,
}

/// Why a PTF is in an order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// Named.
    Requested,
    /// A prerequisite or co-requisite.
    Requisite,
    /// It supersedes one named.
    Supersedes,
    /// A member of a group named.
    Group,
}

impl Why {
    fn word(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::Requisite => "requisite",
            Self::Supersedes => "supersedes",
            Self::Group => "group",
        }
    }

    fn from_word(w: &str) -> Option<Self> {
        Some(match w {
            "requested" => Self::Requested,
            "requisite" => Self::Requisite,
            "supersedes" => Self::Supersedes,
            "group" => Self::Group,
            _ => return None,
        })
    }
}

/// The resolved order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OrderResult {
    /// The service's order number; empty from a local resolution.
    pub order: String,
    /// The PTFs, requisites first.
    pub ptfs: Vec<(String, Why)>,
    /// Groups named, with their level.
    pub groups: Vec<(String, u32)>,
    /// Items the catalog does not have.
    pub missing: Vec<String>,
}

const REQ_MAGIC: &str = "QS400ORDER 1";
const RES_MAGIC: &str = "QS400ORDERED 1";

impl OrderRequest {
    /// The request body.
    #[must_use]
    pub fn encode(&self) -> String {
        let mut out = format!("{REQ_MAGIC}\nrelease {}\n", self.release);
        let _ = writeln!(
            out,
            "order {}\nreorder {}\npart {}",
            match self.order {
                OrderMode::Required => "*REQUIRED",
                OrderMode::PtfId => "*PTFID",
            },
            if self.reorder { "*YES" } else { "*NO" },
            if self.cover_only { "*CVRLTR" } else { "*ALL" }
        );
        for i in &self.items {
            let _ = writeln!(out, "item {i}");
        }
        for h in &self.have {
            let _ = writeln!(out, "have {h}");
        }
        out
    }

    /// Parse a request body.
    pub fn decode(text: &str) -> Result<Self> {
        let mut lines = text.lines();
        if lines.next().map(str::trim) != Some(REQ_MAGIC) {
            return Err(Error::Protocol("not an order".into()));
        }
        let mut r = Self::default();
        for line in lines {
            let (k, v) = line.trim().split_once(' ').unwrap_or((line.trim(), ""));
            let v = v.trim();
            match k {
                "release" => r.release = v.into(),
                "order" => r.order = if v == "*PTFID" { OrderMode::PtfId } else { OrderMode::Required },
                "reorder" => r.reorder = v == "*YES",
                "part" => r.cover_only = v == "*CVRLTR",
                "item" if !v.is_empty() => r.items.push(v.to_ascii_uppercase()),
                "have" if !v.is_empty() => r.have.push(v.to_ascii_uppercase()),
                "" => {}
                _ => return Err(Error::Protocol(format!("order line {line:?}"))),
            }
        }
        Ok(r)
    }
}

impl OrderResult {
    /// The answer body.
    #[must_use]
    pub fn encode(&self) -> String {
        let mut out = format!("{RES_MAGIC}\norder {}\n", self.order);
        for (p, w) in &self.ptfs {
            let _ = writeln!(out, "ptf {p} {}", w.word());
        }
        for (g, l) in &self.groups {
            let _ = writeln!(out, "group {g} {l}");
        }
        for m in &self.missing {
            let _ = writeln!(out, "missing {m}");
        }
        out
    }

    /// Parse an answer body.
    pub fn decode(text: &str) -> Result<Self> {
        let mut lines = text.lines();
        if lines.next().map(str::trim) != Some(RES_MAGIC) {
            return Err(Error::Protocol("not an order answer".into()));
        }
        let mut r = Self::default();
        for line in lines {
            let f: Vec<&str> = line.split_whitespace().collect();
            match f.as_slice() {
                ["order", n] => r.order = (*n).into(),
                ["order"] | [] => {}
                ["ptf", p, w] => r.ptfs.push((
                    (*p).into(),
                    Why::from_word(w).ok_or_else(|| Error::Protocol(format!("order line {line:?}")))?,
                )),
                ["group", g, l] => r.groups.push((
                    (*g).into(),
                    l.parse().map_err(|_| Error::Protocol(format!("order line {line:?}")))?,
                )),
                ["missing", m] => r.missing.push((*m).into()),
                _ => return Err(Error::Protocol(format!("order line {line:?}"))),
            }
        }
        Ok(r)
    }
}

/// Who supersedes whom: each PTF to the newest PTF that supersedes it
/// directly (the highest ID when several do).
fn superseders(cat: &Catalog) -> BTreeMap<&str, &str> {
    let mut m: BTreeMap<&str, &str> = BTreeMap::new();
    for p in &cat.ptfs {
        for s in &p.supersedes {
            let e = m.entry(s.as_str()).or_insert(p.id.as_str());
            if p.id.as_str() > *e {
                *e = p.id.as_str();
            }
        }
    }
    m
}

/// `id` and the chain of PTFs superseding it, oldest first.
fn chain<'a>(sup: &BTreeMap<&str, &'a str>, id: &'a str) -> Vec<&'a str> {
    let mut out = vec![id];
    let mut cur = id;
    while let Some(next) = sup.get(cur) {
        if out.contains(next) {
            break;
        }
        out.push(next);
        cur = next;
    }
    out
}

/// Resolve an order against a catalog.
#[must_use]
pub fn resolve(cat: &Catalog, req: &OrderRequest) -> OrderResult {
    let sup = superseders(cat);
    let have: BTreeSet<&str> = req.have.iter().map(String::as_str).collect();
    let mut res = OrderResult::default();
    let mut why: BTreeMap<String, Why> = BTreeMap::new();
    let add = |id: &str, w: Why, why: &mut BTreeMap<String, Why>| {
        why.entry(id.to_string()).or_insert(w);
    };
    for item in &req.items {
        let group = match item.as_str() {
            "*CUMPKG" => cat.cumulative(&req.release),
            "*HIPER" => cat.hiper(&req.release),
            name => cat.group(name),
        };
        if let Some(g) = group {
            res.groups.push((g.name.clone(), g.level));
            for p in &g.ptfs {
                add(p, Why::Group, &mut why);
            }
        } else if cat.ptf(item).is_some() {
            add(item, Why::Requested, &mut why);
        } else {
            res.missing.push(item.clone());
        }
    }
    if req.order == OrderMode::Required {
        // Superseded PTFs give way to the newest that supersedes them.
        let named: Vec<(String, Why)> = why.iter().map(|(k, v)| (k.clone(), *v)).collect();
        why.clear();
        for (id, w) in named {
            let newest = *chain(&sup, &id).last().expect("chain");
            let w = if newest == id { w } else { Why::Supersedes };
            add(newest, w, &mut why);
        }
        // Requisites, transitively.
        let mut todo: Vec<String> = why.keys().cloned().collect();
        while let Some(id) = todo.pop() {
            let Some(p) = cat.ptf(&id) else { continue };
            for r in p.prereqs.iter().chain(&p.coreqs) {
                let newest = (*chain(&sup, r).last().expect("chain")).to_string();
                if cat.ptf(&newest).is_none() {
                    if !res.missing.contains(&newest) {
                        res.missing.push(newest);
                    }
                    continue;
                }
                if !why.contains_key(&newest) {
                    why.insert(newest.clone(), Why::Requisite);
                    todo.push(newest);
                }
            }
        }
    }
    if !req.reorder {
        why.retain(|id, _| !chain(&sup, id).iter().any(|c| have.contains(c)));
    }
    // Members the catalog does not list cannot be ordered.
    let unknown: Vec<String> = why.keys().filter(|id| cat.ptf(id).is_none()).cloned().collect();
    for u in unknown {
        why.remove(&u);
        if !res.missing.contains(&u) {
            res.missing.push(u);
        }
    }
    // Requisites first: depth-first over the prerequisites, by ID.
    let set: BTreeSet<&str> = why.keys().map(String::as_str).collect();
    let mut done = BTreeSet::new();
    let mut order = Vec::new();
    for id in &set {
        let p = cat.ptf(id).expect("listed");
        visit(cat, &p.id, &set, &mut done, &mut order);
    }
    res.ptfs = order
        .into_iter()
        .map(|id| (id.to_string(), why[id]))
        .collect();
    res
}

fn visit<'a>(
    cat: &'a Catalog,
    id: &'a str,
    set: &BTreeSet<&str>,
    done: &mut BTreeSet<&'a str>,
    order: &mut Vec<&'a str>,
) {
    if !done.insert(id) {
        return;
    }
    if let Some(p) = cat.ptf(id) {
        let mut pre: Vec<&str> = p
            .prereqs
            .iter()
            .map(String::as_str)
            .filter(|r| set.contains(r))
            .collect();
        pre.sort_unstable();
        for r in pre {
            visit(cat, r, set, done, order);
        }
    }
    order.push(id);
}
