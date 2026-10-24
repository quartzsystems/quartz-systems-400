// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Declarations: the module line, types, variables, files, prototypes
//! and procedure headers.

use super::Parser;
use crate::error::Result;
use crate::inst::{Const, literal};
use crate::module::{
    ActGrp, Block, DbgView, FileDev, FileRef, FileUsage, FormatRef, Lang, Linkage, Param, Proc,
    Proto, StorageClass, TypeDef, Var, index32,
};
use crate::text::lex::Tok;
use crate::types::Type;

impl Parser {
    pub(crate) fn module(&mut self) -> Result<()> {
        self.skip_newlines();
        self.expect_word("module")?;
        self.m.name = self.ident()?;
        self.module_attrs()?;
        self.end_line()?;
        let mut bodies = Vec::new();
        loop {
            self.skip_newlines();
            if self.at_end() {
                break;
            }
            let word = self.ident()?;
            match word.as_str() {
                "type" => {
                    let name = self.ident()?;
                    if self.m.type_named(&name).is_some() {
                        return Err(self.err(format!("type {name} is declared twice")));
                    }
                    self.expect_sym("=")?;
                    let ty = self.ty()?;
                    self.m.types.push(TypeDef { name, ty });
                }
                "static" | "import" | "const" | "based" | "auto" => {
                    let v = self.var_decl(&word)?;
                    self.m.vars.push(v);
                }
                "file" => {
                    let f = self.file_decl()?;
                    self.m.files.push(f);
                }
                "extern" => {
                    let p = self.extern_decl()?;
                    self.m.protos.push(p);
                }
                "proto" => {
                    let name = self.ident()?;
                    let (params, ret, opdesc) = self.signature()?;
                    self.m.protos.push(Proto {
                        name,
                        params,
                        ret,
                        opdesc,
                        linkage: Linkage::Signature,
                    });
                }
                "proc" => {
                    bodies.push(self.proc_header()?);
                    continue;
                }
                w => return Err(self.err(format!("{w} does not start a declaration"))),
            }
            self.end_line()?;
        }
        for (index, at) in bodies {
            self.at = at;
            self.proc_body(index)?;
        }
        Ok(())
    }

    fn module_attrs(&mut self) -> Result<()> {
        while let Tok::Ident(w) = self.peek().clone() {
            self.bump();
            match w.as_str() {
                "lang" => {
                    let s = self.paren(Self::ident)?;
                    self.m.lang = Lang::from_name(&s)
                        .ok_or_else(|| self.err(format!("{s} is not a language")))?;
                }
                "ccsid" => {
                    let n = self.paren(Self::u32)?;
                    self.m.ccsid = u16::try_from(n).map_err(|_| self.err("ccsid is 1 to 65535"))?;
                }
                "actgrp" => {
                    self.m.actgrp = self.paren(|p| {
                        Ok(match p.bump() {
                            Tok::Special(s) if s == "NEW" => ActGrp::New,
                            Tok::Special(s) if s == "CALLER" => ActGrp::Caller,
                            Tok::Ident(n) => ActGrp::Named(n),
                            _ => return Err(p.err("actgrp is *NEW, *CALLER or a name")),
                        })
                    })?;
                }
                "dftactgrp" => self.m.dftactgrp = true,
                "nomain" => self.m.nomain = true,
                "srcstmt" => self.m.options.srcstmt = true,
                "nodebugio" => self.m.options.nodebugio = true,
                "dbgview" => {
                    let s = self.paren(|p| match p.bump() {
                        Tok::Special(s) => Ok(format!("*{s}")),
                        _ => Err(p.err("dbgview is *NONE, *STMT, *SOURCE, *LIST or *ALL")),
                    })?;
                    self.m.options.dbgview = DbgView::from_name(&s)
                        .ok_or_else(|| self.err(format!("{s} is not a debug view")))?;
                }
                _ => return Err(self.err(format!("{w} is not a module attribute"))),
            }
        }
        Ok(())
    }

    /// `KIND $name: T [inz(LIT) | = LIT] [on $p] [export]`.
    pub(crate) fn var_decl(&mut self, kind: &str) -> Result<Var> {
        let name = self.var_name()?;
        self.expect_sym(":")?;
        let ty = self.ty()?;
        let class = match kind {
            "static" => StorageClass::Static,
            "auto" => StorageClass::Auto,
            "import" => StorageClass::Import,
            "const" => StorageClass::Const,
            "based" => {
                self.expect_word("on")?;
                StorageClass::Based(self.var_ref()?)
            }
            _ => unreachable!(),
        };
        let mut v = Var::new(&name, ty, class);
        loop {
            if self.eat_word("inz") {
                let c = self.paren(Self::literal)?;
                v.inz = Some(self.encode(&v.ty, &c)?);
            } else if self.eat_sym("=") {
                let c = self.literal()?;
                v.inz = Some(self.encode(&v.ty, &c)?);
            } else if self.eat_word("export") {
                v.export = true;
            } else {
                break;
            }
        }
        Ok(v)
    }

