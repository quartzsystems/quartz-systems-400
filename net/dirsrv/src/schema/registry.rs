// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The schema registry: attribute types and object classes by name and
//! OID (case-insensitive), resolved against each other, with the
//! syntaxes and matching rules they name.

use std::collections::{BTreeSet, HashMap, HashSet};

use ldap::schema::{
    AttributeTypeDesc, ClassKind, IbmAttributeType, MatchingRuleDesc, ObjectClassDesc, SyntaxDesc,
    Usage,
};

use super::model::{AccessClass, AttributeType, ObjectClass};
use super::rule::{self, Kind, Rule};
use super::syntax;

/// Why a definition cannot go in the schema.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SchemaError {
    /// A description that does not parse.
    #[error("schema description does not parse: {0}")]
    Parse(String),
    /// An OID or name already defined by another definition.
    #[error("{0} is already defined")]
    Duplicate(String),
    /// A definition to delete or replace that is not there.
    #[error("{0} is not defined")]
    NotFound(String),
    /// A supertype or superclass that is not defined.
    #[error("{what} names {name}, which is not defined")]
    Unknown {
        /// The definition.
        what: String,
        /// The name it names.
        name: String,
    },
    /// A supertype or superclass chain that loops.
    #[error("{0} is its own supertype")]
    Cycle(String),
    /// A matching rule of the wrong kind for its place.
    #[error("{what}: {rule} is not an {kind} rule")]
    WrongRule {
        /// The definition.
        what: String,
        /// The rule.
        rule: String,
        /// The kind wanted.
        kind: &'static str,
    },
    /// An attribute type without a syntax, own or inherited.
    #[error("{0} has no syntax")]
    NoSyntax(String),
    /// A definition still used by another.
    #[error("{0} is used by {1}")]
    InUse(String, String),
    /// A definition QS/400 needs, or a kind of definition (syntax,
    /// matching rule) that cannot be changed.
    #[error("{0} cannot be changed")]
    Protected(String),
}

impl SchemaError {
    /// The LDAP result code for a schema modification that fails so.
    #[must_use]
    pub fn code(&self) -> ldap::ResultCode {
        use ldap::ResultCode as R;
        match self {
            Self::Parse(_) => R::InvalidAttributeSyntax,
            Self::Duplicate(_) => R::AttributeOrValueExists,
            Self::NotFound(_) => R::NoSuchAttribute,
            Self::Protected(_) => R::UnwillingToPerform,
            Self::Unknown { .. }
            | Self::Cycle(_)
            | Self::WrongRule { .. }
            | Self::NoSyntax(_)
            | Self::InUse(..) => R::ConstraintViolation,
        }
    }
}

/// The schema: every attribute type and object class, resolved, and the
/// syntax and matching rule descriptions published in `cn=schema`.
#[derive(Debug, Clone, Default)]
pub struct Schema {
    attrs: Vec<AttributeType>,
    attr_ix: HashMap<String, usize>,
    classes: Vec<ObjectClass>,
    class_ix: HashMap<String, usize>,
    syntaxes: Vec<SyntaxDesc>,
    rules: Vec<MatchingRuleDesc>,
    /// OIDs QS/400 itself relies on: not deletable or replaceable.
    protected: HashSet<String>,
    /// Attribute type and object class OIDs as shipped.
    pub(super) shipped: HashSet<String>,
    /// OIDs added or replaced since: what `V3.modifiedschema` holds.
    pub(super) changed: BTreeSet<String>,
    /// Shipped attribute types deleted since.
    pub(super) deleted_attrs: BTreeSet<String>,
    /// Shipped object classes deleted since.
    pub(super) deleted_classes: BTreeSet<String>,
}

fn key(s: &str) -> String {
    s.to_ascii_lowercase()
}

fn rule_for(
    what: &str,
    id: Option<&str>,
    kind: Kind,
) -> Result<Option<&'static Rule>, SchemaError> {
    let Some(id) = id else { return Ok(None) };
    let r = rule::find(id).ok_or_else(|| SchemaError::Unknown {
        what: what.into(),
        name: id.into(),
    })?;
    // IBM's schema gives a few attributes an ordering rule as equality
    // and so on; only a substring rule in a non-substring place, or the
    // reverse, is refused.
    let fits = match kind {
        Kind::Substring => r.kind == Kind::Substring,
        Kind::Equality | Kind::Ordering => r.kind != Kind::Substring,
    };
    if fits {
        Ok(Some(r))
    } else {
        Err(SchemaError::WrongRule {
            what: what.into(),
            rule: id.into(),
            kind: match kind {
                Kind::Equality => "equality",
                Kind::Ordering => "ordering",
                Kind::Substring => "substring",
            },
        })
    }
}

