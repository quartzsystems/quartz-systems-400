// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Word expansion: tilde, parameters, command substitution, arithmetic,
//! field splitting ([`split`]), pathname expansion and quote removal.
//!
//! Expansion builds fields of characters tagged quoted (`Q`), produced
//! by an unquoted expansion (`S`, subject to splitting) or a zero-width
//! marker (`K`) left by quotes so that `""` makes an empty field.

mod param;
mod split;

use crate::msg::Msg;
use crate::pattern::{self, PChar, path::Dirs};
use crate::shell::options::Opt;
use crate::shell::{Flow, Shell};
use crate::syntax::{List, ParamName, ParamOp, Word, WordPart};
use crate::sys::{Buffer, FileType, Sys};

pub(crate) const Q: u8 = 1;
pub(crate) const S: u8 = 2;
pub(crate) const K: u8 = 4;

/// A field under construction.
pub(crate) type Field = Vec<(char, u8)>;

/// Fields being built.
pub(crate) struct Builder {
    pub(crate) fields: Vec<Field>,
}

impl Builder {
    fn new() -> Self {
        Self {
            fields: vec![Vec::new()],
        }
    }

    pub(crate) fn push(&mut self, s: &str, flags: u8) {
        let cur = self.fields.last_mut().expect("field");
        cur.extend(s.chars().map(|c| (c, flags)));
    }

    pub(crate) fn keep(&mut self) {
        self.fields.last_mut().expect("field").push(('\0', K));
    }

    pub(crate) fn brk(&mut self) {
        self.fields.push(Vec::new());
    }
}

/// Where tilde expansion applies.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tilde {
    None,
    Start,
    Assign,
}

/// How literal text inside an expansion's word behaves.
#[derive(Clone, Copy)]
pub(crate) struct Ctx {
    /// Inside double quotes.
    pub(crate) dq: bool,
    /// Unquoted literal text is subject to splitting (the word of an
    /// unquoted `${x:-word}`).
    pub(crate) lit_split: bool,
}

fn text_of(f: &Field) -> String {
    f.iter()
        .filter(|(_, fl)| fl & K == 0)
        .map(|(c, _)| *c)
        .collect()
}

/// The file system as pathname expansion sees it.
struct SysDirs<'a>(&'a dyn Sys);

impl Dirs for SysDirs<'_> {
    fn names(&self, dir: &str) -> Option<Vec<String>> {
        self.0.read_dir(dir).ok()
    }

    fn exists(&self, path: &str) -> bool {
        self.0.lstat(path).is_ok()
    }

    fn is_dir(&self, path: &str) -> bool {
        matches!(self.0.stat(path), Ok(st) if st.ftype == FileType::Dir)
    }

    fn case_insensitive(&self, dir: &str) -> bool {
        self.0.case_insensitive(dir)
    }
}

impl Shell {
    /// Expand words into fields: splitting and pathname expansion.
    pub(crate) fn expand_words(&mut self, words: &[Word]) -> Result<Vec<String>, Flow> {
        let mut out = Vec::new();
        for w in words {
            let b = self.build(w, Tilde::Start)?;
            let ifs = self.var("IFS");
            for f in b.fields {
                for field in split::split(&f, ifs.as_deref()) {
                    self.glob_into(&field, &mut out);
                }
            }
        }
        Ok(out)
    }

    fn glob_into(&self, field: &Field, out: &mut Vec<String>) {
        if !self.opts.get(Opt::NoGlob) {
            let pat: Vec<PChar> = field
                .iter()
                .filter(|(_, fl)| fl & K == 0)
                .map(|(c, fl)| PChar {
                    c: *c,
                    quoted: fl & Q != 0,
                })
                .collect();
            if pattern::has_glob(&pat) {
                let found = pattern::path::expand(&pat, &self.cwd, &SysDirs(&*self.sys));
                if !found.is_empty() {
                    out.extend(found);
                    return;
                }
            }
        }
        out.push(text_of(field));
    }

    /// Expand a word to one string: no splitting, no pathname expansion.
    pub(crate) fn expand_string(&mut self, w: &Word) -> Result<String, Flow> {
        let b = self.build(w, Tilde::Start)?;
        Ok(b.fields.iter().map(text_of).collect::<Vec<_>>().join(" "))
    }

    /// Expand an assignment's value (tildes after `:` too).
    pub(crate) fn expand_assign_value(&mut self, w: &Word) -> Result<String, Flow> {
        let b = self.build(w, Tilde::Assign)?;
        Ok(b.fields.iter().map(text_of).collect::<Vec<_>>().join(" "))
    }

    /// Expand a here-document body.
    pub(crate) fn expand_here(&mut self, w: &Word) -> Result<String, Flow> {
        let b = self.build(w, Tilde::None)?;
        Ok(b.fields.iter().map(text_of).collect::<Vec<_>>().join(" "))
    }

    /// Expand a pattern word, keeping which characters were quoted.
    pub(crate) fn expand_pattern(&mut self, w: &Word) -> Result<Vec<PChar>, Flow> {
        let b = self.build(w, Tilde::Start)?;
        let mut out = Vec::new();
        for (i, f) in b.fields.iter().enumerate() {
            if i > 0 {
                out.push(PChar {
                    c: ' ',
                    quoted: true,
                });
            }
            out.extend(f.iter().filter(|(_, fl)| fl & K == 0).map(|(c, fl)| PChar {
                c: *c,
                quoted: fl & Q != 0,
            }));
        }
        Ok(out)
    }

