// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Schema descriptions (RFC 4512 §4.1) and IBM's `IBMAttributeTypes`
//! extension values, as parsed from `cn=schema` or a schema file. The
//! model that checks entries is `dirsrv::schema`'s.
//!
//! Each description parses with its `parse` and prints with `Display` in
//! RFC 4512's order; [`SchemaValue::parse`] picks the kind by the
//! subschema attribute's name.

use std::fmt;

/// How an attribute is used (RFC 4512 §4.1.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Usage {
    /// A user attribute.
    #[default]
    UserApplications,
    /// Operational, directory.
    DirectoryOperation,
    /// Operational, shared.
    DistributedOperation,
    /// Operational, this server.
    DsaOperation,
}

/// An attribute type description: RFC 4512's flags are bools as the
/// grammar has them.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AttributeTypeDesc {
    /// The OID.
    pub oid: String,
    /// The names, first the primary.
    pub names: Vec<String>,
    /// The description.
    pub desc: Option<String>,
    /// Obsolete.
    pub obsolete: bool,
    /// The supertype.
    pub sup: Option<String>,
    /// The equality matching rule.
    pub equality: Option<String>,
    /// The ordering matching rule.
    pub ordering: Option<String>,
    /// The substring matching rule.
    pub substr: Option<String>,
    /// The syntax OID with its length bound, as written.
    pub syntax: Option<String>,
    /// Single-valued.
    pub single_value: bool,
    /// Collective.
    pub collective: bool,
    /// No user modification.
    pub no_user_modification: bool,
    /// The usage.
    pub usage: Usage,
    /// `X-` and other extensions, name and values, in order.
    pub extensions: Vec<(String, Vec<String>)>,
}

/// The kind of an object class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClassKind {
    /// Abstract.
    Abstract,
    /// Structural.
    #[default]
    Structural,
    /// Auxiliary.
    Auxiliary,
}

/// An object class description.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ObjectClassDesc {
    /// The OID.
    pub oid: String,
    /// The names.
    pub names: Vec<String>,
    /// The description.
    pub desc: Option<String>,
    /// Obsolete.
    pub obsolete: bool,
    /// The superclasses.
    pub sup: Vec<String>,
    /// The kind.
    pub kind: ClassKind,
    /// Required attributes.
    pub must: Vec<String>,
    /// Allowed attributes.
    pub may: Vec<String>,
    /// Extensions.
    pub extensions: Vec<(String, Vec<String>)>,
}

/// A syntax description.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SyntaxDesc {
    /// The OID.
    pub oid: String,
    /// The description.
    pub desc: Option<String>,
    /// Extensions.
    pub extensions: Vec<(String, Vec<String>)>,
}

/// A matching rule description.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MatchingRuleDesc {
    /// The OID.
    pub oid: String,
    /// The names.
    pub names: Vec<String>,
    /// The description.
    pub desc: Option<String>,
    /// Obsolete.
    pub obsolete: bool,
    /// The assertion syntax.
    pub syntax: String,
    /// Extensions.
    pub extensions: Vec<(String, Vec<String>)>,
}

/// IBM's extension values for one attribute (`IBMAttributeTypes`):
/// its database name, access class and length.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IbmAttributeType {
    /// The attribute's OID.
    pub oid: String,
    /// The access class (`normal`, `sensitive`, `critical`, `system`,
    /// `restricted`).
    pub access_class: Option<String>,
    /// The maximum length.
    pub length: Option<u32>,
    /// The remaining keywords and values, in order.
    pub other: Vec<(String, Vec<String>)>,
}

/// A matching rule use description (RFC 4512 §4.1.4).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MatchingRuleUseDesc {
    /// The matching rule's OID.
    pub oid: String,
    /// The names.
    pub names: Vec<String>,
    /// The description.
    pub desc: Option<String>,
    /// Obsolete.
    pub obsolete: bool,
    /// The attribute types the rule applies to.
    pub applies: Vec<String>,
    /// Extensions.
    pub extensions: Vec<(String, Vec<String>)>,
}

/// A DIT content rule description (RFC 4512 §4.1.6).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DitContentRuleDesc {
    /// The structural class's OID.
    pub oid: String,
    /// The names.
    pub names: Vec<String>,
    /// The description.
    pub desc: Option<String>,
    /// Obsolete.
    pub obsolete: bool,
    /// Auxiliary classes allowed.
    pub aux: Vec<String>,
    /// Required attributes.
    pub must: Vec<String>,
    /// Allowed attributes.
    pub may: Vec<String>,
    /// Precluded attributes.
    pub not: Vec<String>,
    /// Extensions.
    pub extensions: Vec<(String, Vec<String>)>,
}

