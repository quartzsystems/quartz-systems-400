// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The parser: `.qir` text to a [`Module`]. Names resolve as they are
//! met; labels resolve at the end of each procedure; registers take
//! their type at their first definition.

mod decl;
mod ext;
mod inst;
mod types;

use std::collections::HashMap;

use super::lex::{Tok, Token, lex};
use crate::error::{QirError, Result};
use crate::module::{Label, Module, Proc, Reg, RegInfo, Scope, VarRef, index32};
use crate::types::Type;

/// The placeholder type of a register before its first definition.
/// `bytes(0)` is never a valid type, so it cannot be mistaken for one.
pub(crate) const UNTYPED: Type = Type::Bytes(0);

/// Parse a module from text.
pub fn parse(text: &str) -> Result<Module> {
    let toks = lex(text)?;
    let mut p = Parser {
        toks,
        at: 0,
        m: Module::new(""),
        proc: None,
        labels: HashMap::new(),
        label_order: Vec::new(),
    };
    p.module()?;
    Ok(p.m)
}

pub(crate) struct Parser {
    toks: Vec<Token>,
    at: usize,
    pub(crate) m: Module,
    /// The procedure being parsed.
    pub(crate) proc: Option<Proc>,
    /// Labels mentioned in the current procedure, by first mention.
    labels: HashMap<String, u32>,
    label_order: Vec<String>,
}

impl Parser {
    // Token access.

    pub(crate) fn peek(&self) -> &Tok {
        &self.toks[self.at].tok
    }

    pub(crate) fn peek_at(&self, n: usize) -> &Tok {
        let i = (self.at + n).min(self.toks.len() - 1);
        &self.toks[i].tok
    }

    pub(crate) fn bump(&mut self) -> Tok {
        let t = self.toks[self.at].tok.clone();
        if self.at + 1 < self.toks.len() {
            self.at += 1;
        }
        t
    }

    pub(crate) fn err(&self, msg: impl Into<String>) -> QirError {
        let t = &self.toks[self.at];
        QirError::syntax(t.line, t.col, msg)
    }

    pub(crate) fn unexpected(&self, expected: &str) -> QirError {
        self.err(format!(
            "expected {expected}, found {}",
            self.peek().describe()
        ))
    }

    pub(crate) fn is_sym(&self, s: &str) -> bool {
        matches!(self.peek(), Tok::Sym(x) if *x == s)
    }

    pub(crate) fn is_word(&self, w: &str) -> bool {
        matches!(self.peek(), Tok::Ident(x) if x == w)
    }

    pub(crate) fn eat_sym(&mut self, s: &str) -> bool {
        if self.is_sym(s) {
            self.bump();
            true
        } else {
            false
        }
    }

    pub(crate) fn eat_word(&mut self, w: &str) -> bool {
        if self.is_word(w) {
            self.bump();
            true
        } else {
            false
        }
    }

    pub(crate) fn expect_sym(&mut self, s: &str) -> Result<()> {
        if self.eat_sym(s) {
            Ok(())
        } else {
            Err(self.unexpected(&format!("'{s}'")))
        }
    }

    pub(crate) fn expect_word(&mut self, w: &str) -> Result<()> {
        if self.eat_word(w) {
            Ok(())
        } else {
            Err(self.unexpected(w))
        }
    }

    pub(crate) fn ident(&mut self) -> Result<String> {
        match self.peek() {
            Tok::Ident(s) => {
                let s = s.clone();
                self.bump();
                Ok(s)
            }
            _ => Err(self.unexpected("a name")),
        }
    }

    pub(crate) fn var_name(&mut self) -> Result<String> {
        match self.peek() {
            Tok::Var(s) => {
                let s = s.clone();
                self.bump();
                Ok(s)
            }
            _ => Err(self.unexpected("a $variable")),
        }
    }

    pub(crate) fn reg_name(&mut self) -> Result<String> {
        match self.peek() {
            Tok::Reg(s) => {
                let s = s.clone();
                self.bump();
                Ok(s)
            }
            _ => Err(self.unexpected("a %register")),
        }
    }

    pub(crate) fn int(&mut self) -> Result<i128> {
        match self.peek() {
            Tok::Int(v) => {
                let v = *v;
                self.bump();
                Ok(v)
            }
            _ => Err(self.unexpected("an integer")),
        }
    }

    pub(crate) fn u32(&mut self) -> Result<u32> {
        let v = self.int()?;
        u32::try_from(v).map_err(|_| self.err(format!("{v} is out of range")))
    }

    pub(crate) fn string(&mut self) -> Result<String> {
        match self.peek() {
            Tok::Str(s) => {
                let s = s.clone();
                self.bump();
                Ok(s)
            }
            _ => Err(self.unexpected("a string")),
        }
    }

