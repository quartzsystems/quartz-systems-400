// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The parser: tokens to [`Def`]s. Anonymous `struct`, `enum` and
//! `union` bodies inside a declaration, and `float`, `double` and
//! `quadruple`, are refused: no protocol here needs them.

use crate::error::{At, Pos, Result};
use crate::lex::{Tok, Token, tokens};
use crate::model::{
    Case, Decl, DeclKind, Def, Proc, Program, TypeBody, TypeDef, TypeSpec, Union, Value, Version,
};

/// The definitions of `src`.
pub fn parse(src: &str) -> Result<Vec<Def>> {
    let mut p = Parser {
        toks: tokens(src)?,
        at: 0,
    };
    let mut defs = Vec::new();
    while p.peek() != &Tok::Eof {
        defs.push(p.def()?);
    }
    Ok(defs)
}

/// Words that cannot name anything.
const RESERVED: &[&str] = &[
    "bool",
    "case",
    "const",
    "default",
    "double",
    "quadruple",
    "enum",
    "float",
    "hyper",
    "int",
    "opaque",
    "string",
    "struct",
    "switch",
    "typedef",
    "union",
    "unsigned",
    "void",
    "program",
    "version",
];

struct Parser {
    toks: Vec<Token>,
    at: usize,
}

impl Parser {
    fn token(&self) -> &Token {
        let last = self.toks.len().saturating_sub(1);
        &self.toks[self.at.min(last)]
    }

    fn peek(&self) -> &Tok {
        &self.token().tok
    }

    fn pos(&self) -> Pos {
        self.token().pos
    }

    fn next(&mut self) -> Token {
        let t = self.token().clone();
        if t.tok != Tok::Eof {
            self.at += 1;
        }
        t
    }

    fn fail<T>(&self, what: &str) -> Result<T> {
        let found = match self.peek() {
            Tok::Ident(s) => format!("`{s}`"),
            Tok::Num(n) => format!("`{n}`"),
            Tok::Sym(c) => format!("`{c}`"),
            Tok::Eof => "the end".to_string(),
        };
        Err(At::new(
            self.pos(),
            format!("expected {what}, found {found}"),
        ))
    }

    fn is_word(&self, w: &str) -> bool {
        matches!(self.peek(), Tok::Ident(s) if s == w)
    }

    fn eat_word(&mut self, w: &str) -> bool {
        let yes = self.is_word(w);
        if yes {
            self.next();
        }
        yes
    }

    fn word(&mut self, w: &str) -> Result<()> {
        if self.eat_word(w) {
            Ok(())
        } else {
            self.fail(&format!("`{w}`"))
        }
    }

    fn is_sym(&self, c: char) -> bool {
        self.peek() == &Tok::Sym(c)
    }

    fn eat_sym(&mut self, c: char) -> bool {
        let yes = self.is_sym(c);
        if yes {
            self.next();
        }
        yes
    }

    fn sym(&mut self, c: char) -> Result<()> {
        if self.eat_sym(c) {
            Ok(())
        } else {
            self.fail(&format!("`{c}`"))
        }
    }

    fn ident(&mut self) -> Result<(String, Pos)> {
        match self.peek().clone() {
            Tok::Ident(s) if !RESERVED.contains(&s.as_str()) => {
                let pos = self.next().pos;
                Ok((s, pos))
            }
            _ => self.fail("a name"),
        }
    }

    fn value(&mut self) -> Result<Value> {
        match self.peek().clone() {
            Tok::Num(n) => {
                self.next();
                Ok(Value::Num(n))
            }
            Tok::Ident(_) => {
                let (s, pos) = self.ident()?;
                Ok(Value::Name(s, pos))
            }
            _ => self.fail("a constant"),
        }
    }

    fn def(&mut self) -> Result<Def> {
        let pos = self.pos();
        if self.eat_word("const") {
            let (name, pos) = self.ident()?;
            self.sym('=')?;
            let v = self.value()?;
            self.sym(';')?;
            return Ok(Def::Const(name, v, pos));
        }
        if self.eat_word("program") {
            return self.program().map(Def::Program);
        }
        let body = if self.eat_word("typedef") {
            let d = self.decl()?;
            self.sym(';')?;
            let Some(name) = d.name.clone() else {
                return Err(At::new(pos, "a typedef of void"));
            };
            return Ok(Def::Type(TypeDef {
                name,
                pos: d.pos,
                body: TypeBody::Typedef(d),
            }));
        } else if self.is_word("enum") || self.is_word("struct") || self.is_word("union") {
            self.peek().clone()
        } else {
            return self.fail("a definition");
        };
        let Tok::Ident(kw) = body else { unreachable!() };
        self.next();
        let (name, pos) = self.ident()?;
        let body = match kw.as_str() {
            "enum" => TypeBody::Enum(self.enum_body()?),
            "struct" => TypeBody::Struct(self.struct_body()?),
            _ => TypeBody::Union(self.union_body()?),
        };
        self.sym(';')?;
        Ok(Def::Type(TypeDef { name, pos, body }))
    }

