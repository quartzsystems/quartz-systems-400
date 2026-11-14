// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Distinguished names (RFC 4514). Parsing and printing are this
//! module's; normalization against matching rules is `dirsrv`'s.

/// An attribute value assertion in an RDN: `type=value`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Ava {
    /// The attribute type as written: a name or a dotted OID.
    pub ty: String,
    /// The value's bytes (escapes and `#hex` decoded).
    pub value: Vec<u8>,
}

/// A relative distinguished name: one or more AVAs (`a=1+b=2`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Rdn(pub Vec<Ava>);

/// A distinguished name, most specific RDN first, as written. The empty
/// DN (the root DSE) has no RDNs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Dn(pub Vec<Rdn>);

impl Dn {
    /// The root DSE's name.
    #[must_use]
    pub const fn root() -> Self {
        Self(Vec::new())
    }

    /// Whether this is the empty DN.
    #[must_use]
    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }
}
