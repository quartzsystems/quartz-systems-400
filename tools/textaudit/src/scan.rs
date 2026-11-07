// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The string literals of one Rust source, by token: attributes, test
//! items, logging and assertion macros and `.expect(...)` messages are
//! skipped; every other string literal is a candidate.

use std::str::FromStr;

use proc_macro2::{Delimiter, TokenStream, TokenTree};

/// Macros whose text never reaches a display: logs, panics, asserts.
const SKIP_MACROS: &[&str] = &[
    "trace",
    "debug",
    "info",
    "warn",
    "error",
    "panic",
    "unreachable",
    "todo",
    "unimplemented",
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
    "eprintln",
    "eprint",
    "include_str",
    "include_bytes",
    "env",
    "concat",
    "matches",
    "cfg",
];

/// Methods whose argument is a programmer's message, not the user's.
const SKIP_METHODS: &[&str] = &["expect", "expect_err", "context", "with_context"];

/// One string literal of a source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Literal {
    /// Its line, from 1.
    pub line: usize,
    /// Its value.
    pub text: String,
    /// The calls it is an argument of, innermost last.
    pub calls: Vec<String>,
}

/// The string literals of `src`; `None` when it does not tokenize.
#[must_use]
pub fn literals(src: &str) -> Option<Vec<Literal>> {
    let ts = TokenStream::from_str(src).ok()?;
    let mut out = Vec::new();
    walk(ts, &mut Vec::new(), &mut out);
    Some(out)
}

/// Whether an attribute's tokens mark a test item.
fn is_test_attr(g: &proc_macro2::Group) -> bool {
    let s: String = g
        .stream()
        .to_string()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    s == "test" || s.starts_with("cfg(test") || s.contains("cfg(all(test") || s.ends_with("::test")
}

fn walk(ts: TokenStream, calls: &mut Vec<String>, out: &mut Vec<Literal>) {
    let toks: Vec<TokenTree> = ts.into_iter().collect();
    let mut i = 0;
    let mut skip_item = false;
    while i < toks.len() {
        if skip_item {
            match &toks[i] {
                TokenTree::Group(g) if g.delimiter() == Delimiter::Brace => skip_item = false,
                TokenTree::Punct(p) if p.as_char() == ';' => skip_item = false,
                _ => {}
            }
            i += 1;
            continue;
        }
        match &toks[i] {
            TokenTree::Punct(p) if p.as_char() == '#' => {
                // `#[...]` or `#![...]`.
                let mut j = i + 1;
                if matches!(toks.get(j), Some(TokenTree::Punct(q)) if q.as_char() == '!') {
                    j += 1;
                }
                if let Some(TokenTree::Group(g)) = toks.get(j)
                    && g.delimiter() == Delimiter::Bracket
                {
                    if is_test_attr(g) {
                        skip_item = true;
                    }
                    i = j + 1;
                    continue;
                }
                i += 1;
            }
            TokenTree::Ident(id) => {
                let name = id.to_string();
                // A macro call.
                if matches!(toks.get(i + 1), Some(TokenTree::Punct(q)) if q.as_char() == '!')
                    && let Some(TokenTree::Group(g)) = toks.get(i + 2)
                {
                    if !SKIP_MACROS.contains(&name.as_str()) {
                        calls.push(format!("{name}!"));
                        walk(g.stream(), calls, out);
                        calls.pop();
                    }
                    i += 3;
                    continue;
                }
                // A call or method call.
                if let Some(TokenTree::Group(g)) = toks.get(i + 1)
                    && g.delimiter() == Delimiter::Parenthesis
                {
                    let method =
                        i > 0 && matches!(&toks[i - 1], TokenTree::Punct(q) if q.as_char() == '.');
                    if !(method && SKIP_METHODS.contains(&name.as_str())) {
                        // `Type::f` when a path names it.
                        let path = i >= 3
                            && matches!(&toks[i - 1], TokenTree::Punct(q) if q.as_char() == ':')
                            && matches!(&toks[i - 2], TokenTree::Punct(q) if q.as_char() == ':');
                        let full = match (path, toks.get(i.wrapping_sub(3))) {
                            (true, Some(TokenTree::Ident(t))) => format!("{t}::{name}"),
                            _ => name,
                        };
                        calls.push(full);
                        walk(g.stream(), calls, out);
                        calls.pop();
                    }
                    i += 2;
                    continue;
                }
                i += 1;
            }
            TokenTree::Group(g) => {
                walk(g.stream(), calls, out);
                i += 1;
            }
            TokenTree::Literal(l) => {
                let s = l.to_string();
                if (s.starts_with('"') || s.starts_with("r\"") || s.starts_with("r#"))
                    && let Ok(v) = syn::parse_str::<syn::LitStr>(&s)
                {
                    out.push(Literal {
                        line: l.span().start().line,
                        text: v.value(),
                        calls: calls.clone(),
                    });
                }
                i += 1;
            }
            TokenTree::Punct(_) => i += 1,
        }
    }
}
