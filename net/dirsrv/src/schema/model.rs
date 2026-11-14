// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Attribute types and object classes as the registry holds them: the
//! definition as written and what it resolves to (inherited syntax and
//! rules, superclass closure, MUST and MAY collected).

use ldap::schema::{AttributeTypeDesc, ClassKind, IbmAttributeType, ObjectClassDesc, Usage};

use super::rule::Rule;

/// IBM's attribute access classes, which ACLs grant by (§12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum AccessClass {
    /// Most user attributes.
    #[default]
    Normal,
    /// Personal data a site may want to guard.
    Sensitive,
    /// Credentials (`userPassword`).
    Critical,
    /// Operational attributes the server keeps.
    System,
    /// The access control attributes.
    Restricted,
}

impl AccessClass {
    /// The class's keyword, as `ACCESS-CLASS` and `aclEntry` write it.
    #[must_use]
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Sensitive => "sensitive",
            Self::Critical => "critical",
            Self::System => "system",
            Self::Restricted => "restricted",
        }
    }

    /// The class a keyword names, ignoring case.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        [
            Self::Normal,
            Self::Sensitive,
            Self::Critical,
            Self::System,
            Self::Restricted,
        ]
        .into_iter()
        .find(|c| c.keyword().eq_ignore_ascii_case(s))
    }
}

/// An attribute type, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributeType {
    /// The definition as written.
    pub def: AttributeTypeDesc,
    /// IBM's extension values, if any were given.
    pub ibm: Option<IbmAttributeType>,
    /// The supertype's OID.
    pub sup: Option<String>,
    /// The syntax OID, own or inherited.
    pub syntax: Option<String>,
    /// The syntax's length bound (`{n}`), own or inherited.
    pub syntax_len: Option<u32>,
    /// The equality rule, own or inherited.
    pub equality: Option<&'static Rule>,
    /// The ordering rule, own or inherited.
    pub ordering: Option<&'static Rule>,
    /// The substring rule, own or inherited.
    pub substr: Option<&'static Rule>,
    /// The access class.
    pub access: AccessClass,
}

impl AttributeType {
    /// The OID.
    #[must_use]
    pub fn oid(&self) -> &str {
        &self.def.oid
    }

    /// The primary name, or the OID when it has none.
    #[must_use]
    pub fn name(&self) -> &str {
        self.def.names.first().map_or(&self.def.oid, String::as_str)
    }

    /// Whether `n` is one of its names or its OID (case ignored).
    #[must_use]
    pub fn is_named(&self, n: &str) -> bool {
        self.def.oid == n || self.def.names.iter().any(|x| x.eq_ignore_ascii_case(n))
    }

    /// An operational attribute (any usage but `userApplications`).
    #[must_use]
    pub fn is_operational(&self) -> bool {
        self.def.usage != Usage::UserApplications
    }

    /// Single-valued.
    #[must_use]
    pub fn single_value(&self) -> bool {
        self.def.single_value
    }

    /// Not changeable by users.
    #[must_use]
    pub fn no_user_modification(&self) -> bool {
        self.def.no_user_modification
    }

    /// The maximum value length: IBM's `LENGTH`, else the syntax bound.
    #[must_use]
    pub fn max_length(&self) -> Option<u32> {
        self.ibm.as_ref().and_then(|i| i.length).or(self.syntax_len)
    }
}

/// An object class, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectClass {
    /// The definition as written.
    pub def: ObjectClassDesc,
    /// Every superclass's OID, nearest first, without this class.
    pub supers: Vec<String>,
    /// Every required attribute's OID, this class's and its
    /// superclasses'.
    pub must: Vec<String>,
    /// Every allowed attribute's OID that is not required.
    pub may: Vec<String>,
}

impl ObjectClass {
    /// The OID.
    #[must_use]
    pub fn oid(&self) -> &str {
        &self.def.oid
    }

    /// The primary name, or the OID.
    #[must_use]
    pub fn name(&self) -> &str {
        self.def.names.first().map_or(&self.def.oid, String::as_str)
    }

    /// The kind.
    #[must_use]
    pub fn kind(&self) -> ClassKind {
        self.def.kind
    }

    /// Whether `oid` is this class or one of its superclasses.
    #[must_use]
    pub fn is_a(&self, oid: &str) -> bool {
        self.def.oid == oid || self.supers.iter().any(|s| s == oid)
    }
}
