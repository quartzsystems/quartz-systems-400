// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Shell variables and their attributes.

use std::collections::BTreeMap;

/// A variable.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Var {
    /// The value; `None` is declared (exported or read-only) but unset.
    pub value: Option<String>,
    /// Passed to commands' environments.
    pub exported: bool,
    /// Cannot be assigned or unset.
    pub readonly: bool,
    /// `declare -i`: assignments are evaluated as arithmetic.
    pub integer: bool,
}

/// An assignment the variable refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadOnlyError(pub String);

/// All variables, sorted by name for `set` and `export -p`.
#[derive(Clone, Debug, Default)]
pub struct Vars {
    map: BTreeMap<String, Var>,
}

impl Vars {
    /// A value, `None` when unset.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.map.get(name).and_then(|v| v.value.as_deref())
    }

    /// A variable with its attributes.
    pub fn var(&self, name: &str) -> Option<&Var> {
        self.map.get(name)
    }

    /// A variable for changing attributes, created unset if missing.
    pub fn entry(&mut self, name: &str) -> &mut Var {
        self.map.entry(name.to_string()).or_default()
    }

    /// Assign a value.
    pub fn set(&mut self, name: &str, value: &str) -> Result<(), ReadOnlyError> {
        let v = self.entry(name);
        if v.readonly {
            return Err(ReadOnlyError(name.to_string()));
        }
        v.value = Some(value.to_string());
        Ok(())
    }

    /// Remove a variable.
    pub fn unset(&mut self, name: &str) -> Result<(), ReadOnlyError> {
        if self.map.get(name).is_some_and(|v| v.readonly) {
            return Err(ReadOnlyError(name.to_string()));
        }
        self.map.remove(name);
        Ok(())
    }

    /// Replace a variable whole (restoring a saved one), ignoring
    /// read-only.
    pub fn restore(&mut self, name: &str, var: Option<Var>) {
        match var {
            Some(v) => {
                self.map.insert(name.to_string(), v);
            }
            None => {
                self.map.remove(name);
            }
        }
    }

    /// Every variable in name order.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Var)> {
        self.map.iter()
    }

    /// The exported variables with values, as an environment.
    pub fn environ(&self) -> Vec<(String, String)> {
        self.map
            .iter()
            .filter(|(_, v)| v.exported)
            .filter_map(|(k, v)| v.value.as_ref().map(|val| (k.clone(), val.clone())))
            .collect()
    }
}

/// Quote a value so the shell reads it back unchanged: as is when it
/// needs no quoting, else in single quotes.
pub fn quote(s: &str) -> String {
    if !s.is_empty()
        && s.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, '_' | '-' | '.' | '/' | ':' | ',' | '+' | '@' | '%')
        })
    {
        return s.to_string();
    }
    let mut out = String::from("'");
    for c in s.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}
