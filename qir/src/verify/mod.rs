// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The verifier: a module that passes has well-formed types, resolvable
//! names, consistent register types, every register assigned before it
//! is used on every path, blocks that end in one terminator, and
//! instructions whose operands fit.

pub mod place;
pub mod typing;

mod flow;
mod inst;

use std::collections::HashSet;

use crate::error::{QirError, Result};
use crate::module::{Module, Param, Proc, Scope, StorageClass, Var, index32};
use crate::types::{Type, TypeClass};

/// Verify a module.
pub fn verify(m: &Module) -> Result<()> {
    module_level(m)?;
    for p in &m.procs {
        proc_level(m, p)?;
    }
    Ok(())
}

fn err(at: impl Into<String>, msg: impl Into<String>) -> QirError {
    QirError::verify(at, msg)
}

fn module_level(m: &Module) -> Result<()> {
    let at = "module";
    if m.name.is_empty() || m.name.len() > 10 {
        return Err(err(at, "module name is 1 to 10 characters"));
    }
    let mut names = HashSet::new();
    for t in &m.types {
        if !names.insert(t.name.as_str()) {
            return Err(err(at, format!("type {} is declared twice", t.name)));
        }
        t.ty.check().map_err(|e| err(at, e.to_string()))?;
    }
    vars(m, None, &m.vars, at)?;
    let mut files = HashSet::new();
    for f in &m.files {
        if !files.insert(f.name.as_str()) {
            return Err(err(at, format!("file {} is declared twice", f.name)));
        }
        for r in &f.formats {
            if let Some(l) = &r.level
                && (l.len() != 13 || !l.bytes().all(|b| b.is_ascii_hexdigit()))
            {
                return Err(err(
                    at,
                    format!("format {} level {l} is not 13 hexadecimal digits", r.name),
                ));
            }
        }
    }
    let mut callables = HashSet::new();
    for p in &m.protos {
        if !callables.insert(p.name.as_str()) {
            return Err(err(at, format!("{} is declared twice", p.name)));
        }
        params(&p.params, p.ret.as_ref(), &format!("proto {}", p.name))?;
    }
    let mut entries = 0;
    for p in &m.procs {
        if !callables.insert(p.name.as_str()) {
            return Err(err(at, format!("{} is declared twice", p.name)));
        }
        if p.entry {
            entries += 1;
        }
    }
    if entries > 1 {
        return Err(err(at, "more than one entry procedure"));
    }
    if entries == 1 && m.nomain {
        return Err(err(at, "a nomain module has no entry procedure"));
    }
    Ok(())
}

fn params(params: &[Param], ret: Option<&Type>, at: &str) -> Result<()> {
    let mut names = HashSet::new();
    let mut optional = false;
    for p in params {
        if !names.insert(p.name.as_str()) {
            return Err(err(at, format!("parameter {} is declared twice", p.name)));
        }
        p.ty.check().map_err(|e| err(at, e.to_string()))?;
        if p.nopass {
            optional = true;
        } else if optional {
            return Err(err(
                at,
                format!(
                    "parameter {} follows a *NOPASS parameter and is not *NOPASS",
                    p.name
                ),
            ));
        }
        if p.by_value && (p.varsize || p.const_) {
            return Err(err(
                at,
                format!("by-value parameter {} cannot be *VARSIZE or const", p.name),
            ));
        }
        if p.varsize && !p.ty.is_string() && p.ty.class() != TypeClass::Aggregate {
            return Err(err(
                at,
                format!("*VARSIZE parameter {} is not text or an aggregate", p.name),
            ));
        }
    }
    if let Some(r) = ret {
        r.check().map_err(|e| err(at, e.to_string()))?;
    }
    Ok(())
}

