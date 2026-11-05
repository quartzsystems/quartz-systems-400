// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A routine's arguments bound to its parameters: by position or by
//! name, the oracle's defaults for the rest.

use std::collections::HashMap;

use super::super::super::SqlContext;
use super::shape::Shape;
use crate::error::{Result, SqlError};
use crate::value::Value;

/// The bound arguments by parameter name.
#[derive(Clone, Debug, Default)]
pub struct Args {
    values: HashMap<String, Value>,
}

impl Args {
    /// Bind `given` to `shape`'s parameters. Too many arguments, a name
    /// it has no parameter for, or a parameter given twice is SQL0440,
    /// as IBM reports a call that matches no routine.
    pub fn bind(shape: &Shape, given: Vec<(Option<String>, Value)>, ctx: &SqlContext) -> Result<Self> {
        let parms: Vec<_> = shape.parms.iter().filter(|p| p.mode != "OUT").collect();
        let no_match = |why: &str| SqlError::Coded {
            code: -440,
            state: "42884",
            what: format!("{}.{}: {why}", shape.schema, shape.name),
        };
        let mut values = HashMap::new();
        let mut named = false;
        for (i, (name, v)) in given.into_iter().enumerate() {
            let p = match name {
                Some(n) => {
                    named = true;
                    parms.iter().find(|p| p.name == n.to_ascii_uppercase()).ok_or_else(|| no_match(&format!("no parameter {n}")))?
                }
                None if named => return Err(no_match("a positional argument after a named one")),
                None => parms.get(i).ok_or_else(|| no_match("too many arguments"))?,
            };
            if values.insert(p.name.clone(), v).is_some() {
                return Err(no_match(&format!("{} given twice", p.name)));
            }
        }
        for p in &parms {
            if values.contains_key(&p.name) {
                continue;
            }
            if p.default.is_empty() || p.default == "-" {
                return Err(no_match(&format!("no argument for {}", p.name)));
            }
            values.insert(p.name.clone(), default(&p.default, ctx));
        }
        Ok(Self { values })
    }

    /// An argument; null when absent.
    #[must_use]
    pub fn get(&self, name: &str) -> &Value {
        static NULL: Value = Value::Null;
        self.values.get(name).unwrap_or(&NULL)
    }

    /// An argument as trimmed upper-case text; `None` when null.
    #[must_use]
    pub fn word(&self, name: &str) -> Option<String> {
        match self.get(name) {
            Value::Null => None,
            v => Some(v.to_string().trim().to_ascii_uppercase()),
        }
    }

    /// An argument as text, as given; `None` when null.
    #[must_use]
    pub fn text(&self, name: &str) -> Option<String> {
        match self.get(name) {
            Value::Null => None,
            Value::Str(s) => Some(s.clone()),
            v => Some(v.to_string()),
        }
    }

    /// An argument as an integer; `None` when null or not a number.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)] // arguments are small counts
    pub fn int(&self, name: &str) -> Option<i64> {
        match self.get(name) {
            Value::Null => None,
            v => v.as_decimal().ok().and_then(|d| d.to_string().parse::<f64>().ok()).map(|f| f as i64),
        }
    }
}

/// A default as the oracle writes it: a string, a number, `NULL`,
/// `USER`; anything else null.
fn default(text: &str, ctx: &SqlContext) -> Value {
    let t = text.trim();
    if let Some(inner) = t.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')) {
        return Value::Str(inner.replace("''", "'"));
    }
    if t.eq_ignore_ascii_case("USER") {
        return Value::Str(ctx.user.clone());
    }
    if let Ok(n) = t.parse::<i64>() {
        return Value::int(n);
    }
    Value::Null
}
