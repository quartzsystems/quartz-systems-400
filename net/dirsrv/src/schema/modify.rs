// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Changes to `cn=schema` and the instance's `etc/V3.modifiedschema`
//! (docs/DIRSRV.md §6): a modify adds, replaces or deletes attribute
//! types and object classes, checked for consistency as a whole; what
//! differs from the shipped schema is the file's text.

use std::fmt::Write as _;

use ldap::msg::{ModOp, Modification};
use ldap::schema::SchemaValue;

use super::registry::{Schema, SchemaError};
use super::shipped::{lines, parse_line};

/// The version of the `V3.modifiedschema` text [`Schema::modified_text`]
/// writes and [`Schema::load_modified`] reads (docs/DIRSRV.md §6.4);
/// frozen at V1R2M0 (`tools/fmtcheck`).
pub const MODIFIED_FORMAT_VERSION: u32 = 1;

/// The file's attribute for a deleted shipped attribute type (a QS/400
/// keyword: the value is the OID).
pub const DELETED_ATTRIBUTE_TYPES: &str = "deletedAttributeTypes";
/// The file's attribute for a deleted shipped object class.
pub const DELETED_OBJECT_CLASSES: &str = "deletedObjectClasses";

/// The OID of a description: its first token after `(`.
fn first_oid(v: &str) -> Option<&str> {
    v.trim()
        .strip_prefix('(')?
        .split_whitespace()
        .next()
        .map(|t| t.trim_end_matches(')'))
        .filter(|t| !t.is_empty())
}

impl Schema {
    /// Applies a modify request's changes to `cn=schema`: all of them or
    /// none. `add` defines (an OID already defined is 20), `replace`
    /// redefines each value's OID or defines it, `delete` removes each
    /// value's OID; syntaxes, matching rules and any other attribute of
    /// the entry are refused (53).
    ///
    /// # Errors
    /// [`SchemaError`], whose [`SchemaError::code`] is the result.
    pub fn modify(&mut self, mods: &[Modification]) -> Result<(), SchemaError> {
        let mut next = self.clone();
        for m in mods {
            next.apply_one(m)?;
        }
        next.resolve()?;
        *self = next;
        Ok(())
    }

    fn apply_one(&mut self, m: &Modification) -> Result<(), SchemaError> {
        let attr = m
            .attr
            .ty
            .split(';')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        let kind = match attr.as_str() {
            "attributetypes" | "objectclasses" | "ibmattributetypes" => attr.as_str(),
            _ => return Err(SchemaError::Protected(m.attr.ty.clone())),
        };
        if matches!(m.op, ModOp::Increment) || (m.attr.vals.is_empty() && m.op != ModOp::Replace) {
            return Err(SchemaError::Protected(m.attr.ty.clone()));
        }
        for v in &m.attr.vals {
            let text = std::str::from_utf8(v)
                .map_err(|_| SchemaError::Parse("a value is not UTF-8".into()))?;
            if m.op == ModOp::Delete {
                let oid = first_oid(text).ok_or_else(|| SchemaError::Parse(text.into()))?;
                self.delete_def(kind, oid, true)?;
                continue;
            }
            let parsed =
                SchemaValue::parse(kind, text).map_err(|e| SchemaError::Parse(e.to_string()))?;
            self.define(parsed, m.op == ModOp::Replace)?;
        }
        Ok(())
    }

    fn delete_def(&mut self, kind: &str, oid: &str, check_use: bool) -> Result<(), SchemaError> {
        match kind {
            "attributetypes" => {
                let gone = self.remove_attribute(oid, check_use)?;
                let o = gone.def.oid;
                self.changed.remove(&o);
                if self.shipped.contains(&o) {
                    self.deleted_attrs.insert(o);
                }
            }
            "objectclasses" => {
                let gone = self.remove_class(oid, check_use)?;
                let o = gone.def.oid;
                self.changed.remove(&o);
                if self.shipped.contains(&o) {
                    self.deleted_classes.insert(o);
                }
            }
            _ => {
                let at = self
                    .attribute(oid)
                    .ok_or_else(|| SchemaError::NotFound(oid.into()))?;
                let o = at.def.oid.clone();
                if self.is_protected(&o) {
                    return Err(SchemaError::Protected(o));
                }
                self.clear_ibm(&o);
                self.changed.insert(o);
            }
        }
        Ok(())
    }