fn vars(m: &Module, proc: Option<&Proc>, vars: &[Var], at: &str) -> Result<()> {
    let mut names = HashSet::new();
    for v in vars {
        if !names.insert(v.name.as_str()) {
            return Err(err(at, format!("variable {} is declared twice", v.name)));
        }
        v.ty.check()
            .map_err(|e| err(at, format!("${}: {e}", v.name)))?;
        let in_proc = proc.is_some();
        match &v.class {
            StorageClass::Auto if !in_proc => {
                return Err(err(at, format!("${} is auto outside a procedure", v.name)));
            }
            StorageClass::Import if in_proc => {
                return Err(err(
                    at,
                    format!("${} is an import inside a procedure", v.name),
                ));
            }
            StorageClass::Param(k) => {
                let p = proc
                    .and_then(|p| p.params.get(*k as usize))
                    .ok_or_else(|| {
                        err(
                            at,
                            format!("${} is parameter {k}, which does not exist", v.name),
                        )
                    })?;
                if p.name != v.name || p.ty != v.ty {
                    return Err(err(at, format!("${} does not match parameter {k}", v.name)));
                }
            }
            StorageClass::Based(on) => {
                if on.scope == Scope::Proc && !in_proc {
                    return Err(err(
                        at,
                        format!("${} is based on a procedure variable", v.name),
                    ));
                }
                let b = m.var(*on, proc).ok_or_else(|| {
                    err(
                        at,
                        format!("${} is based on a variable that does not exist", v.name),
                    )
                })?;
                if b.ty != Type::Ptr {
                    return Err(err(
                        at,
                        format!("${} is based on ${}, which is not a ptr", v.name, b.name),
                    ));
                }
            }
            StorageClass::Static
            | StorageClass::Auto
            | StorageClass::Import
            | StorageClass::Const => {}
        }
        if v.export && v.class != StorageClass::Static {
            return Err(err(at, format!("${} is exported and not static", v.name)));
        }
        if v.class == StorageClass::Const && v.inz.is_none() {
            return Err(err(at, format!("const ${} has no value", v.name)));
        }
        if let Some(b) = &v.inz {
            if !matches!(
                v.class,
                StorageClass::Static | StorageClass::Auto | StorageClass::Const
            ) {
                return Err(err(
                    at,
                    format!(
                        "${} is {} and has an initializer",
                        v.name,
                        v.class.keyword()
                    ),
                ));
            }
            if b.len() as u64 != v.ty.size() {
                return Err(err(
                    at,
                    format!(
                        "${} initializer is {} bytes, the type {}",
                        v.name,
                        b.len(),
                        v.ty.size()
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn proc_level(m: &Module, p: &Proc) -> Result<()> {
    let at = format!("proc {}", p.name);
    params(&p.params, p.ret.as_ref(), &at)?;
    if p.entry {
        if p.ret.is_some() {
            return Err(err(&at, "the entry procedure returns nothing"));
        }
        if p.params.iter().any(|x| x.by_value) {
            return Err(err(&at, "entry parameters are by reference"));
        }
    }
    // Every parameter is a variable, in order, before anything else.
    for (k, param) in p.params.iter().enumerate() {
        match p.vars.get(k) {
            Some(v) if v.class == StorageClass::Param(index32(k)) && v.name == param.name => {}
            _ => {
                return Err(err(
                    &at,
                    format!("parameter {} has no variable at index {k}", param.name),
                ));
            }
        }
    }
    vars(m, Some(p), &p.vars, &at)?;
    for r in &p.regs {
        if !r.ty.is_register() {
            return Err(err(
                &at,
                format!("%{} is a {}, which no register holds", r.name, r.ty),
            ));
        }
    }
    if p.blocks.is_empty() {
        return Err(err(&at, "no blocks"));
    }
    let mut labels = HashSet::new();
    for b in &p.blocks {
        if !labels.insert(b.label.as_str()) {
            return Err(err(&at, format!("label {} is declared twice", b.label)));
        }
    }
    for b in &p.blocks {
        let at_b = format!("{at} block {}", b.label);
        match b.insts.last() {
            Some(i) if i.is_terminator() => {}
            _ => return Err(err(&at_b, "block does not end in a terminator")),
        }
        for (ii, i) in b.insts.iter().enumerate() {
            let at_i = format!("{at_b} inst {}", ii + 1);
            if i.is_terminator() && ii + 1 != b.insts.len() {
                return Err(err(&at_i, "terminator before the end of the block"));
            }
            for l in i.targets() {
                if l.0 as usize >= p.blocks.len() {
                    return Err(err(&at_i, "branch to a label that does not exist"));
                }
            }
            inst::check(m, p, i).map_err(|msg| err(&at_i, msg))?;
        }
    }
    flow::check_assigned(p).map_err(|(b, i, msg)| err(format!("{at} block {b} inst {i}"), msg))?;
    Ok(())
}
