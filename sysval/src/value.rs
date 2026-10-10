// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A system value's value.

use std::fmt;

/// Typed value. `List` elements are character strings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// Character value.
    Char(String),
    /// Decimal value.
    Dec(i64),
    /// List of character values.
    List(Vec<String>),
}

impl Value {
    /// Text form: the string, the number, or the elements space separated.
    pub fn text(&self) -> String {
        match self {
            Value::Char(s) => s.clone(),
            Value::Dec(n) => n.to_string(),
            Value::List(v) => v.join(" "),
        }
    }

    /// The type name used in messages.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Value::Char(_) => "char",
            Value::Dec(_) => "dec",
            Value::List(_) => "list",
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text())
    }
}
