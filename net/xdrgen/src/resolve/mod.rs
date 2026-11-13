// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Name resolution: constants evaluated, references checked (forward
//! references are fine), unions' labels checked against their
//! discriminants, list nodes found, recursive optionals boxed.
//!
//! A list node is a struct whose last field is an optional of itself
//! (`entry3 *nextentry`, or through a typedef such as `mountlist`), with
//! no other link to itself, and that is used only through such
//! optionals. Its chain is held as `Vec<S>` and coded without recursion.

mod graph;
pub mod ir;

use std::collections::{HashMap, HashSet};

use crate::error::{At, Pos, Result};
use crate::model::{Decl, DeclKind, Def, TypeBody, TypeDef, TypeSpec, Union, Value};
use ir::{Arm, Base, Disc, Kind, Model, RProc, RProgram, RVersion, Shape, Ty};

/// The model of `defs`.
pub fn resolve(defs: &[Def]) -> Result<Model> {
    let cx = Ctx::new(defs)?;
    let lists = cx.lists(defs);
    let cx = Ctx { lists, ..cx };
    let mut model = Model::default();
    for d in defs {
        match d {
            Def::Const(name, v, pos) => {
                let n = cx.eval(v, *pos)?;
                model.consts.push((name.clone(), n));
            }
            Def::Type(t) => model.types.push(Ty {
                name: t.name.clone(),
                kind: cx.kind(t)?,
                min: 0,
                line: t.pos.line,
            }),
            Def::Program(p) => model.programs.push(cx.program(p)?),
        }
    }
    cx.check_procs()?;
    graph::finish(&mut model)?;
    Ok(model)
}

struct Ctx<'a> {
    types: HashMap<&'a str, &'a TypeDef>,
    values: HashMap<String, (Value, Pos)>,
    /// Procedure names declared again, which must have the same number.
    again: Vec<(String, Value, Pos)>,
    lists: HashSet<String>,
}

/// What a type specifier comes to through typedefs.
enum Under<'a> {
    Base(Base),
    Def(&'a TypeDef),
}

fn base(ts: &TypeSpec) -> Option<Base> {
    Some(match ts {
        TypeSpec::Int => Base::Int,
        TypeSpec::UInt => Base::UInt,
        TypeSpec::Hyper => Base::Hyper,
        TypeSpec::UHyper => Base::UHyper,
        TypeSpec::Bool => Base::Bool,
        TypeSpec::Named(..) => return None,
    })
}

impl<'a> Ctx<'a> {
    fn new(defs: &'a [Def]) -> Result<Self> {
        let mut cx = Ctx {
            types: HashMap::new(),
            values: HashMap::new(),
            again: Vec::new(),
            lists: HashSet::new(),
        };
        cx.values
            .insert("TRUE".into(), (Value::Num(1), Pos::default()));
        cx.values
            .insert("FALSE".into(), (Value::Num(0), Pos::default()));
        for d in defs {
            match d {
                Def::Const(name, v, pos) => cx.value(name, v.clone(), *pos)?,
                Def::Type(t) => {
                    if cx.types.insert(&t.name, t).is_some() {
                        return Err(At::new(t.pos, format!("type `{}` defined twice", t.name)));
                    }
                    if let TypeBody::Enum(members) = &t.body {
                        for (name, v, pos) in members {
                            cx.value(name, v.clone(), *pos)?;
                        }
                    }
                }
                Def::Program(p) => {
                    cx.value(&p.name, p.number.clone(), p.pos)?;
                    for v in &p.versions {
                        cx.value(&v.name, v.number.clone(), v.pos)?;
                        for pr in &v.procs {
                            if cx.values.contains_key(&pr.name) {
                                cx.again.push((pr.name.clone(), pr.number.clone(), pr.pos));
                            } else {
                                cx.value(&pr.name, pr.number.clone(), pr.pos)?;
                            }
                        }
                    }
                }
            }
        }
        Ok(cx)
    }

    fn value(&mut self, name: &str, v: Value, pos: Pos) -> Result<()> {
        if self.values.insert(name.to_string(), (v, pos)).is_some() {
            return Err(At::new(pos, format!("`{name}` defined twice")));
        }
        Ok(())
    }