    /// Skip newlines; used inside brackets.
    pub(crate) fn skip_newlines(&mut self) {
        while *self.peek() == Tok::Newline {
            self.bump();
        }
    }

    /// The line must end here.
    pub(crate) fn end_line(&mut self) -> Result<()> {
        match self.peek() {
            Tok::Newline => {
                self.bump();
                Ok(())
            }
            Tok::End => Ok(()),
            _ => Err(self.unexpected("end of line")),
        }
    }

    pub(crate) fn at_end(&self) -> bool {
        *self.peek() == Tok::End
    }

    /// `word(` … `)` with the contents parsed by `f`.
    pub(crate) fn paren<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        self.expect_sym("(")?;
        let v = f(self)?;
        self.expect_sym(")")?;
        Ok(v)
    }

    // Names.

    /// A variable by name: the procedure's first, then the module's.
    pub(crate) fn lookup_var(&self, name: &str) -> Option<VarRef> {
        if let Some(i) = self
            .proc
            .as_ref()
            .and_then(|p| p.vars.iter().position(|v| v.name == name))
        {
            return Some(VarRef {
                scope: Scope::Proc,
                index: index32(i),
            });
        }
        self.m
            .vars
            .iter()
            .position(|v| v.name == name)
            .map(|i| VarRef {
                scope: Scope::Module,
                index: index32(i),
            })
    }

    pub(crate) fn var_ref(&mut self) -> Result<VarRef> {
        let name = self.var_name()?;
        self.lookup_var(&name)
            .ok_or_else(|| self.err(format!("variable ${name} is not declared")))
    }

    pub(crate) fn cur_proc(&self) -> &Proc {
        self.proc.as_ref().expect("inside a procedure")
    }

    pub(crate) fn cur_proc_mut(&mut self) -> &mut Proc {
        self.proc.as_mut().expect("inside a procedure")
    }

    /// A register by name, created untyped at first mention.
    pub(crate) fn reg(&mut self, name: &str) -> Reg {
        let p = self.cur_proc_mut();
        if let Some(r) = p.reg_named(name) {
            return r;
        }
        p.regs.push(RegInfo {
            name: name.to_string(),
            ty: UNTYPED,
        });
        Reg(index32(p.regs.len() - 1))
    }

    pub(crate) fn reg_operand(&mut self) -> Result<Reg> {
        let name = self.reg_name()?;
        Ok(self.reg(&name))
    }

    /// Give a register its type at a definition.
    pub(crate) fn type_reg(&mut self, r: Reg, ty: Type) -> Result<()> {
        if !ty.is_register() {
            return Err(self.err(format!("a register cannot hold a {ty}")));
        }
        let p = self.cur_proc_mut();
        let info = &mut p.regs[r.0 as usize];
        if info.ty == UNTYPED {
            info.ty = ty;
            Ok(())
        } else if info.ty == ty {
            Ok(())
        } else {
            let msg = format!(
                "%{} is a {} and is assigned a {ty} here",
                info.name, info.ty
            );
            Err(self.err(msg))
        }
    }

    /// A label by name, interned at first mention.
    pub(crate) fn label(&mut self, name: &str) -> Label {
        if let Some(&i) = self.labels.get(name) {
            return Label(i);
        }
        let i = index32(self.label_order.len());
        self.labels.insert(name.to_string(), i);
        self.label_order.push(name.to_string());
        Label(i)
    }

    pub(crate) fn label_operand(&mut self) -> Result<Label> {
        let name = self.ident()?;
        Ok(self.label(&name))
    }

    pub(crate) fn begin_proc(&mut self, p: Proc) {
        self.proc = Some(p);
        self.labels.clear();
        self.label_order.clear();
    }

    /// Finish the procedure: map interned labels to block order and
    /// check every register has a type.
    pub(crate) fn end_proc(&mut self) -> Result<Proc> {
        let mut p = self.proc.take().expect("inside a procedure");
        let mut map = vec![None; self.label_order.len()];
        for (bi, b) in p.blocks.iter().enumerate() {
            let i = self.labels[&b.label] as usize;
            map[i] = Some(Label(index32(bi)));
        }
        for (name, i) in &self.labels {
            if map[*i as usize].is_none() {
                return Err(self.err(format!("label {name} is not defined in {}", p.name)));
            }
        }
        let map: Vec<Label> = map.into_iter().map(|l| l.expect("mapped")).collect();
        for b in &mut p.blocks {
            for i in &mut b.insts {
                inst::remap_labels(i, &map);
            }
        }
        if let Some(r) = p.regs.iter().find(|r| r.ty == UNTYPED) {
            return Err(self.err(format!("%{} is never assigned in {}", r.name, p.name)));
        }
        Ok(p)
    }
}