/// A syntax written `OID{len}` split.
fn split_syntax(s: &str) -> (String, Option<u32>) {
    match s.find('{') {
        Some(p) => (
            s[..p].trim().to_string(),
            s[p + 1..].trim_end_matches('}').trim().parse().ok(),
        ),
        None => (s.trim().to_string(), None),
    }
}

impl Schema {
    /// An empty schema.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Every attribute type, in definition order.
    pub fn attributes(&self) -> impl Iterator<Item = &AttributeType> {
        self.attrs.iter()
    }

    /// Every object class, in definition order.
    pub fn classes(&self) -> impl Iterator<Item = &ObjectClass> {
        self.classes.iter()
    }

    /// The syntax descriptions published.
    #[must_use]
    pub fn syntaxes(&self) -> &[SyntaxDesc] {
        &self.syntaxes
    }

    /// The matching rule descriptions published.
    #[must_use]
    pub fn matching_rules(&self) -> &[MatchingRuleDesc] {
        &self.rules
    }

    /// The attribute type with this name or OID (case ignored).
    #[must_use]
    pub fn attribute(&self, id: &str) -> Option<&AttributeType> {
        self.attr_ix.get(&key(id)).map(|&i| &self.attrs[i])
    }

    /// The object class with this name or OID (case ignored).
    #[must_use]
    pub fn class(&self, id: &str) -> Option<&ObjectClass> {
        self.class_ix.get(&key(id)).map(|&i| &self.classes[i])
    }

    /// Whether QS/400 relies on the definition with this OID.
    #[must_use]
    pub fn is_protected(&self, oid: &str) -> bool {
        self.protected.contains(oid)
    }

