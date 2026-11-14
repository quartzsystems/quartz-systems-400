// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! ACL evaluation (docs/DIRSRV.md §12).
//!
//! An entry's ACL and owners are its own, or inherited from the nearest
//! ancestor whose values propagate, or the defaults. The administrator
//! and the owners have every right. Otherwise each right is decided by
//! level: the requester's own DN first (`access-id`, and `cn=this` on its
//! own entry), then its groups and roles (`cn=authenticated`,
//! `cn=anybody` included). Within a level an `at.NAME` right counts
//! before its access class, and a deny before a grant. The first level
//! and target that says anything decides; nothing said is no right.

use crate::schema::AccessClass;

use super::model::{AclEntry, Kind, Pseudo, Subject, Target};

/// Who asks, normalized.
#[derive(Debug, Clone, Copy)]
pub struct Principal<'a> {
    /// The bound DN, normalized; `None` for anonymous.
    pub dn: Option<&'a [u8]>,
    /// The groups and roles it is a member of, normalized.
    pub groups: &'a [Vec<u8>],
    /// The directory administrator (or an administrative profile).
    pub admin: bool,
}

/// The ACL and owners in force on one entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Effective {
    /// The ACL values.
    pub acl: Vec<AclEntry>,
    /// The entry they come from (as written), `None` for the default.
    pub acl_source: Option<String>,
    /// Whether they propagate.
    pub acl_propagate: bool,
    /// The owners.
    pub owners: Vec<Subject>,
    /// The entry they come from, `None` for the default.
    pub owner_source: Option<String>,
    /// Whether they propagate.
    pub owner_propagate: bool,
}

fn matches(s: &Subject, p: &Principal<'_>, entry: &[u8]) -> Option<u8> {
    // The level: 1 for the requester itself, 2 for its groups.
    match (s.kind, s.pseudo) {
        (_, Some(Pseudo::Anybody)) => Some(2),
        (_, Some(Pseudo::Authenticated)) => p.dn.is_some().then_some(2),
        (_, Some(Pseudo::This)) => (p.dn == Some(entry)).then_some(1),
        (Kind::AccessId, None) => (p.dn == Some(s.key.as_slice())).then_some(1),
        (Kind::Group | Kind::Role, None) => p.groups.iter().any(|g| *g == s.key).then_some(2),
    }
}

impl Effective {
    /// Whether the requester owns the entry (or is the administrator).
    #[must_use]
    pub fn is_owner(&self, p: &Principal<'_>, entry: &[u8]) -> bool {
        p.admin || self.owners.iter().any(|o| matches(o, p, entry).is_some())
    }

    fn decide(&self, p: &Principal<'_>, entry: &[u8], targets: &[Target], bit: u8) -> bool {
        for level in [1u8, 2] {
            let mine: Vec<&AclEntry> = self
                .acl
                .iter()
                .filter(|a| matches(&a.subject, p, entry) == Some(level))
                .collect();
            for t in targets {
                let mut deny = false;
                let mut allow = false;
                for a in &mine {
                    for (rt, g) in &a.rights {
                        if rt == t {
                            deny |= g.deny & bit != 0;
                            allow |= g.allow & bit != 0;
                        }
                    }
                }
                if deny {
                    return false;
                }
                if allow {
                    return true;
                }
            }
        }
        false
    }

    /// The rights (`model::R | W | S | C`) on one attribute of the entry
    /// (`oid` and its access class).
    #[must_use]
    pub fn attr_rights(&self, p: &Principal<'_>, entry: &[u8], oid: &str, class: AccessClass) -> u8 {
        if self.is_owner(p, entry) {
            return 0x0f;
        }
        let targets = [Target::Attr(oid.to_string(), String::new()), Target::Class(class)];
        let mut out = 0;
        for bit in [super::R, super::W, super::S, super::C] {
            if self.decide(p, entry, &targets, bit) {
                out |= bit;
            }
        }
        out
    }

