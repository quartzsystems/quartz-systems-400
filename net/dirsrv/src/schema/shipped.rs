// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The shipped schema: QS/400's files, embedded in the build and
//! written to `/QIBM/ProdData/OS400/DirSrv/etc/` (docs/DIRSRV.md §6),
//! and the text form they and `V3.modifiedschema` share.

use ldap::schema::SchemaValue;

use super::registry::{Schema, SchemaError};

/// Where the shipped files go in the IFS.
pub const SHIPPED_DIR: &str = "/QIBM/ProdData/OS400/DirSrv/etc";

/// The shipped files, in load order: name and contents.
pub const FILES: &[(&str, &str)] = &[
    (
        "qs.ldapsyntaxes",
        include_str!("../../schema/qs.ldapsyntaxes"),
    ),
    (
        "qs.matchingrules",
        include_str!("../../schema/qs.matchingrules"),
    ),
    ("qs.system.at", include_str!("../../schema/qs.system.at")),
    ("qs.user.at", include_str!("../../schema/qs.user.at")),
    ("qs.os400.at", include_str!("../../schema/qs.os400.at")),
    ("qs.ibm.at", include_str!("../../schema/qs.ibm.at")),
    ("qs.system.oc", include_str!("../../schema/qs.system.oc")),
    ("qs.user.oc", include_str!("../../schema/qs.user.oc")),
    ("qs.os400.oc", include_str!("../../schema/qs.os400.oc")),
    ("qs.ibm.oc", include_str!("../../schema/qs.ibm.oc")),
];

/// Files whose definitions QS/400 relies on: they cannot be deleted or
/// replaced through `cn=schema`.
const PROTECTED: &[&str] = &["qs.system.at", "qs.system.oc", "qs.os400.at", "qs.os400.oc"];

/// One line of a schema file: the subschema attribute and its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// The line number where it starts (1-based).
    pub number: usize,
    /// The attribute (`attributeTypes`, `deletedObjectClasses`, ...).
    pub attr: String,
    /// The value.
    pub value: String,
}

/// The lines of a schema file: `#` comments and blank lines skipped,
/// LDIF folding undone (a line starting with a space continues the one
/// before, less that space).
///
/// # Errors
/// [`SchemaError::Parse`] for a line without `attr: value`.
pub fn lines(text: &str) -> Result<Vec<Line>, SchemaError> {
    let mut out: Vec<Line> = Vec::new();
    let mut open = false;
    for (i, raw) in text.lines().enumerate() {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        if let Some(cont) = raw.strip_prefix(' ') {
            match out.last_mut() {
                Some(l) if open => l.value.push_str(cont),
                _ => {
                    return Err(SchemaError::Parse(format!(
                        "line {}: continuation without a line",
                        i + 1
                    )));
                }
            }
            continue;
        }
        open = false;
        if raw.trim().is_empty() || raw.starts_with('#') {
            continue;
        }
        let (attr, value) = raw
            .split_once(':')
            .ok_or_else(|| SchemaError::Parse(format!("line {}: no attribute name", i + 1)))?;
        out.push(Line {
            number: i + 1,
            attr: attr.trim().to_string(),
            value: value.trim_start().to_string(),
        });
        open = true;
    }
    Ok(out)
}

/// Parses one line as a description.
///
/// # Errors
/// [`SchemaError::Parse`] naming the line.
pub fn parse_line(l: &Line) -> Result<SchemaValue, SchemaError> {
    SchemaValue::parse(&l.attr, &l.value)
        .map_err(|e| SchemaError::Parse(format!("line {}: {e}", l.number)))
}

impl Schema {
    /// Adds every description of a schema file, unresolved.
    ///
    /// # Errors
    /// [`SchemaError`] for a line that does not parse or a definition
    /// already present.
    pub fn load_text(&mut self, text: &str) -> Result<Vec<String>, SchemaError> {
        let mut oids = Vec::new();
        for l in lines(text)? {
            match parse_line(&l)? {
                SchemaValue::AttributeType(d) => {
                    oids.push(d.oid.clone());
                    self.add_attribute(d, None)?;
                }
                SchemaValue::ObjectClass(d) => {
                    oids.push(d.oid.clone());
                    self.add_class(d)?;
                }
                SchemaValue::Ibm(d) => self.set_ibm(d)?,
                SchemaValue::Syntax(d) => self.add_syntax(d),
                SchemaValue::MatchingRule(d) => self.add_matching_rule(d)?,
                other => {
                    return Err(SchemaError::Parse(format!(
                        "line {}: {} is not kept in schema files",
                        l.number,
                        other.attr()
                    )));
                }
            }
        }
        Ok(oids)
    }

    /// The shipped schema, resolved, its system and projected definitions
    /// protected.
    ///
    /// # Errors
    /// [`SchemaError`] only if the shipped files are inconsistent, which
    /// the tests rule out.
    pub fn shipped() -> Result<Self, SchemaError> {
        let mut s = Self::new();
        let mut protected = Vec::new();
        for (name, text) in FILES {
            let oids = s
                .load_text(text)
                .map_err(|e| SchemaError::Parse(format!("{name}: {e}")))?;
            if PROTECTED.contains(name) {
                protected.extend(oids);
            }
        }
        s.resolve()?;
        s.protect(protected.iter().map(String::as_str));
        s.mark_shipped();
        Ok(s)
    }
}