    fn enum_body(&mut self) -> Result<Vec<(String, Value, Pos)>> {
        self.sym('{')?;
        let mut out = Vec::new();
        loop {
            let (name, pos) = self.ident()?;
            self.sym('=')?;
            out.push((name, self.value()?, pos));
            if !self.eat_sym(',') {
                break;
            }
        }
        self.sym('}')?;
        Ok(out)
    }

    fn struct_body(&mut self) -> Result<Vec<Decl>> {
        self.sym('{')?;
        let mut out = Vec::new();
        while !self.eat_sym('}') {
            let d = self.decl()?;
            if d.name.is_none() {
                return Err(At::new(d.pos, "void in a struct"));
            }
            self.sym(';')?;
            out.push(d);
        }
        if out.is_empty() {
            return Err(At::new(self.pos(), "an empty struct"));
        }
        Ok(out)
    }

    fn union_body(&mut self) -> Result<Union> {
        self.word("switch")?;
        self.sym('(')?;
        let disc = self.decl()?;
        self.sym(')')?;
        self.sym('{')?;
        let mut cases = Vec::new();
        let mut default = None;
        loop {
            if self.is_word("case") {
                let mut labels = Vec::new();
                while self.eat_word("case") {
                    let pos = self.pos();
                    labels.push((self.value()?, pos));
                    self.sym(':')?;
                }
                let decl = self.decl()?;
                self.sym(';')?;
                cases.push(Case { labels, decl });
            } else if self.eat_word("default") {
                self.sym(':')?;
                default = Some(self.decl()?);
                self.sym(';')?;
                self.sym('}')?;
                break;
            } else if self.eat_sym('}') {
                break;
            } else {
                return self.fail("`case`, `default` or `}`");
            }
        }
        if cases.is_empty() {
            return Err(At::new(disc.pos, "a union without a case"));
        }
        Ok(Union {
            disc,
            cases,
            default,
        })
    }

    /// A type specifier other than `void`, `opaque` and `string`.
    fn type_spec(&mut self) -> Result<TypeSpec> {
        let pos = self.pos();
        if self.eat_word("unsigned") {
            if self.eat_word("int") {
                return Ok(TypeSpec::UInt);
            }
            if self.eat_word("hyper") {
                return Ok(TypeSpec::UHyper);
            }
            return Ok(TypeSpec::UInt);
        }
        for (w, t) in [
            ("int", TypeSpec::Int),
            ("hyper", TypeSpec::Hyper),
            ("bool", TypeSpec::Bool),
        ] {
            if self.eat_word(w) {
                return Ok(t);
            }
        }
        for w in ["float", "double", "quadruple"] {
            if self.is_word(w) {
                return Err(At::new(pos, format!("`{w}` is not supported")));
            }
        }
        for w in ["struct", "enum", "union"] {
            if self.eat_word(w) {
                if self.is_sym('{') || self.is_word("switch") {
                    return Err(At::new(
                        pos,
                        format!("an anonymous {w}: name it with its own definition"),
                    ));
                }
                let (name, pos) = self.ident()?;
                return Ok(TypeSpec::Named(name, pos));
            }
        }
        let (name, pos) = self.ident()?;
        Ok(TypeSpec::Named(name, pos))
    }

    /// `<n>` or `<>`, the `<` already seen.
    fn bound(&mut self) -> Result<Option<Value>> {
        if self.eat_sym('>') {
            return Ok(None);
        }
        let v = self.value()?;
        self.sym('>')?;
        Ok(Some(v))
    }

    fn decl(&mut self) -> Result<Decl> {
        let pos = self.pos();
        if self.eat_word("void") {
            return Ok(Decl {
                name: None,
                kind: DeclKind::Void,
                pos,
            });
        }
        if self.eat_word("opaque") {
            let (name, pos) = self.ident()?;
            let kind = if self.eat_sym('[') {
                let v = self.value()?;
                self.sym(']')?;
                DeclKind::FixedOpaque(v)
            } else if self.eat_sym('<') {
                DeclKind::VarOpaque(self.bound()?)
            } else {
                return self.fail("`[` or `<` after an opaque");
            };
            return Ok(Decl {
                name: Some(name),
                kind,
                pos,
            });
        }
        if self.eat_word("string") {
            let (name, pos) = self.ident()?;
            if !self.eat_sym('<') {
                return self.fail("`<` after a string");
            }
            return Ok(Decl {
                name: Some(name),
                kind: DeclKind::String(self.bound()?),
                pos,
            });
        }
        let ty = self.type_spec()?;
        if self.eat_sym('*') {
            let (name, pos) = self.ident()?;
            return Ok(Decl {
                name: Some(name),
                kind: DeclKind::Optional(ty),
                pos,
            });
        }
        let (name, pos) = self.ident()?;
        let kind = if self.eat_sym('[') {
            let v = self.value()?;
            self.sym(']')?;
            DeclKind::FixedArray(ty, v)
        } else if self.eat_sym('<') {
            DeclKind::VarArray(ty, self.bound()?)
        } else {
            DeclKind::Plain(ty)
        };
        Ok(Decl {
            name: Some(name),
            kind,
            pos,
        })
    }