/// A DIT structure rule description (RFC 4512 §4.1.7.1).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DitStructureRuleDesc {
    /// The rule ID.
    pub rule_id: u32,
    /// The names.
    pub names: Vec<String>,
    /// The description.
    pub desc: Option<String>,
    /// Obsolete.
    pub obsolete: bool,
    /// The name form.
    pub form: String,
    /// The superior rules.
    pub sup: Vec<u32>,
    /// Extensions.
    pub extensions: Vec<(String, Vec<String>)>,
}

/// A name form description (RFC 4512 §4.1.7.2).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NameFormDesc {
    /// The OID.
    pub oid: String,
    /// The names.
    pub names: Vec<String>,
    /// The description.
    pub desc: Option<String>,
    /// Obsolete.
    pub obsolete: bool,
    /// The structural class it names.
    pub oc: String,
    /// Attributes the RDN must hold.
    pub must: Vec<String>,
    /// Attributes the RDN may hold.
    pub may: Vec<String>,
    /// Extensions.
    pub extensions: Vec<(String, Vec<String>)>,
}

/// Any description, by the subschema attribute it is a value of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaValue {
    /// `attributeTypes`.
    AttributeType(AttributeTypeDesc),
    /// `objectClasses`.
    ObjectClass(ObjectClassDesc),
    /// `ldapSyntaxes`.
    Syntax(SyntaxDesc),
    /// `matchingRules`.
    MatchingRule(MatchingRuleDesc),
    /// `matchingRuleUse`.
    MatchingRuleUse(MatchingRuleUseDesc),
    /// `dITContentRules`.
    DitContentRule(DitContentRuleDesc),
    /// `dITStructureRules`.
    DitStructureRule(DitStructureRuleDesc),
    /// `nameForms`.
    NameForm(NameFormDesc),
    /// IBM's `IBMAttributeTypes`.
    Ibm(IbmAttributeType),
}

/// Why a description does not parse.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("schema description at offset {at}: {what}")]
pub struct SchemaError {
    /// The byte offset.
    pub at: usize,
    /// What was wrong.
    pub what: String,
}

impl SchemaValue {
    /// Parses `value` as a value of the subschema attribute `attr`
    /// (its name compared ignoring ASCII case).
    ///
    /// # Errors
    ///
    /// A [`SchemaError`], or one at offset 0 for an attribute that holds
    /// no descriptions.
    pub fn parse(attr: &str, value: &str) -> Result<Self, SchemaError> {
        let a = attr.to_ascii_lowercase();
        Ok(match a.as_str() {
            "attributetypes" => Self::AttributeType(AttributeTypeDesc::parse(value)?),
            "objectclasses" => Self::ObjectClass(ObjectClassDesc::parse(value)?),
            "ldapsyntaxes" => Self::Syntax(SyntaxDesc::parse(value)?),
            "matchingrules" => Self::MatchingRule(MatchingRuleDesc::parse(value)?),
            "matchingruleuse" => Self::MatchingRuleUse(MatchingRuleUseDesc::parse(value)?),
            "ditcontentrules" => Self::DitContentRule(DitContentRuleDesc::parse(value)?),
            "ditstructurerules" => Self::DitStructureRule(DitStructureRuleDesc::parse(value)?),
            "nameforms" => Self::NameForm(NameFormDesc::parse(value)?),
            "ibmattributetypes" => Self::Ibm(IbmAttributeType::parse(value)?),
            _ => {
                return Err(SchemaError {
                    at: 0,
                    what: format!("{attr} holds no schema descriptions"),
                });
            }
        })
    }

    /// The subschema attribute this is a value of.
    #[must_use]
    pub const fn attr(&self) -> &'static str {
        match self {
            Self::AttributeType(_) => "attributeTypes",
            Self::ObjectClass(_) => "objectClasses",
            Self::Syntax(_) => "ldapSyntaxes",
            Self::MatchingRule(_) => "matchingRules",
            Self::MatchingRuleUse(_) => "matchingRuleUse",
            Self::DitContentRule(_) => "dITContentRules",
            Self::DitStructureRule(_) => "dITStructureRules",
            Self::NameForm(_) => "nameForms",
            Self::Ibm(_) => "IBMAttributeTypes",
        }
    }
}

impl fmt::Display for SchemaValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AttributeType(d) => d.fmt(f),
            Self::ObjectClass(d) => d.fmt(f),
            Self::Syntax(d) => d.fmt(f),
            Self::MatchingRule(d) => d.fmt(f),
            Self::MatchingRuleUse(d) => d.fmt(f),
            Self::DitContentRule(d) => d.fmt(f),
            Self::DitStructureRule(d) => d.fmt(f),
            Self::NameForm(d) => d.fmt(f),
            Self::Ibm(d) => d.fmt(f),
        }
    }
}

// Tokens.

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Open,
    Close,
    Dollar,
    Quoted(String),
    Word(String),
}

fn is_sep(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b'(' | b')' | b'$' | b'\'')
}