    fn eval(&self, v: &Value, pos: Pos) -> Result<i128> {
        let mut v = v;
        let mut pos = pos;
        for _ in 0..64 {
            match v {
                Value::Num(n) => return Ok(*n),
                Value::Name(n, p) => {
                    let Some((next, _)) = self.values.get(n) else {
                        return Err(At::new(*p, format!("unknown constant `{n}`")));
                    };
                    pos = *p;
                    v = next;
                }
            }
        }
        Err(At::new(pos, "a constant defined by itself"))
    }

    fn bound(&self, v: &Value, pos: Pos) -> Result<u32> {
        let n = self.eval(v, pos)?;
        u32::try_from(n).map_err(|_| At::new(pos, format!("bound {n} outside 0 to 4294967295")))
    }

    fn check_procs(&self) -> Result<()> {
        for (name, v, pos) in &self.again {
            let first = self.values.get(name).map(|(v, p)| (v.clone(), *p));
            let Some((fv, fp)) = first else { continue };
            if self.eval(&fv, fp)? != self.eval(v, *pos)? {
                return Err(At::new(
                    *pos,
                    format!("`{name}` defined twice with different numbers"),
                ));
            }
        }
        Ok(())
    }

    fn named(&self, name: &str, pos: Pos) -> Result<&'a TypeDef> {
        self.types
            .get(name)
            .copied()
            .ok_or_else(|| At::new(pos, format!("unknown type `{name}`")))
    }

    /// `ts` through plain typedefs.
    fn under(&self, ts: &TypeSpec) -> Result<Under<'a>> {
        let mut ts = ts.clone();
        for _ in 0..64 {
            if let Some(b) = base(&ts) {
                return Ok(Under::Base(b));
            }
            let TypeSpec::Named(n, pos) = &ts else {
                unreachable!()
            };
            let t = self.named(n, *pos)?;
            match &t.body {
                TypeBody::Typedef(Decl {
                    kind: DeclKind::Plain(next),
                    ..
                }) => ts = next.clone(),
                _ => return Ok(Under::Def(t)),
            }
        }
        Err(At::new(Pos::default(), "a typedef defined by itself"))
    }

    /// The struct `decl` is an optional of, directly or through a typedef.
    fn optional_of(&self, decl: &Decl) -> Option<String> {
        match &decl.kind {
            DeclKind::Optional(TypeSpec::Named(n, _)) => Some(n.clone()),
            DeclKind::Plain(TypeSpec::Named(n, _)) => match &self.types.get(n.as_str())?.body {
                TypeBody::Typedef(Decl {
                    kind: DeclKind::Optional(TypeSpec::Named(s, _)),
                    ..
                }) => Some(s.clone()),
                _ => None,
            },
            _ => None,
        }
    }

    fn lists(&self, defs: &[Def]) -> HashSet<String> {
        let mut candidates = HashSet::new();
        let mut direct = HashSet::new();
        let mut by_procs = Vec::new();
        let mut used = |d: &Decl| match &d.kind {
            DeclKind::Plain(TypeSpec::Named(n, _))
            | DeclKind::FixedArray(TypeSpec::Named(n, _), _)
            | DeclKind::VarArray(TypeSpec::Named(n, _), _) => {
                direct.insert(n.clone());
            }
            _ => {}
        };
        for d in defs {
            match d {
                Def::Type(t) => match &t.body {
                    TypeBody::Struct(fields) => {
                        let links = fields
                            .iter()
                            .filter(|f| self.optional_of(f).as_deref() == Some(t.name.as_str()))
                            .count();
                        let last = fields.last().and_then(|f| self.optional_of(f));
                        if links == 1 && last.as_deref() == Some(t.name.as_str()) {
                            candidates.insert(t.name.clone());
                        }
                        fields.iter().for_each(&mut used);
                    }
                    TypeBody::Union(u) => {
                        u.cases.iter().for_each(|c| used(&c.decl));
                        u.default.iter().for_each(&mut used);
                    }
                    TypeBody::Typedef(d) => used(d),
                    TypeBody::Enum(_) => {}
                },
                Def::Program(p) => {
                    for pr in p.versions.iter().flat_map(|v| &v.procs) {
                        for ts in [&pr.arg, &pr.result].into_iter().flatten() {
                            if let TypeSpec::Named(n, _) = ts {
                                by_procs.push(n.clone());
                            }
                        }
                    }
                }
                Def::Const(..) => {}
            }
        }
        direct.extend(by_procs);
        candidates.retain(|c| !direct.contains(c));
        candidates
    }

    fn spec(&self, ts: &TypeSpec) -> Result<Shape> {
        if let Some(b) = base(ts) {
            return Ok(Shape::Base(b));
        }
        let TypeSpec::Named(n, pos) = ts else {
            unreachable!()
        };
        self.named(n, *pos)?;
        Ok(Shape::Named(n.clone()))
    }

    fn shape(&self, d: &Decl) -> Result<Option<Shape>> {
        let p = d.pos;
        Ok(Some(match &d.kind {
            DeclKind::Void => return Ok(None),
            DeclKind::Plain(ts) => self.spec(ts)?,
            DeclKind::FixedArray(ts, v) => {
                Shape::FixedArray(Box::new(self.spec(ts)?), self.bound(v, p)?)
            }
            DeclKind::VarArray(ts, v) => Shape::VarArray(
                Box::new(self.spec(ts)?),
                v.as_ref().map_or(Ok(u32::MAX), |v| self.bound(v, p))?,
            ),
            DeclKind::FixedOpaque(v) => Shape::FixedOpaque(self.bound(v, p)?),
            DeclKind::VarOpaque(v) | DeclKind::String(v) => {
                Shape::VarOpaque(v.as_ref().map_or(Ok(u32::MAX), |v| self.bound(v, p))?)
            }
            DeclKind::Optional(ts) => match ts {
                TypeSpec::Named(n, _) if self.lists.contains(n) => Shape::List(n.clone()),
                _ => Shape::Optional(Box::new(self.spec(ts)?), false),
            },
        }))
    }

    fn kind(&self, t: &TypeDef) -> Result<Kind> {
        match &t.body {
            TypeBody::Enum(members) => {
                let mut out: Vec<(String, i32)> = Vec::new();
                for (name, v, pos) in members {
                    let n = self.eval(v, *pos)?;
                    let n = i32::try_from(n)
                        .map_err(|_| At::new(*pos, format!("enum value {n} outside int")))?;
                    if let Some((other, _)) = out.iter().find(|(_, x)| *x == n) {
                        return Err(At::new(
                            *pos,
                            format!("`{name}` has the value of `{other}`"),
                        ));
                    }
                    out.push((name.clone(), n));
                }
                Ok(Kind::Enum(out))
            }
            TypeBody::Struct(fields) => {
                let mut fields: &[Decl] = fields;
                if self.lists.contains(&t.name) {
                    fields = fields.split_last().map_or(fields, |(_, rest)| rest);
                }
                let mut out: Vec<(String, Shape)> = Vec::new();
                for f in fields {
                    let name = f.name.clone().unwrap_or_default();
                    if out.iter().any(|(n, _)| *n == name) {
                        return Err(At::new(f.pos, format!("field `{name}` declared twice")));
                    }
                    let shape = self
                        .shape(f)?
                        .ok_or_else(|| At::new(f.pos, "void in a struct"))?;
                    out.push((name, shape));
                }
                Ok(Kind::Struct(out))
            }
            TypeBody::Union(u) => self.union(u),
            TypeBody::Typedef(d) => {
                let shape = self
                    .shape(d)?
                    .ok_or_else(|| At::new(d.pos, "a typedef of void"))?;
                Ok(match shape {
                    Shape::Base(_)
                    | Shape::Named(_)
                    | Shape::FixedOpaque(_)
                    | Shape::Optional(..) => Kind::Alias(shape),
                    _ => Kind::Newtype(shape),
                })
            }
        }
    }

    fn union(&self, u: &Union) -> Result<Kind> {
        let DeclKind::Plain(dts) = &u.disc.kind else {
            return Err(At::new(
                u.disc.pos,
                "a union's discriminant must be a plain declaration",
            ));
        };
        let bad = || {
            At::new(
                u.disc.pos,
                "a union's discriminant must be int, unsigned int, bool or an enum",
            )
        };
        let disc = match self.under(dts)? {
            Under::Base(Base::Int) => Disc::Int,
            Under::Base(Base::UInt) => Disc::UInt,
            Under::Base(Base::Bool) => Disc::Bool,
            Under::Base(_) => return Err(bad()),
            Under::Def(d) => match &d.body {
                TypeBody::Enum(_) => Disc::Enum(d.name.clone()),
                _ => return Err(bad()),
            },
        };
        let members: Vec<(String, i128)> = match &disc {
            Disc::Enum(e) => match &self.named(e, u.disc.pos)?.body {
                TypeBody::Enum(m) => m
                    .iter()
                    .map(|(n, v, p)| Ok((n.clone(), self.eval(v, *p)?)))
                    .collect::<Result<_>>()?,
                _ => Vec::new(),
            },
            _ => Vec::new(),
        };
        let mut arms: Vec<Arm> = Vec::new();
        for c in &u.cases {
            let shape = self.shape(&c.decl)?;
            for (label, pos) in &c.labels {
                let value = self.eval(label, *pos)?;
                let ok = match disc {
                    Disc::Int => i32::try_from(value).is_ok(),
                    Disc::UInt => u32::try_from(value).is_ok(),
                    Disc::Bool => value == 0 || value == 1,
                    Disc::Enum(_) => members.iter().any(|(_, v)| *v == value),
                };
                if !ok {
                    return Err(At::new(
                        *pos,
                        format!("case {value} is not a value of the discriminant"),
                    ));
                }
                let member = members
                    .iter()
                    .find(|(_, v)| *v == value)
                    .map(|(n, _)| n.clone());
                let variant = match (label, &disc) {
                    (Value::Name(n, _), _) => n.clone(),
                    (Value::Num(_), Disc::Bool) => {
                        if value == 1 { "TRUE" } else { "FALSE" }.to_string()
                    }
                    (Value::Num(_), Disc::Enum(_)) => member.clone().unwrap_or_default(),
                    (Value::Num(n), _) if *n < 0 => format!("VM{}", -n),
                    (Value::Num(n), _) => format!("V{n}"),
                };
                if arms.iter().any(|a| a.value == value) {
                    return Err(At::new(*pos, format!("case {value} given twice")));
                }
                if arms.iter().any(|a| a.variant == variant) {
                    return Err(At::new(*pos, format!("case `{variant}` given twice")));
                }
                arms.push(Arm {
                    variant,
                    value,
                    member,
                    shape: shape.clone(),
                });
            }
        }
        let default = u.default.as_ref().map(|d| self.shape(d)).transpose()?;
        // A bool union of TRUE: T and FALSE: void is an Option.
        if disc == Disc::Bool && default.is_none() && arms.len() == 2 {
            let t_arm = arms.iter().find(|a| a.value == 1);
            let f_arm = arms.iter().find(|a| a.value == 0);
            if let (Some(ta), Some(fa)) = (t_arm, f_arm)
                && fa.shape.is_none()
                && let Some(s @ (Shape::Base(_) | Shape::Named(_))) = &ta.shape
            {
                return Ok(Kind::Alias(Shape::Optional(Box::new(s.clone()), false)));
            }
        }
        Ok(Kind::Union {
            disc,
            arms,
            default,
        })
    }

    fn program(&self, p: &crate::model::Program) -> Result<RProgram> {
        let number = self.bound(&p.number, p.pos)?;
        let mut versions: Vec<RVersion> = Vec::new();
        for v in &p.versions {
            let vn = self.bound(&v.number, v.pos)?;
            if versions.iter().any(|x| x.number == vn) {
                return Err(At::new(v.pos, format!("version {vn} given twice")));
            }
            let mut procs: Vec<RProc> = Vec::new();
            for pr in &v.procs {
                let n = self.bound(&pr.number, pr.pos)?;
                if procs.iter().any(|x| x.number == n || x.name == pr.name) {
                    return Err(At::new(pr.pos, format!("procedure {n} given twice")));
                }
                procs.push(RProc {
                    name: pr.name.clone(),
                    number: n,
                    arg: pr.arg.as_ref().map(|t| self.spec(t)).transpose()?,
                    result: pr.result.as_ref().map(|t| self.spec(t)).transpose()?,
                });
            }
            versions.push(RVersion {
                name: v.name.clone(),
                number: vn,
                procs,
            });
        }
        Ok(RProgram {
            name: p.name.clone(),
            number,
            versions,
        })
    }
}

#[cfg(test)]
mod tests;
