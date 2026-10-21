// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The catalog: parsed from `catalog.toml`, checked for consistency, and
//! used to validate every value.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::error::SysValError;
use crate::value::Value;

/// Value type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Character.
    Char,
    /// Decimal.
    Dec,
    /// List of characters.
    List,
}

/// Catalog group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Group {
    /// Date and time.
    Dattim,
    /// Editing.
    Edt,
    /// Library list.
    Libl,
    /// Allocation.
    Alc,
    /// Messages and logging.
    Msg,
    /// Security.
    Sec,
    /// Storage.
    Stg,
    /// System control.
    Sysctl,
    /// Network.
    Net,
}

/// One catalog entry.
#[derive(Clone, Debug, Deserialize)]
pub struct Entry {
    /// System value name.
    pub name: String,
    /// Type.
    #[serde(rename = "type")]
    pub kind: Kind,
    /// Characters (char) or characters per element (list).
    #[serde(default)]
    pub len: Option<usize>,
    /// Maximum elements (list).
    #[serde(default)]
    pub elems: Option<usize>,
    /// Default value.
    pub default: toml::Value,
    /// Allowed values (char).
    #[serde(default)]
    pub allowed: Option<Vec<String>>,
    /// Allowed range (dec).
    #[serde(default)]
    pub range: Option<[i64; 2]>,
    /// Takes effect at the next IPL.
    #[serde(default)]
    pub restart_required: bool,
    /// Set by the system only.
    #[serde(default)]
    pub readonly: bool,
    /// One-line description.
    pub text: String,
    /// Group.
    pub group: Group,
}

#[derive(Deserialize)]
struct File {
    sysval: Vec<Entry>,
}

/// The parsed catalog, by name.
#[derive(Debug)]
pub struct Catalog {
    entries: BTreeMap<String, Entry>,
}

impl Catalog {
    /// Parse and check the TOML text.
    pub fn parse(text: &str) -> Result<Self, SysValError> {
        let file: File = toml::from_str(text).map_err(|e| SysValError::Catalog(e.to_string()))?;
        let mut entries = BTreeMap::new();
        for e in file.sysval {
            check_entry(&e)?;
            if entries.insert(e.name.clone(), e.clone()).is_some() {
                return Err(SysValError::Catalog(format!("duplicate name {}", e.name)));
            }
        }
        Ok(Self { entries })
    }

    /// The compiled-in catalog.
    pub fn builtin() -> Result<Self, SysValError> {
        Self::parse(crate::CATALOG_TOML)
    }

    /// Look up an entry.
    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.entries.get(name)
    }

    /// Every entry, by name.
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.values()
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when the catalog is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Entry {
    /// The default as a typed value.
    pub fn default_value(&self) -> Value {
        match (&self.kind, &self.default) {
            (Kind::Char, toml::Value::String(s)) => Value::Char(s.clone()),
            (Kind::Dec, toml::Value::Integer(n)) => Value::Dec(*n),
            (Kind::List, toml::Value::Array(a)) => Value::List(
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect(),
            ),
            _ => Value::Char(String::new()),
        }
    }

    /// Parse user text into a value of this entry's type. Lists are space
    /// separated.
    pub fn parse_value(&self, text: &str) -> Result<Value, SysValError> {
        match self.kind {
            Kind::Char => Ok(Value::Char(text.to_string())),
            Kind::Dec => text
                .trim()
                .parse()
                .map(Value::Dec)
                .map_err(|_| SysValError::NotANumber {
                    name: self.name.clone(),
                    got: text.to_string(),
                }),
            Kind::List => Ok(Value::List(
                text.split_whitespace().map(str::to_string).collect(),
            )),
        }
    }

    /// Check a value against type, length, allowed set and range.
    pub fn validate(&self, value: &Value) -> Result<(), SysValError> {
        let expected = match self.kind {
            Kind::Char => "char",
            Kind::Dec => "dec",
            Kind::List => "list",
        };
        match (self.kind, value) {
            (Kind::Char, Value::Char(s)) => {
                if let Some(max) = self.len
                    && s.chars().count() > max
                {
                    return Err(SysValError::TooLong {
                        name: self.name.clone(),
                        max,
                        got: s.chars().count(),
                    });
                }
                if let Some(allowed) = &self.allowed
                    && !allowed.iter().any(|a| a == s)
                {
                    return Err(SysValError::NotAllowed {
                        name: self.name.clone(),
                        got: s.clone(),
                        allowed: allowed.join(", "),
                    });
                }
                Ok(())
            }
            (Kind::Dec, Value::Dec(n)) => {
                if let Some([min, max]) = self.range
                    && (*n < min || *n > max)
                {
                    return Err(SysValError::OutOfRange {
                        name: self.name.clone(),
                        min,
                        max,
                        got: *n,
                    });
                }
                Ok(())
            }
            (Kind::List, Value::List(items)) => {
                if let Some(max) = self.elems
                    && items.len() > max
                {
                    return Err(SysValError::TooMany {
                        name: self.name.clone(),
                        max,
                        got: items.len(),
                    });
                }
                if let Some(max) = self.len
                    && let Some(long) = items.iter().find(|i| i.chars().count() > max)
                {
                    return Err(SysValError::TooLong {
                        name: self.name.clone(),
                        max,
                        got: long.chars().count(),
                    });
                }
                Ok(())
            }
            _ => Err(SysValError::WrongType {
                name: self.name.clone(),
                expected,
            }),
        }
    }
}