fn tokens(text: &str) -> Result<Vec<(usize, Tok)>, SchemaError> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' | b'\r' | b'\n' => i += 1,
            b'(' => {
                out.push((i, Tok::Open));
                i += 1;
            }
            b')' => {
                out.push((i, Tok::Close));
                i += 1;
            }
            b'$' => {
                out.push((i, Tok::Dollar));
                i += 1;
            }
            b'\'' => {
                let start = i;
                i += 1;
                let mut v = Vec::new();
                loop {
                    match bytes.get(i) {
                        None => {
                            return Err(SchemaError {
                                at: start,
                                what: "unterminated quoted string".into(),
                            });
                        }
                        Some(b'\'') => {
                            i += 1;
                            break;
                        }
                        Some(b'\\') => {
                            let hex = text
                                .get(i + 1..i + 3)
                                .and_then(|h| u8::from_str_radix(h, 16).ok());
                            if let Some(x) = hex {
                                v.push(x);
                                i += 3;
                            } else {
                                v.push(b'\\');
                                i += 1;
                            }
                        }
                        Some(&x) => {
                            v.push(x);
                            i += 1;
                        }
                    }
                }
                out.push((start, Tok::Quoted(String::from_utf8_lossy(&v).into_owned())));
            }
            _ => {
                let start = i;
                while i < bytes.len() && !is_sep(bytes[i]) {
                    i += 1;
                }
                out.push((start, Tok::Word(text[start..i].to_string())));
            }
        }
    }
    Ok(out)
}

/// The parser over the tokens of one description.
struct P {
    toks: Vec<(usize, Tok)>,
    i: usize,
    end: usize,
}

impl P {
    fn new(s: &str) -> Result<Self, SchemaError> {
        let mut p = Self {
            toks: tokens(s)?,
            i: 0,
            end: s.len(),
        };
        if p.next() != Some(Tok::Open) {
            return Err(p.err_at(0, "a description starts with '('"));
        }
        Ok(p)
    }

    fn at(&self) -> usize {
        self.toks.get(self.i).map_or(self.end, |t| t.0)
    }

    fn err_at(&self, at: usize, what: impl Into<String>) -> SchemaError {
        let _ = self;
        SchemaError {
            at,
            what: what.into(),
        }
    }

    fn err(&self, what: impl Into<String>) -> SchemaError {
        self.err_at(self.at(), what)
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.i).map(|t| &t.1)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.i).map(|t| t.1.clone());
        if t.is_some() {
            self.i += 1;
        }
        t
    }

    fn back(&mut self, what: &str) -> SchemaError {
        self.i = self.i.saturating_sub(1);
        self.err(what)
    }

    fn word(&mut self) -> Result<String, SchemaError> {
        match self.next() {
            Some(Tok::Word(w)) => Ok(w),
            _ => Err(self.back("expected a name or OID")),
        }
    }

    fn qdstring(&mut self) -> Result<String, SchemaError> {
        match self.next() {
            Some(Tok::Quoted(w)) => Ok(w),
            _ => Err(self.back("expected a quoted string")),
        }
    }

    /// `'a'` or `( 'a' 'b' )`; a bare word is taken too.
    fn qdstrings(&mut self) -> Result<Vec<String>, SchemaError> {
        match self.next() {
            Some(Tok::Quoted(w) | Tok::Word(w)) => Ok(vec![w]),
            Some(Tok::Open) => {
                let mut v = Vec::new();
                loop {
                    match self.next() {
                        Some(Tok::Close) => return Ok(v),
                        Some(Tok::Quoted(w) | Tok::Word(w)) => v.push(w),
                        _ => return Err(self.back("expected a quoted string or ')'")),
                    }
                }
            }
            _ => Err(self.back("expected a quoted string or '('")),
        }
    }

    /// `oid` or `( oid $ oid )`; the `$` may be left out.
    fn oids(&mut self) -> Result<Vec<String>, SchemaError> {
        match self.next() {
            Some(Tok::Word(w)) => Ok(vec![w]),
            Some(Tok::Open) => {
                let mut v = Vec::new();
                loop {
                    match self.next() {
                        Some(Tok::Close) if !v.is_empty() => return Ok(v),
                        Some(Tok::Dollar) if !v.is_empty() => {}
                        Some(Tok::Word(w)) => v.push(w),
                        _ => return Err(self.back("expected an OID list")),
                    }
                }
            }
            _ => Err(self.back("expected an OID or '('")),
        }
    }

    /// The next keyword, or none at the closing `)`.
    fn keyword(&mut self) -> Result<Option<(usize, String)>, SchemaError> {
        let at = self.at();
        match self.next() {
            Some(Tok::Close) => {
                if self.i < self.toks.len() {
                    return Err(self.err("text after the closing ')'"));
                }
                Ok(None)
            }
            Some(Tok::Word(w)) => Ok(Some((at, w.to_ascii_uppercase()))),
            None => Err(self.err_at(self.end, "missing ')'")),
            _ => Err(self.err_at(at, "expected a keyword")),
        }
    }

    fn unknown(&self, at: usize, k: &str) -> SchemaError {
        self.err_at(at, format!("unknown keyword {k}"))
    }

    /// The OID after the opening `(`.
    fn oid(&mut self) -> Result<String, SchemaError> {
        self.word()
    }
}