    /// Defines or, with `replace`, redefines.
    fn define(&mut self, v: SchemaValue, replace: bool) -> Result<(), SchemaError> {
        match v {
            SchemaValue::AttributeType(d) => {
                let oid = d.oid.clone();
                if replace && self.attribute(&oid).is_some() {
                    self.replace_attribute(d)?;
                } else {
                    self.add_attribute(d, None)?;
                }
                self.deleted_attrs.remove(&oid);
                self.changed.insert(oid);
            }
            SchemaValue::ObjectClass(d) => {
                let oid = d.oid.clone();
                if replace && self.class(&oid).is_some() {
                    self.replace_class(d)?;
                } else {
                    self.add_class(d)?;
                }
                self.deleted_classes.remove(&oid);
                self.changed.insert(oid);
            }
            SchemaValue::Ibm(d) => {
                let oid = self
                    .attribute(&d.oid)
                    .map(|a| a.def.oid.clone())
                    .ok_or_else(|| SchemaError::NotFound(d.oid.clone()))?;
                if self.is_protected(&oid) {
                    return Err(SchemaError::Protected(oid));
                }
                if !replace && self.attribute(&oid).is_some_and(|a| a.ibm.is_some()) {
                    return Err(SchemaError::Duplicate(oid));
                }
                self.set_ibm(d)?;
                self.changed.insert(oid);
            }
            other => return Err(SchemaError::Protected(other.attr().into())),
        }
        Ok(())
    }

    /// The text of `V3.modifiedschema`: every definition added or
    /// replaced since the shipped schema, and every shipped one deleted.
    #[must_use]
    pub fn modified_text(&self) -> String {
        let mut s =
            String::from("# QS/400 directory server: schema changes made through cn=schema.\n");
        for a in self.attributes().filter(|a| self.changed.contains(a.oid())) {
            let _ = writeln!(s, "attributeTypes: {}", a.def);
            if let Some(i) = &a.ibm {
                let _ = writeln!(s, "IBMAttributeTypes: {i}");
            }
        }
        for c in self.classes().filter(|c| self.changed.contains(c.oid())) {
            let _ = writeln!(s, "objectClasses: {}", c.def);
        }
        for o in &self.deleted_attrs {
            let _ = writeln!(s, "{DELETED_ATTRIBUTE_TYPES}: {o}");
        }
        for o in &self.deleted_classes {
            let _ = writeln!(s, "{DELETED_OBJECT_CLASSES}: {o}");
        }
        s
    }

    /// Applies a `V3.modifiedschema` text over this (shipped) schema:
    /// deletions first, then definitions, then resolution.
    ///
    /// # Errors
    /// [`SchemaError`] naming the line that fails.
    pub fn load_modified(&mut self, text: &str) -> Result<(), SchemaError> {
        let ls = lines(text)?;
        let mut next = self.clone();
        // Deletions go first, unchecked: resolution checks what remains.
        for want in [DELETED_OBJECT_CLASSES, DELETED_ATTRIBUTE_TYPES] {
            for l in ls.iter().filter(|l| l.attr.eq_ignore_ascii_case(want)) {
                let kind = if want == DELETED_OBJECT_CLASSES {
                    "objectclasses"
                } else {
                    "attributetypes"
                };
                next.delete_def(kind, l.value.trim(), false)
                    .map_err(|e| SchemaError::Parse(format!("line {}: {e}", l.number)))?;
            }
        }
        for l in &ls {
            if l.attr.eq_ignore_ascii_case(DELETED_ATTRIBUTE_TYPES)
                || l.attr.eq_ignore_ascii_case(DELETED_OBJECT_CLASSES)
            {
                continue;
            }
            let v = parse_line(l)?;
            next.define(v, true)
                .map_err(|e| SchemaError::Parse(format!("line {}: {e}", l.number)))?;
        }
        next.resolve()?;
        *self = next;
        Ok(())
    }
}
