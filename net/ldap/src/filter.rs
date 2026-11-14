// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Search filters (RFC 4511 §4.5.1.7, strings RFC 4515).

/// A search filter. Attribute descriptions are kept as written (type
/// and options); values as bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Filter {
    /// `(&...)`; empty is absolute true (RFC 4526).
    And(Vec<Filter>),
    /// `(|...)`; empty is absolute false.
    Or(Vec<Filter>),
    /// `(!...)`.
    Not(Box<Filter>),
    /// `(attr=value)`.
    Equality(String, Vec<u8>),
    /// `(attr=ini*any*fin)`.
    Substrings(String, Substrings),
    /// `(attr>=value)`.
    GreaterOrEqual(String, Vec<u8>),
    /// `(attr<=value)`.
    LessOrEqual(String, Vec<u8>),
    /// `(attr=*)`.
    Present(String),
    /// `(attr~=value)`.
    Approx(String, Vec<u8>),
    /// `(attr:dn:rule:=value)`.
    Extensible(Extensible),
}

/// The parts of a substring assertion.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Substrings {
    /// The initial part, if any.
    pub initial: Option<Vec<u8>>,
    /// The middle parts, in order.
    pub any: Vec<Vec<u8>>,
    /// The final part, if any.
    pub fin: Option<Vec<u8>>,
}

/// An extensible match assertion.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Extensible {
    /// The matching rule, a name or OID.
    pub rule: Option<String>,
    /// The attribute description.
    pub attr: Option<String>,
    /// The assertion value.
    pub value: Vec<u8>,
    /// Whether the DN's attributes take part (`:dn`).
    pub dn_attributes: bool,
}