    fn program(&mut self) -> Result<Program> {
        let (name, pos) = self.ident()?;
        self.sym('{')?;
        let mut versions = Vec::new();
        while !self.eat_sym('}') {
            versions.push(self.version()?);
        }
        if versions.is_empty() {
            return Err(At::new(pos, "a program without a version"));
        }
        self.sym('=')?;
        let number = self.value()?;
        self.sym(';')?;
        Ok(Program {
            name,
            pos,
            versions,
            number,
        })
    }

    fn version(&mut self) -> Result<Version> {
        self.word("version")?;
        let (name, pos) = self.ident()?;
        self.sym('{')?;
        let mut procs = Vec::new();
        while !self.eat_sym('}') {
            procs.push(self.procedure()?);
        }
        if procs.is_empty() {
            return Err(At::new(pos, "a version without a procedure"));
        }
        self.sym('=')?;
        let number = self.value()?;
        self.sym(';')?;
        Ok(Version {
            name,
            pos,
            procs,
            number,
        })
    }

    fn proc_type(&mut self) -> Result<Option<TypeSpec>> {
        if self.eat_word("void") {
            return Ok(None);
        }
        if self.is_word("opaque") || self.is_word("string") {
            return Err(At::new(
                self.pos(),
                "a procedure's type must be named: use a typedef",
            ));
        }
        self.type_spec().map(Some)
    }

    fn procedure(&mut self) -> Result<Proc> {
        let result = self.proc_type()?;
        let (name, pos) = self.ident()?;
        self.sym('(')?;
        let arg = self.proc_type()?;
        if self.is_sym(',') {
            return Err(At::new(self.pos(), "more than one argument: use a struct"));
        }
        self.sym(')')?;
        self.sym('=')?;
        let number = self.value()?;
        self.sym(';')?;
        Ok(Proc {
            name,
            pos,
            arg,
            result,
            number,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_form() {
        let defs = parse(
            "const N = 4;
             typedef opaque fh<N>;
             typedef struct node *list;
             enum color { RED = 0, BLUE = N };
             struct node { unsigned x; hyper y[2]; string s<>; list next; int z<N>; opaque o[8]; };
             union u switch (color c) { case RED: case BLUE: int v; default: void; };
             program P { version V { u PROC(fh) = 1; void NUL(void) = 0; } = 1; } = 100;",
        )
        .unwrap();
        assert_eq!(defs.len(), 7);
        let Def::Type(TypeDef {
            body: TypeBody::Struct(f),
            ..
        }) = &defs[4]
        else {
            panic!()
        };
        assert_eq!(f[0].kind, DeclKind::Plain(TypeSpec::UInt));
        assert!(matches!(
            f[1].kind,
            DeclKind::FixedArray(TypeSpec::Hyper, Value::Num(2))
        ));
        assert_eq!(f[2].kind, DeclKind::String(None));
        let Def::Type(TypeDef {
            body: TypeBody::Union(u),
            ..
        }) = &defs[5]
        else {
            panic!()
        };
        assert_eq!(u.cases[0].labels.len(), 2);
        assert_eq!(u.default.as_ref().unwrap().kind, DeclKind::Void);
    }

    fn err(s: &str) -> String {
        let e = parse(s).unwrap_err();
        format!("{}: {}", e.pos, e.msg)
    }

    #[test]
    fn errors_have_positions() {
        assert_eq!(err("struct s { int a }"), "1:18: expected `;`, found `}`");
        assert_eq!(err("typedef float f;"), "1:9: `float` is not supported");
        assert!(err("struct s { struct { int a; } b; };").contains("anonymous struct"));
        assert!(err("struct s { };").contains("empty struct"));
        assert!(err("union u switch (int d) { default: void; };").contains("without a case"));
        assert!(
            err("program P { version V { void X(int, int) = 1; } = 1; } = 1;")
                .contains("more than one")
        );
        assert!(err("const int = 3;").contains("expected a name"));
        assert!(err("typedef void;").contains("void"));
        assert!(err("bogus").contains("a definition"));
        assert!(err("typedef opaque x;").contains("`[` or `<`"));
    }
}
