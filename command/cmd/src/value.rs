// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A parameter's value once the analyzer has read it: typed, special
//! values replaced, lists and groups structured as the definition says.

use std::fmt;

use crate::token::Tok;

/// A value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// Character data (`*CHAR`, the name types, `*CMD`, `*CMDSTR`,
    /// `*PNAME`, `*VARNAME`, `*LGL` as `'0'`/`'1'`, `*DATE` as `CYMD`,
    /// `*TIME` as `HMS`).
    Char(String),
    /// Hexadecimal data (`X'..'`).
    Hex(Vec<u8>),
    /// A number (`*DEC`, `*INT2`, `*INT4`, `*UINT2`, `*UINT4`, `*X`).
    Num(qdecimal::Decimal),
    /// A CL variable, when analysis runs for a compiler.
    Var(String),
    /// An expression the compiler evaluates at run time.
    Expr(Vec<Tok>),
    /// `*NULL`: no value is passed (`PASSVAL(*NULL)`, `TYPE(*NULL)`).
    Null,
    /// A qualified name: the parts in definition order (the object
    /// first, then its library).
    Qual(Vec<Value>),
    /// The elements of a group, in definition order; `None` where the
    /// element takes its default.
    Elems(Vec<Option<Value>>),
    /// A list.
    List(Vec<Value>),
}

impl Value {
    /// Character text, when the value is text.
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Char(s) => Some(s),
            _ => None,
        }
    }

    /// The value as a command string would spell it.
    #[must_use]
    pub fn source(&self) -> String {
        match self {
            Self::Char(s) => {
                if needs_quotes(s) {
                    format!("'{}'", s.replace('\'', "''"))
                } else {
                    s.clone()
                }
            }
            Self::Hex(b) => Tok::Hex(b.clone()).text(),
            Self::Num(d) => d.to_string(),
            Self::Var(v) => format!("&{v}"),
            Self::Expr(t) => Tok::join(t),
            Self::Null => "*NULL".into(),
            Self::Qual(parts) => {
                let mut v: Vec<String> = parts
                    .iter()
                    .filter(|p| !matches!(p, Self::Char(s) if s.is_empty()))
                    .map(Value::source)
                    .collect();
                v.reverse();
                v.join("/")
            }
            Self::Elems(items) => {
                let v: Vec<String> = items
                    .iter()
                    .map(|i| i.as_ref().map_or_else(|| "*N".to_string(), Value::source))
                    .collect();
                format!("({})", v.join(" "))
            }
            Self::List(items) => {
                let v: Vec<String> = items.iter().map(Value::source).collect();
                v.join(" ")
            }
        }
    }

    /// The value as it is written inside `KWD(...)`: a group's elements
    /// without the parentheses a nested group needs.
    #[must_use]
    pub fn parm_source(&self) -> String {
        match self {
            Self::Elems(items) => items
                .iter()
                .map(|i| i.as_ref().map_or_else(|| "*N".to_string(), Value::source))
                .collect::<Vec<_>>()
                .join(" "),
            other => other.source(),
        }
    }

    /// The elements of a list, or the value itself as a one-item list.
    #[must_use]
    pub fn items(&self) -> Vec<&Value> {
        match self {
            Self::List(v) => v.iter().collect(),
            other => vec![other],
        }
    }
}

/// Whether text needs quoting in a command string: anything but an
/// upper-case word of name characters, `*SPECIAL` values and numbers.
#[must_use]
pub fn needs_quotes(s: &str) -> bool {
    s.is_empty()
        || s.chars().any(|c| {
            !(c.is_ascii_uppercase()
                || c.is_ascii_digit()
                || matches!(c, '*' | '_' | '$' | '#' | '@' | '.' | '/' | '-' | '+'))
        })
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.source())
    }
}