/// The fields every description has.
#[derive(Default)]
struct Common {
    names: Vec<String>,
    desc: Option<String>,
    obsolete: bool,
    extensions: Vec<(String, Vec<String>)>,
}

impl Common {
    /// Takes `k` if it is a common keyword.
    fn take(&mut self, p: &mut P, k: &str) -> Result<bool, SchemaError> {
        match k {
            "NAME" => self.names = p.qdstrings()?,
            "DESC" => self.desc = Some(p.qdstring()?),
            "OBSOLETE" => self.obsolete = true,
            _ if k.starts_with("X-") => {
                let v = p.qdstrings()?;
                self.extensions.push((k.to_string(), v));
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}

fn parse_usage(p: &P, at: usize, w: &str) -> Result<Usage, SchemaError> {
    Ok(match w.to_ascii_lowercase().as_str() {
        "userapplications" => Usage::UserApplications,
        "directoryoperation" => Usage::DirectoryOperation,
        "distributedoperation" => Usage::DistributedOperation,
        "dsaoperation" => Usage::DsaOperation,
        _ => return Err(p.err_at(at, format!("unknown usage {w}"))),
    })
}

impl AttributeTypeDesc {
    /// Parses an `AttributeTypeDescription`.
    ///
    /// # Errors
    ///
    /// A [`SchemaError`].
    pub fn parse(s: &str) -> Result<Self, SchemaError> {
        let mut p = P::new(s)?;
        let mut d = Self {
            oid: p.oid()?,
            ..Self::default()
        };
        let mut com = Common::default();
        while let Some((at, k)) = p.keyword()? {
            if com.take(&mut p, &k)? {
                continue;
            }
            match k.as_str() {
                "SUP" => d.sup = Some(p.word()?),
                "EQUALITY" => d.equality = Some(p.word()?),
                "ORDERING" => d.ordering = Some(p.word()?),
                "SUBSTR" => d.substr = Some(p.word()?),
                "SYNTAX" => d.syntax = Some(p.word()?),
                "SINGLE-VALUE" => d.single_value = true,
                "COLLECTIVE" => d.collective = true,
                "NO-USER-MODIFICATION" => d.no_user_modification = true,
                "USAGE" => {
                    let at = p.at();
                    let w = p.word()?;
                    d.usage = parse_usage(&p, at, &w)?;
                }
                _ => return Err(p.unknown(at, &k)),
            }
        }
        d.names = com.names;
        d.desc = com.desc;
        d.obsolete = com.obsolete;
        d.extensions = com.extensions;
        Ok(d)
    }
}

impl ObjectClassDesc {
    /// Parses an `ObjectClassDescription`.
    ///
    /// # Errors
    ///
    /// A [`SchemaError`].
    pub fn parse(s: &str) -> Result<Self, SchemaError> {
        let mut p = P::new(s)?;
        let mut d = Self {
            oid: p.oid()?,
            ..Self::default()
        };
        let mut com = Common::default();
        while let Some((at, k)) = p.keyword()? {
            if com.take(&mut p, &k)? {
                continue;
            }
            match k.as_str() {
                "SUP" => d.sup = p.oids()?,
                "ABSTRACT" => d.kind = ClassKind::Abstract,
                "STRUCTURAL" => d.kind = ClassKind::Structural,
                "AUXILIARY" => d.kind = ClassKind::Auxiliary,
                "MUST" => d.must = p.oids()?,
                "MAY" => d.may = p.oids()?,
                _ => return Err(p.unknown(at, &k)),
            }
        }
        d.names = com.names;
        d.desc = com.desc;
        d.obsolete = com.obsolete;
        d.extensions = com.extensions;
        Ok(d)
    }
}

impl SyntaxDesc {
    /// Parses a `SyntaxDescription`.
    ///
    /// # Errors
    ///
    /// A [`SchemaError`].
    pub fn parse(s: &str) -> Result<Self, SchemaError> {
        let mut p = P::new(s)?;
        let mut d = Self {
            oid: p.oid()?,
            ..Self::default()
        };
        let mut com = Common::default();
        while let Some((at, k)) = p.keyword()? {
            if k == "NAME" || k == "OBSOLETE" || !com.take(&mut p, &k)? {
                return Err(p.unknown(at, &k));
            }
        }
        d.desc = com.desc;
        d.extensions = com.extensions;
        Ok(d)
    }
}

impl MatchingRuleDesc {
    /// Parses a `MatchingRuleDescription`.
    ///
    /// # Errors
    ///
    /// A [`SchemaError`].
    pub fn parse(s: &str) -> Result<Self, SchemaError> {
        let mut p = P::new(s)?;
        let mut d = Self {
            oid: p.oid()?,
            ..Self::default()
        };
        let mut com = Common::default();
        let mut syntax = None;
        while let Some((at, k)) = p.keyword()? {
            if com.take(&mut p, &k)? {
                continue;
            }
            match k.as_str() {
                "SYNTAX" => syntax = Some(p.word()?),
                _ => return Err(p.unknown(at, &k)),
            }
        }
        d.syntax = syntax.ok_or_else(|| p.err_at(0, "a matching rule needs SYNTAX"))?;
        d.names = com.names;
        d.desc = com.desc;
        d.obsolete = com.obsolete;
        d.extensions = com.extensions;
        Ok(d)
    }
}

impl MatchingRuleUseDesc {
    /// Parses a `MatchingRuleUseDescription`.
    ///
    /// # Errors
    ///
    /// A [`SchemaError`].
    pub fn parse(s: &str) -> Result<Self, SchemaError> {
        let mut p = P::new(s)?;
        let mut d = Self {
            oid: p.oid()?,
            ..Self::default()
        };
        let mut com = Common::default();
        let mut applies = None;
        while let Some((at, k)) = p.keyword()? {
            if com.take(&mut p, &k)? {
                continue;
            }
            match k.as_str() {
                "APPLIES" => applies = Some(p.oids()?),
                _ => return Err(p.unknown(at, &k)),
            }
        }
        d.applies = applies.ok_or_else(|| p.err_at(0, "a matching rule use needs APPLIES"))?;
        d.names = com.names;
        d.desc = com.desc;
        d.obsolete = com.obsolete;
        d.extensions = com.extensions;
        Ok(d)
    }
}

impl DitContentRuleDesc {
    /// Parses a `DITContentRuleDescription`.
    ///
    /// # Errors
    ///
    /// A [`SchemaError`].
    pub fn parse(s: &str) -> Result<Self, SchemaError> {
        let mut p = P::new(s)?;
        let mut d = Self {
            oid: p.oid()?,
            ..Self::default()
        };
        let mut com = Common::default();
        while let Some((at, k)) = p.keyword()? {
            if com.take(&mut p, &k)? {
                continue;
            }
            match k.as_str() {
                "AUX" => d.aux = p.oids()?,
                "MUST" => d.must = p.oids()?,
                "MAY" => d.may = p.oids()?,
                "NOT" => d.not = p.oids()?,
                _ => return Err(p.unknown(at, &k)),
            }
        }
        d.names = com.names;
        d.desc = com.desc;
        d.obsolete = com.obsolete;
        d.extensions = com.extensions;
        Ok(d)
    }
}

fn rule_id(at: usize, w: &str) -> Result<u32, SchemaError> {
    w.parse::<u32>().map_err(|_| SchemaError {
        at,
        what: format!("bad rule ID {w}"),
    })
}

impl DitStructureRuleDesc {
    /// Parses a `DITStructureRuleDescription`.
    ///
    /// # Errors
    ///
    /// A [`SchemaError`].
    pub fn parse(s: &str) -> Result<Self, SchemaError> {
        let mut p = P::new(s)?;
        let at = p.at();
        let id = p.word()?;
        let mut d = Self {
            rule_id: rule_id(at, &id)?,
            ..Self::default()
        };
        let mut com = Common::default();
        let mut form = None;
        while let Some((at, k)) = p.keyword()? {
            if com.take(&mut p, &k)? {
                continue;
            }
            match k.as_str() {
                "FORM" => form = Some(p.word()?),
                "SUP" => {
                    let at = p.at();
                    d.sup = p
                        .oids()?
                        .iter()
                        .map(|w| rule_id(at, w))
                        .collect::<Result<_, _>>()?;
                }
                _ => return Err(p.unknown(at, &k)),
            }
        }
        d.form = form.ok_or_else(|| p.err_at(0, "a structure rule needs FORM"))?;
        d.names = com.names;
        d.desc = com.desc;
        d.obsolete = com.obsolete;
        d.extensions = com.extensions;
        Ok(d)
    }
}

impl NameFormDesc {
    /// Parses a `NameFormDescription`.
    ///
    /// # Errors
    ///
    /// A [`SchemaError`].
    pub fn parse(s: &str) -> Result<Self, SchemaError> {
        let mut p = P::new(s)?;
        let mut d = Self {
            oid: p.oid()?,
            ..Self::default()
        };
        let mut com = Common::default();
        let mut oc = None;
        let mut must = None;
        while let Some((at, k)) = p.keyword()? {
            if com.take(&mut p, &k)? {
                continue;
            }
            match k.as_str() {
                "OC" => oc = Some(p.word()?),
                "MUST" => must = Some(p.oids()?),
                "MAY" => d.may = p.oids()?,
                _ => return Err(p.unknown(at, &k)),
            }
        }
        d.oc = oc.ok_or_else(|| p.err_at(0, "a name form needs OC"))?;
        d.must = must.ok_or_else(|| p.err_at(0, "a name form needs MUST"))?;
        d.names = com.names;
        d.desc = com.desc;
        d.obsolete = com.obsolete;
        d.extensions = com.extensions;
        Ok(d)
    }
}

impl IbmAttributeType {
    /// Parses an `IBMAttributeTypes` value: the OID, then `DBNAME( 'a'
    /// 'b' )`, `ACCESS-CLASS word`, `LENGTH n` and flag keywords
    /// (`EQUALITY`, `ORDERING`, `SUBSTR`, `APPROX`, `REVERSE`) in any
    /// order. Keywords other than the access class and the length are
    /// kept in [`IbmAttributeType::other`].
    ///
    /// # Errors
    ///
    /// A [`SchemaError`].
    pub fn parse(s: &str) -> Result<Self, SchemaError> {
        let mut p = P::new(s)?;
        let mut d = Self {
            oid: p.oid()?,
            ..Self::default()
        };
        while let Some((_, k)) = p.keyword()? {
            match k.as_str() {
                "ACCESS-CLASS" => d.access_class = Some(p.word()?),
                "LENGTH" => {
                    let at = p.at();
                    let w = p.word()?;
                    d.length = Some(
                        w.parse()
                            .map_err(|_| p.err_at(at, format!("bad length {w}")))?,
                    );
                }
                _ => {
                    let v = if matches!(p.peek(), Some(Tok::Open | Tok::Quoted(_))) {
                        p.qdstrings()?
                    } else {
                        Vec::new()
                    };
                    d.other.push((k, v));
                }
            }
        }
        Ok(d)
    }
}

// Printing.

fn quote(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('\'');
    for c in s.chars() {
        match c {
            '\'' => o.push_str("\\27"),
            '\\' => o.push_str("\\5C"),
            _ => o.push(c),
        }
    }
    o.push('\'');
    o
}

fn put_list(f: &mut fmt::Formatter<'_>, k: &str, v: &[String]) -> fmt::Result {
    write!(f, " {k} (")?;
    for s in v {
        write!(f, " {}", quote(s))?;
    }
    f.write_str(" )")
}

fn put_qdstrings(f: &mut fmt::Formatter<'_>, k: &str, v: &[String]) -> fmt::Result {
    match v {
        [] => Ok(()),
        [one] => write!(f, " {k} {}", quote(one)),
        _ => put_list(f, k, v),
    }
}

fn put_oids(f: &mut fmt::Formatter<'_>, k: &str, v: &[String]) -> fmt::Result {
    match v {
        [] => Ok(()),
        [one] => write!(f, " {k} {one}"),
        _ => write!(f, " {k} ( {} )", v.join(" $ ")),
    }
}

fn put_opt(f: &mut fmt::Formatter<'_>, k: &str, v: Option<&String>) -> fmt::Result {
    v.map_or(Ok(()), |v| write!(f, " {k} {v}"))
}

fn put_head(
    f: &mut fmt::Formatter<'_>,
    oid: &str,
    names: &[String],
    desc: Option<&String>,
    obsolete: bool,
) -> fmt::Result {
    write!(f, "( {oid}")?;
    put_qdstrings(f, "NAME", names)?;
    if let Some(d) = desc {
        write!(f, " DESC {}", quote(d))?;
    }
    if obsolete {
        f.write_str(" OBSOLETE")?;
    }
    Ok(())
}

fn put_tail(f: &mut fmt::Formatter<'_>, ext: &[(String, Vec<String>)]) -> fmt::Result {
    for (k, v) in ext {
        if v.len() == 1 {
            write!(f, " {k} {}", quote(&v[0]))?;
        } else {
            put_list(f, k, v)?;
        }
    }
    f.write_str(" )")
}

impl fmt::Display for AttributeTypeDesc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        put_head(f, &self.oid, &self.names, self.desc.as_ref(), self.obsolete)?;
        put_opt(f, "SUP", self.sup.as_ref())?;
        put_opt(f, "EQUALITY", self.equality.as_ref())?;
        put_opt(f, "ORDERING", self.ordering.as_ref())?;
        put_opt(f, "SUBSTR", self.substr.as_ref())?;
        put_opt(f, "SYNTAX", self.syntax.as_ref())?;
        if self.single_value {
            f.write_str(" SINGLE-VALUE")?;
        }
        if self.collective {
            f.write_str(" COLLECTIVE")?;
        }
        if self.no_user_modification {
            f.write_str(" NO-USER-MODIFICATION")?;
        }
        match self.usage {
            Usage::UserApplications => {}
            Usage::DirectoryOperation => f.write_str(" USAGE directoryOperation")?,
            Usage::DistributedOperation => f.write_str(" USAGE distributedOperation")?,
            Usage::DsaOperation => f.write_str(" USAGE dSAOperation")?,
        }
        put_tail(f, &self.extensions)
    }
}

impl fmt::Display for ObjectClassDesc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        put_head(f, &self.oid, &self.names, self.desc.as_ref(), self.obsolete)?;
        put_oids(f, "SUP", &self.sup)?;
        f.write_str(match self.kind {
            ClassKind::Abstract => " ABSTRACT",
            ClassKind::Structural => " STRUCTURAL",
            ClassKind::Auxiliary => " AUXILIARY",
        })?;
        put_oids(f, "MUST", &self.must)?;
        put_oids(f, "MAY", &self.may)?;
        put_tail(f, &self.extensions)
    }
}