    /// The object rights (`model::A | D`).
    #[must_use]
    pub fn object_rights(&self, p: &Principal<'_>, entry: &[u8]) -> u8 {
        if self.is_owner(p, entry) {
            return super::A | super::D;
        }
        let mut out = 0;
        for bit in [super::A, super::D] {
            if self.decide(p, entry, &[Target::Object], bit) {
                out |= bit;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::super::model::{parse_acl, parse_owner};
    use super::super::{A, C, D, R, S, W};
    use super::*;
    use crate::schema::Schema;
    use ldap::Dn;

    struct World {
        schema: Schema,
    }

    impl World {
        fn key(&self, dn: &str) -> Vec<u8> {
            self.schema.normalize_parsed_dn(&Dn::parse(dn).unwrap())
        }
        fn eff(&self, acl: &[&str], owners: &[&str]) -> Effective {
            Effective {
                acl: acl.iter().map(|a| parse_acl(&self.schema, a).unwrap()).collect(),
                acl_source: None,
                acl_propagate: true,
                owners: owners
                    .iter()
                    .map(|o| parse_owner(&self.schema, o).unwrap())
                    .collect(),
                owner_source: None,
                owner_propagate: true,
            }
        }
    }

    /// The matrix: each requester against each rule set, every right.
    #[test]
    fn matrix() {
        let w = World {
            schema: Schema::shipped().unwrap(),
        };
        let entry = w.key("cn=Joe,o=Acme");
        let joe = w.key("cn=Joe,o=Acme");
        let ann = w.key("cn=Ann,o=Acme");
        let staff = vec![w.key("cn=Staff,o=Acme")];
        let none: Vec<Vec<u8>> = Vec::new();
        let anon = Principal { dn: None, groups: &none, admin: false };
        let p_joe = Principal { dn: Some(&joe), groups: &none, admin: false };
        let p_ann = Principal { dn: Some(&ann), groups: &staff, admin: false };
        let p_adm = Principal { dn: Some(&ann), groups: &none, admin: true };
        let n = AccessClass::Normal;
        let cr = AccessClass::Critical;
        let cn = "2.5.4.3";
        let pw = "2.5.4.35";

        // The default ACL: anybody reads, searches, compares normal.
        let d = w.eff(&["group:CN=ANYBODY:normal:rsc:system:rsc:restricted:rsc"], &["access-id:cn=Administrator"]);
        for p in [&anon, &p_joe, &p_ann] {
            assert_eq!(d.attr_rights(p, &entry, cn, n), R | S | C);
            assert_eq!(d.attr_rights(p, &entry, pw, cr), 0);
            assert_eq!(d.object_rights(p, &entry), 0);
        }
        assert_eq!(d.attr_rights(&p_adm, &entry, pw, cr), R | W | S | C);
        assert_eq!(d.object_rights(&p_adm, &entry), A | D);

        // cn=this writes its own entry; authenticated reads critical.
        let t = w.eff(
            &[
                "access-id:cn=this:normal:rwsc:critical:w",
                "group:cn=authenticated:critical:c",
            ],
            &[],
        );
        assert_eq!(t.attr_rights(&p_joe, &entry, cn, n), R | W | S | C);
        assert_eq!(t.attr_rights(&p_joe, &entry, pw, cr), W | C);
        assert_eq!(t.attr_rights(&p_ann, &entry, pw, cr), C);
        assert_eq!(t.attr_rights(&anon, &entry, pw, cr), 0);

        // Deny wins at a level; access-id beats group; at. beats class.
        let x = w.eff(
            &[
                "group:cn=Staff,o=Acme:normal:rwsc:object:a",
                "group:cn=anybody:normal:deny:w",
                "access-id:cn=Ann,o=Acme:at.cn:deny:r",
                "access-id:cn=Ann,o=Acme:normal:r",
            ],
            &[],
        );
        assert_eq!(x.attr_rights(&p_ann, &entry, cn, n), S | C);
        assert_eq!(x.attr_rights(&p_ann, &entry, "2.5.4.4", n), R | S | C);
        assert_eq!(x.object_rights(&p_ann, &entry), A);
        assert_eq!(x.attr_rights(&p_joe, &entry, cn, n), 0);

        // Owners have everything, groups too.
        let o = w.eff(&[], &["group:cn=Staff,o=Acme"]);
        assert_eq!(o.attr_rights(&p_ann, &entry, pw, cr), R | W | S | C);
        assert_eq!(o.object_rights(&p_ann, &entry), A | D);
        assert_eq!(o.object_rights(&p_joe, &entry), 0);
        assert!(!o.is_owner(&anon, &entry));
    }
}