    fn encode(&self, ty: &Type, c: &Const) -> Result<Vec<u8>> {
        literal::encode(ty, c).map_err(|e| self.err(e))
    }

    /// A page length or line number.
    fn form_number(&mut self) -> Result<u16> {
        match self.bump() {
            Tok::Int(n) => u16::try_from(n).map_err(|_| self.err("a line number")),
            _ => Err(self.err("a line number")),
        }
    }

    /// `file NAME file(OBJ) [lib(L)] [usage(*X)] [keyed] [nolvlchk] [formlen(N)] [formofl(N)] [fmt(NAME[:LEVEL])]...`.
    fn file_decl(&mut self) -> Result<FileRef> {
        let name = self.ident()?;
        let mut f = FileRef {
            name: name.clone(),
            file: name,
            lib: "*LIBL".into(),
            usage: FileUsage::Input,
            keyed: false,
            lvlchk: true,
            dev: FileDev::Db,
            formats: Vec::new(),
            form: (0, 0),
        };
        while let Tok::Ident(w) = self.peek().clone() {
            self.bump();
            match w.as_str() {
                "file" => f.file = self.paren(Self::ident)?,
                "lib" => {
                    f.lib = self.paren(|p| match p.bump() {
                        Tok::Ident(n) => Ok(n),
                        Tok::Special(s) => Ok(format!("*{s}")),
                        _ => Err(p.err("a library name")),
                    })?;
                }
                "usage" => {
                    let s = self.paren(|p| match p.bump() {
                        Tok::Special(s) => Ok(format!("*{s}")),
                        _ => Err(p.err("usage is *INPUT, *OUTPUT, *UPDATE or *ALL")),
                    })?;
                    f.usage = FileUsage::from_name(&s)
                        .ok_or_else(|| self.err(format!("{s} is not a usage")))?;
                }
                "keyed" => f.keyed = true,
                "formlen" => f.form.0 = self.paren(Self::form_number)?,
                "formofl" => f.form.1 = self.paren(Self::form_number)?,
                "nolvlchk" => f.lvlchk = false,
                "dev" => {
                    let s = self.paren(|p| match p.bump() {
                        Tok::Special(s) => Ok(format!("*{s}")),
                        _ => Err(p.err("dev is *DB, *DSP or *PRT")),
                    })?;
                    f.dev = FileDev::from_name(&s)
                        .ok_or_else(|| self.err(format!("{s} is not a device")))?;
                }
                "fmt" => {
                    let r = self.paren(|p| {
                        let name = p.ident()?;
                        let level = if p.eat_sym(":") {
                            Some(p.string()?)
                        } else {
                            None
                        };
                        Ok(FormatRef { name, level })
                    })?;
                    f.formats.push(r);
                }
                _ => return Err(self.err(format!("{w} is not a file attribute"))),
            }
        }
        Ok(f)
    }

    /// `extern proc|pgm|sysapi NAME(params) [-> T] [opdesc] [pgm(N)] [lib(L)]`.
    fn extern_decl(&mut self) -> Result<Proto> {
        let kind = self.ident()?;
        let name = self.ident()?;
        let (params, ret, opdesc) = self.signature()?;
        let linkage = match kind.as_str() {
            "proc" => Linkage::Import,
            "sysapi" => Linkage::SysApi,
            "pgm" => {
                let mut pgm = name.clone();
                let mut lib = "*LIBL".to_string();
                while let Tok::Ident(w) = self.peek().clone() {
                    self.bump();
                    match w.as_str() {
                        "pgm" => pgm = self.paren(Self::ident)?,
                        "lib" => {
                            lib = self.paren(|p| match p.bump() {
                                Tok::Ident(n) => Ok(n),
                                Tok::Special(s) => Ok(format!("*{s}")),
                                _ => Err(p.err("a library name")),
                            })?;
                        }
                        _ => return Err(self.err(format!("{w} is not a program attribute"))),
                    }
                }
                Linkage::Pgm { name: pgm, lib }
            }
            _ => return Err(self.err("extern proc, extern pgm or extern sysapi")),
        };
        Ok(Proto {
            name,
            params,
            ret,
            opdesc,
            linkage,
        })
    }