/// Catalog-level consistency: the shape matches the type, text exists,
/// the default validates against the entry's own rules.
fn check_entry(e: &Entry) -> Result<(), SysValError> {
    let bad = |why: &str| Err(SysValError::Catalog(format!("{}: {why}", e.name)));
    if e.text.trim().is_empty() {
        return bad("missing text");
    }
    if e.name.is_empty() || !e.name.starts_with('Q') || e.name.len() > 10 {
        return bad("name must start with Q and be at most 10 characters");
    }
    match e.kind {
        Kind::Char => {
            if e.len.is_none() || e.elems.is_some() || e.range.is_some() {
                return bad("char needs len and neither elems nor range");
            }
        }
        Kind::Dec => {
            if e.len.is_some() || e.elems.is_some() || e.allowed.is_some() {
                return bad("dec takes range only");
            }
        }
        Kind::List => {
            if e.len.is_none() || e.elems.is_none() || e.range.is_some() {
                return bad("list needs len and elems");
            }
        }
    }
    let default = e.default_value();
    if default.kind_name()
        != match e.kind {
            Kind::Char => "char",
            Kind::Dec => "dec",
            Kind::List => "list",
        }
    {
        return bad("default has the wrong type");
    }
    e.validate(&default)
        .map_err(|err| SysValError::Catalog(format!("{}: default invalid: {err}", e.name)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_catalog_is_consistent() {
        let c = Catalog::builtin().expect("catalog parses and checks");
        assert!(c.len() >= 150, "{} entries", c.len());
        for e in c.entries() {
            assert!(!e.text.is_empty(), "{} has no text", e.name);
        }
    }

    #[test]
    fn required_phase1_values_exist() {
        let c = Catalog::builtin().unwrap();
        for name in [
            "QDATE",
            "QTIME",
            "QYEAR",
            "QMONTH",
            "QDAY",
            "QHOUR",
            "QMINUTE",
            "QSECOND",
            "QCENTURY",
            "QDATFMT",
            "QDATSEP",
            "QTIMSEP",
            "QUTCOFFSET",
            "QTIMZON",
            "QDAYOFWEEK",
            "QLEAPADJ",
            "QSECURITY",
            "QCTLSBSD",
            "QSTRUPPGM",
            "QSYSLIBL",
            "QUSRLIBL",
            "QCCSID",
            "QCHRID",
            "QLANGID",
            "QCNTRYID",
            "QKBDTYPE",
            "QAUTOVRT",
            "QINACTITV",
            "QINACTMSGQ",
            "QMAXSIGN",
            "QMAXSGNACN",
            "QPWDLVL",
            "QPWDMINLEN",
            "QPWDEXPITV",
            "QMODEL",
            "QSRLNBR",
            "QPRCFEAT",
            "QIPLSTS",
            "QIPLTYPE",
            "QIPLDATTIM",
            "QABNORMSW",
            "QPWRRSTIPL",
            "QRMTIPL",
            "QAUTOCFG",
            "QAUTORMT",
            "QDEVNAMING",
            "QCONSOLE",
            "QASTLVL",
            "QSTSMSG",
            "QHSTLOGSIZ",
            "QSYSNAME",
        ] {
            assert!(c.get(name).is_some(), "{name} missing");
        }
    }

    #[test]
    fn duplicates_are_rejected() {
        let text = "[[sysval]]\nname='QA'\ntype='char'\nlen=1\ndefault='x'\ntext='t'\ngroup='SEC'\n\
                    [[sysval]]\nname='QA'\ntype='char'\nlen=1\ndefault='x'\ntext='t'\ngroup='SEC'\n";
        assert!(matches!(Catalog::parse(text), Err(SysValError::Catalog(_))));
    }

    #[test]
    fn validation_rules() {
        let c = Catalog::builtin().unwrap();
        let sec = c.get("QSECURITY").unwrap();
        assert!(sec.validate(&Value::Char("50".into())).is_ok());
        assert!(matches!(
            sec.validate(&Value::Char("60".into())),
            Err(SysValError::NotAllowed { .. })
        ));
        assert!(matches!(
            sec.validate(&Value::Dec(50)),
            Err(SysValError::WrongType { .. })
        ));
        let lvl = c.get("QPWDLVL").unwrap();
        assert!(matches!(
            lvl.validate(&Value::Dec(9)),
            Err(SysValError::OutOfRange { .. })
        ));
        assert!(matches!(
            lvl.parse_value("x"),
            Err(SysValError::NotANumber { .. })
        ));
        let libl = c.get("QSYSLIBL").unwrap();
        let too_many = Value::List((0..16).map(|i| format!("L{i}")).collect());
        assert!(matches!(
            libl.validate(&too_many),
            Err(SysValError::TooMany { .. })
        ));
        assert!(matches!(
            libl.validate(&Value::List(vec!["ELEVENCHARS".into()])),
            Err(SysValError::TooLong { .. })
        ));
    }
}