impl fmt::Display for SyntaxDesc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        put_head(f, &self.oid, &[], self.desc.as_ref(), false)?;
        put_tail(f, &self.extensions)
    }
}

impl fmt::Display for MatchingRuleDesc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        put_head(f, &self.oid, &self.names, self.desc.as_ref(), self.obsolete)?;
        write!(f, " SYNTAX {}", self.syntax)?;
        put_tail(f, &self.extensions)
    }
}

impl fmt::Display for MatchingRuleUseDesc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        put_head(f, &self.oid, &self.names, self.desc.as_ref(), self.obsolete)?;
        put_oids(f, "APPLIES", &self.applies)?;
        put_tail(f, &self.extensions)
    }
}

impl fmt::Display for DitContentRuleDesc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        put_head(f, &self.oid, &self.names, self.desc.as_ref(), self.obsolete)?;
        put_oids(f, "AUX", &self.aux)?;
        put_oids(f, "MUST", &self.must)?;
        put_oids(f, "MAY", &self.may)?;
        put_oids(f, "NOT", &self.not)?;
        put_tail(f, &self.extensions)
    }
}

impl fmt::Display for DitStructureRuleDesc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        put_head(
            f,
            &self.rule_id.to_string(),
            &self.names,
            self.desc.as_ref(),
            self.obsolete,
        )?;
        write!(f, " FORM {}", self.form)?;
        match self.sup.as_slice() {
            [] => {}
            [one] => write!(f, " SUP {one}")?,
            v => {
                f.write_str(" SUP (")?;
                for r in v {
                    write!(f, " {r}")?;
                }
                f.write_str(" )")?;
            }
        }
        put_tail(f, &self.extensions)
    }
}

