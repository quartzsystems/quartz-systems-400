// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `PKGNAMCSN`: the package (RDB, collection, ID), its consistency token
//! and a section number, naming the statement a command is about. The
//! fixed form has 18-byte names; the extended form gives each name a
//! two-byte length.

use crate::Error;
use crate::cp::param;
use crate::wire::{Object, ebcdic, ebcdic_fixed, text};

/// The width of names in the fixed form.
pub const NAME_LEN: usize = 18;

/// The fixed form's fields: offset, size, name.
pub const LAYOUT: &[(usize, usize, &str)] = &[
    (0, 18, "rdbnam"),
    (18, 18, "rdbcolid"),
    (36, 18, "pkgid"),
    (54, 8, "pkgcnstkn"),
    (62, 2, "pkgsn"),
];

/// A package, token and section.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Pkgnamcsn {
    /// `RDBNAM`.
    pub rdb: String,
    /// `RDBCOLID`: the collection.
    pub collection: String,
    /// `PKGID`.
    pub package: String,
    /// `PKGCNSTKN`.
    pub token: [u8; 8],
    /// `PKGSN`.
    pub section: u16,
}

impl Pkgnamcsn {
    /// The dynamic statement package QS/400 requesters use.
    pub fn dynamic(rdb: &str, section: u16) -> Self {
        Self {
            rdb: rdb.trim().to_ascii_uppercase(),
            collection: "NULLID".into(),
            package: "SYSSH200".into(),
            token: *b"SYSLVL01",
            section,
        }
    }

    /// The same package at another section.
    #[must_use]
    pub fn at(&self, section: u16) -> Self {
        Self {
            section,
            ..self.clone()
        }
    }

    /// The parameter.
    pub fn to_object(&self) -> Object {
        let fixed = [&self.rdb, &self.collection, &self.package]
            .iter()
            .all(|n| n.len() <= NAME_LEN);
        let mut d = Vec::with_capacity(64);
        for n in [&self.rdb, &self.collection, &self.package] {
            if fixed {
                d.extend(ebcdic_fixed(n, NAME_LEN));
            } else {
                let e = ebcdic(n);
                d.extend(u16::try_from(e.len()).unwrap_or(0).to_be_bytes());
                d.extend(e);
            }
        }
        d.extend(self.token);
        d.extend(self.section.to_be_bytes());
        Object::data(param::PKGNAMCSN, d)
    }

    /// `PKGNAMCT`: the package and token without the section.
    pub fn to_pkgnamct(&self) -> Object {
        let mut b = self.to_object().bytes().to_vec();
        b.truncate(b.len() - 2);
        Object::data(param::PKGNAMCT, b)
    }

    /// From the parameter's data.
    pub fn decode(b: &[u8]) -> Result<Self, Error> {
        let short = || Error::Short {
            what: "PKGNAMCSN",
            need: 64,
            have: b.len(),
        };
        let (names, rest) = if b.len() == 64 {
            let n: Vec<String> = (0..3)
                .map(|i| text(&b[i * NAME_LEN..(i + 1) * NAME_LEN]))
                .collect();
            (n, &b[54..])
        } else {
            let mut at = 0;
            let mut n = Vec::new();
            for _ in 0..3 {
                if b.len() < at + 2 {
                    return Err(short());
                }
                let l = usize::from(crate::be16(b, at));
                if b.len() < at + 2 + l {
                    return Err(short());
                }
                n.push(text(&b[at + 2..at + 2 + l]));
                at += 2 + l;
            }
            (n, &b[at..])
        };
        if rest.len() < 10 {
            return Err(short());
        }
        let mut token = [0u8; 8];
        token.copy_from_slice(&rest[..8]);
        Ok(Self {
            rdb: names[0].clone(),
            collection: names[1].clone(),
            package: names[2].clone(),
            token,
            section: crate::be16(rest, 8),
        })
    }

    /// From a command's `PKGNAMCSN`.
    pub fn from_command(o: &Object) -> Result<Self, Error> {
        Self::decode(o.get(param::PKGNAMCSN).ok_or(Error::Missing("PKGNAMCSN"))?.bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_forms() {
        let p = Pkgnamcsn::dynamic("QS400", 3);
        let o = p.to_object();
        assert_eq!(o.bytes().len(), 64);
        assert_eq!(Pkgnamcsn::decode(o.bytes()).unwrap(), p);
        let mut long = p.clone();
        long.collection = "A_VERY_LONG_COLLECTION_NAME".into();
        let o = long.to_object();
        assert_eq!(Pkgnamcsn::decode(o.bytes()).unwrap(), long);
    }
}