    /// Expand a prompt string (`PS1`, `PS2`, `PS4`): parameters, command
    /// substitution and arithmetic; the text as is when it does not parse.
    pub fn expand_prompt(&mut self, s: &str) -> String {
        let Ok(w) = crate::syntax::parse_here_text(s) else {
            return s.to_string();
        };
        let saved = self.status;
        let r = self.expand_here(&w).unwrap_or_else(|_| s.to_string());
        self.status = saved;
        r
    }

    /// A leading tilde prefix: expand it into the builder and return
    /// the parts left.
    pub(crate) fn tilde_prefix<'p>(
        &mut self,
        parts: &'p [WordPart],
        assign: bool,
        ctx: Ctx,
        b: &mut Builder,
    ) -> &'p [WordPart] {
        let Some(WordPart::Lit(first)) = parts.first() else {
            return parts;
        };
        if !first.starts_with('~') {
            return parts;
        }
        let end = first
            .find('/')
            .or_else(|| if assign { first.find(':') } else { None });
        if end.is_none() && parts.len() != 1 {
            return parts;
        }
        let end = end.unwrap_or(first.len());
        let Some(home) = self.tilde(&first[1..end]) else {
            return parts;
        };
        b.push(&home, Q);
        let mode = if assign { Tilde::Assign } else { Tilde::Start };
        self.push_lit(b, &first[end..], mode, ctx);
        &parts[1..]
    }

    fn build(&mut self, w: &Word, tilde: Tilde) -> Result<Builder, Flow> {
        let mut b = Builder::new();
        let ctx = Ctx {
            dq: false,
            lit_split: false,
        };
        let parts: &[WordPart] = if tilde == Tilde::None {
            &w.parts
        } else {
            self.tilde_prefix(&w.parts, tilde == Tilde::Assign, ctx, &mut b)
        };
        for p in parts {
            if tilde == Tilde::Assign
                && let WordPart::Lit(s) = p
            {
                self.push_lit(
                    &mut b,
                    s,
                    tilde,
                    Ctx {
                        dq: false,
                        lit_split: false,
                    },
                );
                continue;
            }
            self.part(
                p,
                Ctx {
                    dq: false,
                    lit_split: false,
                },
                &mut b,
            )?;
        }
        Ok(b)
    }

    /// Literal text; in assignments, a `~` after `:` expands.
    fn push_lit(&mut self, b: &mut Builder, s: &str, tilde: Tilde, ctx: Ctx) {
        let flags = if ctx.dq {
            Q
        } else if ctx.lit_split {
            S
        } else {
            0
        };
        if tilde != Tilde::Assign || !s.contains(":~") {
            b.push(s, flags);
            return;
        }
        let mut first = true;
        for seg in s.split(':') {
            if !first {
                b.push(":", flags);
            }
            first = false;
            if let Some(rest) = seg.strip_prefix('~') {
                let (user, tail) = rest.split_once('/').map_or((rest, ""), |(u, t)| (u, t));
                if let Some(home) = self.tilde(user) {
                    b.push(&home, Q);
                    if rest.contains('/') {
                        b.push("/", flags);
                        b.push(tail, flags);
                    }
                    continue;
                }
            }
            b.push(seg, flags);
        }
    }

    fn tilde(&mut self, user: &str) -> Option<String> {
        if user.is_empty() {
            return self.var("HOME");
        }
        self.sys.home_of(user)
    }

    /// Expand one part into the builder.
    pub(crate) fn part(&mut self, p: &WordPart, ctx: Ctx, b: &mut Builder) -> Result<(), Flow> {
        let exp_flags = if ctx.dq { Q } else { S };
        match p {
            WordPart::Lit(s) => self.push_lit(b, s, Tilde::None, ctx),
            WordPart::Quoted(s) => {
                b.keep();
                b.push(s, Q);
            }
            WordPart::Double(inner) => {
                let only_at = !inner.is_empty()
                    && inner.iter().all(|p| {
                        matches!(p, WordPart::Param(pp) if pp.name == ParamName::Special('@') && matches!(pp.op, ParamOp::Plain))
                    });
                if !only_at {
                    b.keep();
                }
                for q in inner {
                    self.part(
                        q,
                        Ctx {
                            dq: true,
                            lit_split: false,
                        },
                        b,
                    )?;
                }
            }
            WordPart::Param(pp) => self.param(pp, ctx, b)?,
            WordPart::Command(list) => {
                let s = self.command_subst(list);
                b.push(&s, exp_flags);
            }
            WordPart::Arith(w) => {
                let n = self.arith(w)?;
                b.push(&n.to_string(), exp_flags);
            }
        }
        Ok(())
    }

    /// Run a command substitution and return its output without
    /// trailing newlines.
    pub(crate) fn command_subst(&mut self, list: &List) -> String {
        let buf = Buffer::new();
        let mut sub = self.subshell();
        sub.fds.set(1, buf.fd());
        let r = sub.run_list(list);
        let st = sub.finish_child(r);
        drop(sub);
        self.cmdsub_status = Some(st);
        self.status = st;
        let mut s = String::from_utf8_lossy(&buf.take()).into_owned();
        while s.ends_with('\n') {
            s.pop();
        }
        s
    }

    /// Evaluate an arithmetic expansion.
    pub(crate) fn arith(&mut self, w: &Word) -> Result<i64, Flow> {
        let text = self.expand_here(w)?;
        match crate::arith::eval(&text, &mut crate::shell::exec::ArithVars(self)) {
            Ok(n) => Ok(n),
            Err(e) => {
                self.report(&Msg::Arith(e.to_string()));
                Err(Flow::Error(2))
            }
        }
    }
}