impl fmt::Display for NameFormDesc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        put_head(f, &self.oid, &self.names, self.desc.as_ref(), self.obsolete)?;
        write!(f, " OC {}", self.oc)?;
        put_oids(f, "MUST", &self.must)?;
        put_oids(f, "MAY", &self.may)?;
        put_tail(f, &self.extensions)
    }
}

impl fmt::Display for IbmAttributeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "( {}", self.oid)?;
        for (k, v) in &self.other {
            if v.is_empty() {
                write!(f, " {k}")?;
            } else {
                put_list(f, k, v)?;
            }
        }
        if let Some(a) = &self.access_class {
            write!(f, " ACCESS-CLASS {a}")?;
        }
        if let Some(l) = self.length {
            write!(f, " LENGTH {l}")?;
        }
        f.write_str(" )")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AttributeTypeDesc, ClassKind, DitContentRuleDesc, DitStructureRuleDesc, IbmAttributeType,
        MatchingRuleDesc, MatchingRuleUseDesc, NameFormDesc, ObjectClassDesc, SchemaValue, Usage,
    };

    #[test]
    fn primary_schema_round_trips() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tools/conformance/oracle/dirsrv/schema-primary.txt"
        );
        let text = std::fs::read_to_string(path).unwrap();
        let mut n = 0;
        for line in text
            .lines()
            .filter(|l| !l.starts_with('#') && !l.is_empty())
        {
            let (attr, value) = line.split_once(": ").unwrap();
            let v = SchemaValue::parse(attr, value).unwrap_or_else(|e| panic!("{line}: {e}"));
            assert!(v.attr().eq_ignore_ascii_case(attr));
            let printed = v.to_string();
            let again =
                SchemaValue::parse(attr, &printed).unwrap_or_else(|e| panic!("{printed}: {e}"));
            assert_eq!(again, v, "{line}");
            n += 1;
        }
        assert_eq!(n, 1627 + 428 + 43 + 34);
    }

    #[test]
    #[allow(clippy::many_single_char_names)]
    fn rfc4512_examples() {
        let a = AttributeTypeDesc::parse(
            "( 2.5.4.0 NAME 'objectClass' EQUALITY objectIdentifierMatch \
             SYNTAX 1.3.6.1.4.1.1466.115.121.1.38 )",
        )
        .unwrap();
        assert_eq!(a.names, ["objectClass"]);
        let a = AttributeTypeDesc::parse(
            "( 2.5.18.1 NAME 'createTimestamp' EQUALITY generalizedTimeMatch \
             ORDERING generalizedTimeOrderingMatch SYNTAX 1.3.6.1.4.1.1466.115.121.1.24 \
             SINGLE-VALUE NO-USER-MODIFICATION USAGE directoryOperation )",
        )
        .unwrap();
        assert!(a.single_value && a.no_user_modification);
        assert_eq!(a.usage, Usage::DirectoryOperation);
        let a = AttributeTypeDesc::parse(
            "( 2.5.4.41 NAME 'name' DESC '\\27x\\5C' SYNTAX 1.3.6.1.4.1.1466.115.121.1.15{32768} \
             X-ORIGIN ( 'RFC 4519' 'x' ) )",
        )
        .unwrap();
        assert_eq!(a.desc.as_deref(), Some("'x\\"));
        assert_eq!(
            a.syntax.as_deref(),
            Some("1.3.6.1.4.1.1466.115.121.1.15{32768}")
        );
        assert_eq!(a.extensions[0].1.len(), 2);
        assert_eq!(AttributeTypeDesc::parse(&a.to_string()).unwrap(), a);

        let o = ObjectClassDesc::parse(
            "( 2.5.6.6 NAME 'person' SUP top STRUCTURAL MUST ( sn $ cn ) \
             MAY ( userPassword $ telephoneNumber $ seeAlso $ description ) )",
        )
        .unwrap();
        assert_eq!(o.kind, ClassKind::Structural);
        assert_eq!(o.must, ["sn", "cn"]);
        assert_eq!(
            o.to_string(),
            "( 2.5.6.6 NAME 'person' SUP top STRUCTURAL MUST ( sn $ cn ) \
             MAY ( userPassword $ telephoneNumber $ seeAlso $ description ) )"
        );

        let u = MatchingRuleUseDesc::parse("( 2.5.13.2 APPLIES ( cn $ sn ) )").unwrap();
        assert_eq!(MatchingRuleUseDesc::parse(&u.to_string()).unwrap(), u);
        let c =
            DitContentRuleDesc::parse("( 2.5.6.6 NAME 'personRule' AUX ( a $ b ) NOT c )").unwrap();
        assert_eq!(c.not, ["c"]);
        assert_eq!(DitContentRuleDesc::parse(&c.to_string()).unwrap(), c);
        let s = DitStructureRuleDesc::parse("( 2 NAME 'r' FORM nf SUP ( 1 3 ) )").unwrap();
        assert_eq!(s.sup, [1, 3]);
        assert_eq!(DitStructureRuleDesc::parse(&s.to_string()).unwrap(), s);
        let n =
            NameFormDesc::parse("( 1.2.3 NAME 'nf' OC person MUST cn MAY ( uid $ sn ) )").unwrap();
        assert_eq!(NameFormDesc::parse(&n.to_string()).unwrap(), n);
    }

    #[test]
    fn ibm_attribute_types() {
        let i = IbmAttributeType::parse(
            "( 2.5.4.3 DBNAME( 'cn' 'cn' ) ACCESS-CLASS normal LENGTH 256 \
             EQUALITY ORDERING SUBSTR APPROX )",
        )
        .unwrap();
        assert_eq!(i.access_class.as_deref(), Some("normal"));
        assert_eq!(i.length, Some(256));
        assert_eq!(
            i.other[0],
            (
                "DBNAME".to_string(),
                vec!["cn".to_string(), "cn".to_string()]
            )
        );
        assert_eq!(i.other.len(), 5);
        assert_eq!(IbmAttributeType::parse(&i.to_string()).unwrap(), i);
    }

    #[test]
    fn errors() {
        for bad in [
            "",
            "2.5.4.3 NAME 'x' )",
            "( 2.5.4.3 NAME 'x'",
            "( 2.5.4.3 NAME 'x ) ",
            "( 2.5.4.3 DESC 'a' 'b' )",
            "( 2.5.4.3 FROB )",
            "( 2.5.4.3 USAGE nobody )",
            "( 2.5.4.3 MAY ( ) )",
            "( 2.5.4.3 ) x",
            "( 'quoted' )",
        ] {
            assert!(AttributeTypeDesc::parse(bad).is_err(), "{bad}");
        }
        assert!(MatchingRuleDesc::parse("( 1.2 NAME 'x' )").is_err());
        assert!(NameFormDesc::parse("( 1.2 OC x )").is_err());
        assert!(SchemaValue::parse("cn", "( 1 )").is_err());
    }
}
