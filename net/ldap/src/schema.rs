// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Schema descriptions (RFC 4512 §4.1) and IBM's `IBMAttributeTypes`
//! extension values, as parsed from `cn=schema` or a schema file. The
//! model that checks entries is `dirsrv::schema`'s.

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