    /// `(params) [-> T] [opdesc]`.
    fn signature(&mut self) -> Result<(Vec<Param>, Option<Type>, bool)> {
        let params = self.params()?;
        let ret = if self.eat_sym("->") {
            Some(self.ty()?)
        } else {
            None
        };
        let opdesc = self.eat_word("opdesc");
        Ok((params, ret, opdesc))
    }

    fn params(&mut self) -> Result<Vec<Param>> {
        self.expect_sym("(")?;
        let mut out = Vec::new();
        loop {
            self.skip_newlines();
            if self.eat_sym(")") {
                break;
            }
            let mut by_value = false;
            let mut const_ = false;
            loop {
                if self.eat_word("byval") {
                    by_value = true;
                } else if self.eat_word("const") {
                    const_ = true;
                } else {
                    break;
                }
            }
            let name = self.var_name()?;
            self.expect_sym(":")?;
            let ty = self.ty()?;
            let mut p = Param::by_ref(&name, ty);
            p.by_value = by_value;
            p.const_ = const_;
            while let Tok::Special(s) = self.peek().clone() {
                self.bump();
                match s.as_str() {
                    "NOPASS" => p.nopass = true,
                    "OMIT" => p.omit = true,
                    "VARSIZE" => p.varsize = true,
                    _ => return Err(self.err(format!("*{s} is not a parameter option"))),
                }
            }
            out.push(p);
            self.skip_newlines();
            if !self.eat_sym(",") {
                self.skip_newlines();
                self.expect_sym(")")?;
                break;
            }
        }
        Ok(out)
    }

    /// `proc NAME(params) [-> T] [export] [entry] [opdesc] {`: the header,
    /// pushed to the module so calls can resolve before the body is read.
    /// Returns the procedure's index and the token where its body starts.
    fn proc_header(&mut self) -> Result<(usize, usize)> {
        let name = self.ident()?;
        if self.m.proc_named(&name).is_some() || self.m.proto_named(&name).is_some() {
            return Err(self.err(format!("{name} is declared twice")));
        }
        let (params, ret, opdesc) = self.signature()?;
        let mut p = Proc::new(&name);
        p.ret = ret;
        p.opdesc = opdesc;
        for (i, param) in params.iter().enumerate() {
            p.vars.push(Var::new(
                &param.name,
                param.ty.clone(),
                StorageClass::Param(index32(i)),
            ));
        }
        p.params = params;
        loop {
            if self.eat_word("export") {
                p.export = true;
            } else if self.eat_word("entry") {
                p.entry = true;
            } else if self.eat_word("opdesc") {
                p.opdesc = true;
            } else {
                break;
            }
        }
        self.expect_sym("{")?;
        self.end_line()?;
        let start = self.at;
        // Skip the body to its closing brace.
        let mut depth = 1;
        while depth > 0 {
            match self.peek() {
                Tok::Sym("{") => depth += 1,
                Tok::Sym("}") => depth -= 1,
                Tok::End => return Err(self.err(format!("procedure {name} is not closed"))),
                _ => {}
            }
            self.bump();
        }
        self.end_line()?;
        self.m.procs.push(p);
        Ok((self.m.procs.len() - 1, start))
    }

    /// The declarations and blocks of a procedure whose header is in
    /// the module.
    fn proc_body(&mut self, index: usize) -> Result<()> {
        let p = self.m.procs[index].clone();
        self.begin_proc(p);
        loop {
            self.skip_newlines();
            match self.peek().clone() {
                Tok::Ident(w) if matches!(w.as_str(), "auto" | "static" | "based" | "const") => {
                    self.bump();
                    let v = self.var_decl(&w)?;
                    if self.cur_proc().vars.iter().any(|x| x.name == v.name) {
                        return Err(self.err(format!("${} is declared twice", v.name)));
                    }
                    self.cur_proc_mut().vars.push(v);
                    self.end_line()?;
                }
                _ => break,
            }
        }
        loop {
            self.skip_newlines();
            if self.eat_sym("}") {
                break;
            }
            let label = self.ident()?;
            self.expect_sym(":")?;
            self.end_line()?;
            if self.cur_proc().blocks.iter().any(|b| b.label == label) {
                return Err(self.err(format!("label {label} is declared twice")));
            }
            self.label(&label);
            let mut insts = Vec::new();
            loop {
                self.skip_newlines();
                let next_is_label =
                    matches!(self.peek(), Tok::Ident(_)) && self.peek_at(1) == &Tok::Sym(":");
                if self.is_sym("}") || next_is_label {
                    break;
                }
                insts.push(self.inst()?);
                self.end_line()?;
            }
            self.cur_proc_mut().blocks.push(Block { label, insts });
        }
        let p = self.end_proc()?;
        self.m.procs[index] = p;
        Ok(())
    }
}