    /// Marks definitions QS/400 relies on.
    pub fn protect<'a>(&mut self, oids: impl IntoIterator<Item = &'a str>) {
        self.protected.extend(oids.into_iter().map(str::to_string));
    }

    /// Records every definition present as shipped: later changes are
    /// what [`Schema::modified_text`] writes.
    pub fn mark_shipped(&mut self) {
        self.shipped = self
            .attrs
            .iter()
            .map(|a| a.def.oid.clone())
            .chain(self.classes.iter().map(|c| c.def.oid.clone()))
            .collect();
        self.changed.clear();
        self.deleted_attrs.clear();
        self.deleted_classes.clear();
    }

    /// Adds a syntax description to publish.
    pub fn add_syntax(&mut self, d: SyntaxDesc) {
        if let Some(old) = self.syntaxes.iter_mut().find(|s| s.oid == d.oid) {
            *old = d;
        } else {
            self.syntaxes.push(d);
        }
    }

    /// Adds a matching rule description to publish.
    ///
    /// # Errors
    /// [`SchemaError::Unknown`] for a rule QS/400 has no implementation
    /// of.
    pub fn add_matching_rule(&mut self, d: MatchingRuleDesc) -> Result<(), SchemaError> {
        if rule::find(&d.oid).is_none() {
            return Err(SchemaError::Unknown {
                what: "matchingRules".into(),
                name: d.oid,
            });
        }
        if let Some(old) = self.rules.iter_mut().find(|s| s.oid == d.oid) {
            *old = d;
        } else {
            self.rules.push(d);
        }
        Ok(())
    }

    fn check_free(
        ix: &HashMap<String, usize>,
        oid: &str,
        names: &[String],
        skip: Option<usize>,
    ) -> Result<(), SchemaError> {
        for n in std::iter::once(oid).chain(names.iter().map(String::as_str)) {
            if ix.get(&key(n)).is_some_and(|&i| Some(i) != skip) {
                return Err(SchemaError::Duplicate(n.to_string()));
            }
        }
        Ok(())
    }

    /// Adds an attribute type, unresolved until [`Schema::resolve`].
    ///
    /// # Errors
    /// [`SchemaError::Duplicate`] when its OID or a name is taken.
    pub fn add_attribute(
        &mut self,
        def: AttributeTypeDesc,
        ibm: Option<IbmAttributeType>,
    ) -> Result<(), SchemaError> {
        Self::check_free(&self.attr_ix, &def.oid, &def.names, None)?;
        let i = self.attrs.len();
        for n in std::iter::once(&def.oid).chain(&def.names) {
            self.attr_ix.insert(key(n), i);
        }
        self.attrs.push(AttributeType {
            def,
            ibm,
            sup: None,
            syntax: None,
            syntax_len: None,
            equality: None,
            ordering: None,
            substr: None,
            access: AccessClass::Normal,
        });
        Ok(())
    }

    /// Sets IBM's extension values for an attribute type already added.
    ///
    /// # Errors
    /// [`SchemaError::NotFound`] when the attribute is not defined.
    pub fn set_ibm(&mut self, ibm: IbmAttributeType) -> Result<(), SchemaError> {
        let i = *self
            .attr_ix
            .get(&key(&ibm.oid))
            .ok_or_else(|| SchemaError::NotFound(ibm.oid.clone()))?;
        self.attrs[i].ibm = Some(ibm);
        Ok(())
    }

    /// Drops IBM's extension values of an attribute type.
    pub(super) fn clear_ibm(&mut self, oid: &str) {
        if let Some(&i) = self.attr_ix.get(&key(oid)) {
            self.attrs[i].ibm = None;
        }
    }

    /// Adds an object class, unresolved until [`Schema::resolve`].
    ///
    /// # Errors
    /// [`SchemaError::Duplicate`] when its OID or a name is taken.
    pub fn add_class(&mut self, def: ObjectClassDesc) -> Result<(), SchemaError> {
        Self::check_free(&self.class_ix, &def.oid, &def.names, None)?;
        let i = self.classes.len();
        for n in std::iter::once(&def.oid).chain(&def.names) {
            self.class_ix.insert(key(n), i);
        }
        self.classes.push(ObjectClass {
            def,
            supers: Vec::new(),
            must: Vec::new(),
            may: Vec::new(),
        });
        Ok(())
    }

    /// Replaces the attribute type with the same OID.
    ///
    /// # Errors
    /// [`SchemaError`] when it is not there, is protected, or a new name
    /// is taken.
    pub fn replace_attribute(&mut self, def: AttributeTypeDesc) -> Result<(), SchemaError> {
        let i = *self
            .attr_ix
            .get(&key(&def.oid))
            .ok_or_else(|| SchemaError::NotFound(def.oid.clone()))?;
        if self.is_protected(&def.oid) {
            return Err(SchemaError::Protected(def.oid));
        }
        Self::check_free(&self.attr_ix, &def.oid, &def.names, Some(i))?;
        self.attrs[i].def = def;
        self.reindex();
        Ok(())
    }

    /// Replaces the object class with the same OID.
    ///
    /// # Errors
    /// As [`Schema::replace_attribute`].
    pub fn replace_class(&mut self, def: ObjectClassDesc) -> Result<(), SchemaError> {
        let i = *self
            .class_ix
            .get(&key(&def.oid))
            .ok_or_else(|| SchemaError::NotFound(def.oid.clone()))?;
        if self.is_protected(&def.oid) {
            return Err(SchemaError::Protected(def.oid));
        }
        Self::check_free(&self.class_ix, &def.oid, &def.names, Some(i))?;
        self.classes[i].def = def;
        self.reindex();
        Ok(())
    }

    /// Deletes an attribute type no other definition uses.
    ///
    /// # Errors
    /// [`SchemaError`] when it is not there, protected or in use.
    pub fn delete_attribute(&mut self, id: &str) -> Result<AttributeType, SchemaError> {
        self.remove_attribute(id, true)
    }

    /// Deletes an attribute type; `check_use` refuses one still used
    /// (without it, [`Schema::resolve`] finds what is left dangling).
    pub(super) fn remove_attribute(
        &mut self,
        id: &str,
        check_use: bool,
    ) -> Result<AttributeType, SchemaError> {
        let i = *self
            .attr_ix
            .get(&key(id))
            .ok_or_else(|| SchemaError::NotFound(id.into()))?;
        let oid = self.attrs[i].def.oid.clone();
        if self.is_protected(&oid) {
            return Err(SchemaError::Protected(oid));
        }
        let at = &self.attrs[i];
        if check_use {
            if let Some(sub) = self
                .attrs
                .iter()
                .find(|a| a.def.sup.as_deref().is_some_and(|s| at.is_named(s)))
            {
                return Err(SchemaError::InUse(at.name().into(), sub.name().into()));
            }
            if let Some(c) = self
                .classes
                .iter()
                .find(|c| c.def.must.iter().chain(&c.def.may).any(|n| at.is_named(n)))
            {
                return Err(SchemaError::InUse(at.name().into(), c.name().into()));
            }
        }
        let gone = self.attrs.remove(i);
        self.reindex();
        Ok(gone)
    }

    /// Deletes an object class no other class has as a superclass.
    ///
    /// # Errors
    /// As [`Schema::delete_attribute`].
    pub fn delete_class(&mut self, id: &str) -> Result<ObjectClass, SchemaError> {
        self.remove_class(id, true)
    }

    /// Deletes an object class; `check_use` as [`Schema::remove_attribute`].
    pub(super) fn remove_class(
        &mut self,
        id: &str,
        check_use: bool,
    ) -> Result<ObjectClass, SchemaError> {
        let i = *self
            .class_ix
            .get(&key(id))
            .ok_or_else(|| SchemaError::NotFound(id.into()))?;
        let oid = self.classes[i].def.oid.clone();
        if self.is_protected(&oid) {
            return Err(SchemaError::Protected(oid));
        }
        let c = &self.classes[i];
        let named =
            |n: &String| n == c.oid() || c.def.names.iter().any(|x| x.eq_ignore_ascii_case(n));
        if let Some(sub) = self
            .classes
            .iter()
            .find(|s| s.def.sup.iter().any(named))
            .filter(|_| check_use)
        {
            return Err(SchemaError::InUse(c.name().into(), sub.name().into()));
        }
        let gone = self.classes.remove(i);
        self.reindex();
        Ok(gone)
    }

    fn reindex(&mut self) {
        self.attr_ix.clear();
        for (i, a) in self.attrs.iter().enumerate() {
            for n in std::iter::once(&a.def.oid).chain(&a.def.names) {
                self.attr_ix.insert(key(n), i);
            }
        }
        self.class_ix.clear();
        for (i, c) in self.classes.iter().enumerate() {
            for n in std::iter::once(&c.def.oid).chain(&c.def.names) {
                self.class_ix.insert(key(n), i);
            }
        }
    }

    /// Resolves every definition: supertypes, inherited syntaxes and
    /// rules, access classes, superclass closures, MUST and MAY.
    ///
    /// # Errors
    /// [`SchemaError`] for the first inconsistency.
    pub fn resolve(&mut self) -> Result<(), SchemaError> {
        self.resolve_attributes()?;
        self.resolve_classes()
    }

    #[allow(clippy::many_single_char_names)]
    fn resolve_attributes(&mut self) -> Result<(), SchemaError> {
        let n = self.attrs.len();
        let mut sup = vec![None; n];
        for (i, a) in self.attrs.iter().enumerate() {
            if let Some(s) = &a.def.sup {
                let j = *self
                    .attr_ix
                    .get(&key(s))
                    .ok_or_else(|| SchemaError::Unknown {
                        what: a.name().into(),
                        name: s.clone(),
                    })?;
                sup[i] = Some(j);
            }
        }
        // Order by depth so a supertype resolves before its subtypes.
        let mut done = vec![false; n];
        let mut order = Vec::with_capacity(n);
        for start in 0..n {
            let mut chain = Vec::new();
            let mut cur = Some(start);
            while let Some(c) = cur {
                if done[c] {
                    break;
                }
                if chain.contains(&c) {
                    return Err(SchemaError::Cycle(self.attrs[c].name().into()));
                }
                chain.push(c);
                cur = sup[c];
            }
            for &c in chain.iter().rev() {
                done[c] = true;
                order.push(c);
            }
        }
        for i in order {
            let (syn, len, eq, ord, sub) = match sup[i] {
                Some(j) => {
                    let p = &self.attrs[j];
                    (
                        p.syntax.clone(),
                        p.syntax_len,
                        p.equality,
                        p.ordering,
                        p.substr,
                    )
                }
                None => (None, None, None, None, None),
            };
            let a = &self.attrs[i];
            let what = a.name().to_string();
            let (syntax, syntax_len) = match &a.def.syntax {
                Some(s) => {
                    let (o, l) = split_syntax(s);
                    (Some(o), l)
                }
                None => (syn, len),
            };
            if syntax.is_none() {
                return Err(SchemaError::NoSyntax(what));
            }
            let equality = rule_for(&what, a.def.equality.as_deref(), Kind::Equality)?.or(eq);
            let ordering = rule_for(&what, a.def.ordering.as_deref(), Kind::Ordering)?.or(ord);
            let substr = rule_for(&what, a.def.substr.as_deref(), Kind::Substring)?.or(sub);
            let access = a
                .ibm
                .as_ref()
                .and_then(|x| x.access_class.as_deref())
                .and_then(AccessClass::parse)
                .unwrap_or(if a.def.usage == Usage::UserApplications {
                    AccessClass::Normal
                } else {
                    AccessClass::System
                });
            let sup_oid = sup[i].map(|j| self.attrs[j].def.oid.clone());
            let a = &mut self.attrs[i];
            a.sup = sup_oid;
            a.syntax = syntax;
            a.syntax_len = syntax_len;
            a.equality = equality;
            a.ordering = ordering;
            a.substr = substr;
            a.access = access;
        }
        Ok(())
    }

    fn attr_oid(&self, what: &str, n: &str) -> Result<String, SchemaError> {
        self.attribute(n)
            .map(|a| a.def.oid.clone())
            .ok_or_else(|| SchemaError::Unknown {
                what: what.into(),
                name: n.into(),
            })
    }

    fn resolve_classes(&mut self) -> Result<(), SchemaError> {
        let n = self.classes.len();
        let mut direct: Vec<Vec<usize>> = Vec::with_capacity(n);
        for c in &self.classes {
            let mut v = Vec::new();
            for s in &c.def.sup {
                let j = *self
                    .class_ix
                    .get(&key(s))
                    .ok_or_else(|| SchemaError::Unknown {
                        what: c.name().into(),
                        name: s.clone(),
                    })?;
                v.push(j);
            }
            direct.push(v);
        }
        for i in 0..n {
            // Breadth first: nearest superclasses first.
            let mut supers: Vec<usize> = Vec::new();
            let mut queue: Vec<usize> = direct[i].clone();
            while let Some(j) = (!queue.is_empty()).then(|| queue.remove(0)) {
                if j == i {
                    return Err(SchemaError::Cycle(self.classes[i].name().into()));
                }
                if !supers.contains(&j) {
                    supers.push(j);
                    queue.extend(direct[j].iter().copied());
                }
                if supers.len() > n {
                    return Err(SchemaError::Cycle(self.classes[i].name().into()));
                }
            }
            let what = self.classes[i].name().to_string();
            let mut must = Vec::new();
            let mut may = Vec::new();
            for &c in std::iter::once(&i).chain(&supers) {
                for m in &self.classes[c].def.must {
                    let o = self.attr_oid(&what, m)?;
                    if !must.contains(&o) {
                        must.push(o);
                    }
                }
                for m in &self.classes[c].def.may {
                    let o = self.attr_oid(&what, m)?;
                    if !may.contains(&o) {
                        may.push(o);
                    }
                }
            }
            may.retain(|o| !must.contains(o));
            let supers = supers
                .iter()
                .map(|&j| self.classes[j].def.oid.clone())
                .collect();
            let c = &mut self.classes[i];
            c.supers = supers;
            c.must = must;
            c.may = may;
        }
        Ok(())
    }

    /// The structural classes among `classes` that no other of them is a
    /// subclass of: the entry's structural class when there is one.
    #[must_use]
    pub fn most_specific_structural<'a>(
        &'a self,
        classes: &[&'a ObjectClass],
    ) -> Vec<&'a ObjectClass> {
        let structural: Vec<_> = classes
            .iter()
            .copied()
            .filter(|c| c.kind() == ClassKind::Structural)
            .collect();
        structural
            .iter()
            .copied()
            .filter(|c| {
                !structural
                    .iter()
                    .any(|d| d.oid() != c.oid() && d.is_a(c.oid()))
            })
            .collect()
    }

    /// The numeric OID a descriptor names in any of the schema's
    /// namespaces.
    #[must_use]
    pub fn oid_of(&self, descr: &str) -> Option<String> {
        if syntax::is_numeric_oid(descr) {
            return Some(descr.to_string());
        }
        if let Some(a) = self.attribute(descr) {
            return Some(a.def.oid.clone());
        }
        if let Some(c) = self.class(descr) {
            return Some(c.def.oid.clone());
        }
        rule::find(descr).map(|r| r.oid.to_string())
    }
}
