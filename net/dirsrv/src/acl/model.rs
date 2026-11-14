// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! IBM's non-filtered ACL values (docs/DIRSRV.md §12): `entryOwner` and
//! `aclEntry`, parsed and printed.
//!
//! ```text
//! aclEntry:   [access-id:|group:|role:]DN[:target:[grant:|deny:]perms]...
//! target:     object | normal | sensitive | critical | system | restricted | at.NAME
//! perms:      object: a (add children), d (delete); attributes: r w s c
//! entryOwner: [access-id:|group:|role:]DN
//! ```

use std::fmt;

use ldap::dn::Dn;

use crate::schema::{AccessClass, Schema};

/// Read.
pub const R: u8 = 1;
/// Write.
pub const W: u8 = 2;
/// Search.
pub const S: u8 = 4;
/// Compare.
pub const C: u8 = 8;
/// Object right: add children.
pub const A: u8 = 1;
/// Object right: delete.
pub const D: u8 = 2;

/// What a subject DN names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// One identity.
    AccessId,
    /// A group's members.
    Group,
    /// A role's members.
    Role,
}

impl Kind {
    fn prefix(self) -> &'static str {
        match self {
            Self::AccessId => "access-id",
            Self::Group => "group",
            Self::Role => "role",
        }
    }
}

/// IBM's pseudo DNs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pseudo {
    /// `cn=anybody`: everyone, bound or not.
    Anybody,
    /// `cn=authenticated`: every bound identity.
    Authenticated,
    /// `cn=this`: the entry itself, bound as it.
    This,
}

/// A subject of an ACL or an owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subject {
    /// What it names.
    pub kind: Kind,
    /// The DN as written.
    pub dn: String,
    /// The DN normalized (distinguishedNameMatch).
    pub key: Vec<u8>,
    /// A pseudo DN.
    pub pseudo: Option<Pseudo>,
}

/// What a right applies to. Attribute targets compare by OID.
#[derive(Debug, Clone, Eq)]
pub enum Target {
    /// The entry itself (`object`).
    Object,
    /// Every attribute of an access class.
    Class(AccessClass),
    /// One attribute (`at.NAME`): its OID and the name as written.
    Attr(String, String),
}

impl PartialEq for Target {
    fn eq(&self, o: &Self) -> bool {
        match (self, o) {
            (Self::Object, Self::Object) => true,
            (Self::Class(a), Self::Class(b)) => a == b,
            (Self::Attr(a, _), Self::Attr(b, _)) => a == b,
            _ => false,
        }
    }
}

/// Rights granted and denied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Grant {
    /// Granted bits.
    pub allow: u8,
    /// Denied bits.
    pub deny: u8,
}

/// One `aclEntry` value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AclEntry {
    /// Who.
    pub subject: Subject,
    /// What, in the order written (merged per target).
    pub rights: Vec<(Target, Grant)>,
}

/// Why a value does not parse; the operation answers 21.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct AclError(pub String);

fn err(s: impl Into<String>) -> AclError {
    AclError(s.into())
}

fn pseudo_of(dn: &Dn) -> Option<Pseudo> {
    let [rdn] = dn.0.as_slice() else { return None };
    let [ava] = rdn.0.as_slice() else { return None };
    if !(ava.ty.eq_ignore_ascii_case("cn") || ava.ty == "2.5.4.3") {
        return None;
    }
    let v = ava.value_str()?;
    if v.eq_ignore_ascii_case("anybody") {
        Some(Pseudo::Anybody)
    } else if v.eq_ignore_ascii_case("authenticated") {
        Some(Pseudo::Authenticated)
    } else if v.eq_ignore_ascii_case("this") {
        Some(Pseudo::This)
    } else {
        None
    }
}

fn split_kind(s: &str) -> (Kind, &str) {
    for k in [Kind::AccessId, Kind::Group, Kind::Role] {
        let p = k.prefix();
        if s.len() > p.len() && s[..p.len()].eq_ignore_ascii_case(p) && s.as_bytes()[p.len()] == b':'
        {
            return (k, &s[p.len() + 1..]);
        }
    }
    (Kind::AccessId, s)
}

fn subject(schema: &Schema, kind: Kind, dn: &str) -> Result<Subject, AclError> {
    let dn = dn.trim();
    let parsed = Dn::parse(dn).map_err(|_| err(format!("{dn:?} is not a DN")))?;
    if parsed.is_root() {
        return Err(err("an ACL subject needs a DN"));
    }
    let pseudo = pseudo_of(&parsed);
    Ok(Subject {
        kind,
        dn: dn.to_string(),
        key: schema.normalize_parsed_dn(&parsed),
        pseudo,
    })
}

/// An `entryOwner` value.
///
/// # Errors
/// A value that is not `[kind:]DN`.
pub fn parse_owner(schema: &Schema, s: &str) -> Result<Subject, AclError> {
    let (kind, dn) = split_kind(s.trim());
    subject(schema, kind, dn)
}

fn target_of(schema: &Schema, t: &str) -> Option<Target> {
    if t.eq_ignore_ascii_case("object") {
        return Some(Target::Object);
    }
    if let Some(c) = AccessClass::parse(t) {
        return Some(Target::Class(c));
    }
    let name = t.strip_prefix("at.").or_else(|| t.strip_prefix("AT."))?;
    let at = schema.attribute(name)?;
    Some(Target::Attr(at.oid().to_string(), name.to_string()))
}

fn is_target_word(t: &str) -> bool {
    t.eq_ignore_ascii_case("object")
        || AccessClass::parse(t).is_some()
        || t.len() > 3 && t[..3].eq_ignore_ascii_case("at.")
}

/// An `aclEntry` value.
///
/// # Errors
/// A malformed value, an unknown attribute, a permission letter that
/// does not belong to its target.
pub fn parse_acl(schema: &Schema, s: &str) -> Result<AclEntry, AclError> {
    let (kind, rest) = split_kind(s.trim());
    // The DN ends at the first `:` followed by a target word.
    let parts: Vec<&str> = rest.split(':').collect();
    let at = (1..parts.len())
        .find(|&i| is_target_word(parts[i].trim()))
        .unwrap_or(parts.len());
    let dn = parts[..at].join(":");
    let subject = subject(schema, kind, &dn)?;
    let mut rights: Vec<(Target, Grant)> = Vec::new();
    let mut i = at;
    while i < parts.len() {
        let word = parts[i].trim();
        let target = target_of(schema, word)
            .ok_or_else(|| err(format!("{word:?} is not an ACL target")))?;
        i += 1;
        let mut deny = false;
        if let Some(p) = parts.get(i).map(|p| p.trim()) {
            if p.eq_ignore_ascii_case("grant") {
                i += 1;
            } else if p.eq_ignore_ascii_case("deny") {
                deny = true;
                i += 1;
            }
        }
        let perms = parts.get(i).map_or("", |p| p.trim());
        let bits = if is_target_word(perms) {
            0
        } else {
            i += 1;
            letters(&target, perms)?
        };
        let slot = if let Some(k) = rights.iter().position(|(t, _)| *t == target) {
            k
        } else {
            rights.push((target, Grant::default()));
            rights.len() - 1
        };
        if deny {
            rights[slot].1.deny |= bits;
        } else {
            rights[slot].1.allow |= bits;
        }
    }
    Ok(AclEntry { subject, rights })
}

fn letters(t: &Target, perms: &str) -> Result<u8, AclError> {
    let mut bits = 0;
    for c in perms.chars() {
        bits |= match (t, c.to_ascii_lowercase()) {
            (Target::Object, 'a') => A,
            (Target::Object, 'd') => D,
            (Target::Class(_) | Target::Attr(..), 'r') => R,
            (Target::Class(_) | Target::Attr(..), 'w') => W,
            (Target::Class(_) | Target::Attr(..), 's') => S,
            (Target::Class(_) | Target::Attr(..), 'c') => C,
            _ => return Err(err(format!("permission {c:?} does not apply here"))),
        };
    }
    Ok(bits)
}

fn perm_text(t: &Target, bits: u8) -> String {
    let set: &[(u8, char)] = if *t == Target::Object {
        &[(A, 'a'), (D, 'd')]
    } else {
        &[(R, 'r'), (W, 'w'), (S, 's'), (C, 'c')]
    };
    set.iter().filter(|(b, _)| bits & b != 0).map(|(_, c)| *c).collect()
}

fn target_text(t: &Target) -> String {
    match t {
        Target::Object => "object".into(),
        Target::Class(c) => c.keyword().into(),
        Target::Attr(_, n) => format!("at.{n}"),
    }
}

/// The order targets print in: object, the classes, then attributes.
fn rank(t: &Target) -> (u8, String) {
    match t {
        Target::Object => (0, String::new()),
        Target::Class(c) => (
            match c {
                AccessClass::Restricted => 1,
                AccessClass::System => 2,
                AccessClass::Critical => 3,
                AccessClass::Sensitive => 4,
                AccessClass::Normal => 5,
            },
            String::new(),
        ),
        Target::Attr(oid, _) => (6, oid.clone()),
    }
}

impl fmt::Display for Subject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.kind.prefix(), self.dn)
    }
}

impl fmt::Display for AclEntry {
    /// The canonical form `ibm-effectiveAcl` shows.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.subject)?;
        let mut rs: Vec<&(Target, Grant)> = self.rights.iter().collect();
        rs.sort_by_key(|(t, _)| rank(t));
        for (t, g) in rs {
            let tt = target_text(t);
            if g.allow != 0 || g.deny == 0 {
                write!(f, ":{tt}:{}", perm_text(t, g.allow))?;
            }
            if g.deny != 0 {
                write!(f, ":{tt}:deny:{}", perm_text(t, g.deny))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ibm_forms() {
        let s = Schema::shipped().unwrap();
        let a = parse_acl(&s, "group:CN=ANYBODY:normal:rsc:system:rsc:restricted:rsc").unwrap();
        assert_eq!(a.subject.kind, Kind::Group);
        assert_eq!(a.subject.pseudo, Some(Pseudo::Anybody));
        assert_eq!(
            a.to_string(),
            "group:CN=ANYBODY:restricted:rsc:system:rsc:normal:rsc"
        );
        let b = parse_acl(
            &s,
            "cn=Joe,o=x:object:ad:normal:grant:rwsc:at.userPassword:deny:r:normal:deny:w",
        )
        .unwrap();
        assert_eq!(b.subject.kind, Kind::AccessId);
        assert_eq!(b.subject.dn, "cn=Joe,o=x");
        assert_eq!(
            b.to_string(),
            "access-id:cn=Joe,o=x:object:ad:normal:rwsc:normal:deny:w:at.userPassword:deny:r"
        );
        assert!(parse_acl(&s, "access-id:cn=a:object:r").is_err());
        assert!(parse_acl(&s, "access-id:cn=a:at.nosuchattr:r").is_err());
        assert!(parse_acl(&s, "not a dn").is_err());
        let o = parse_owner(&s, "role:cn=admins,o=x").unwrap();
        assert_eq!(o.kind, Kind::Role);
        assert_eq!(o.to_string(), "role:cn=admins,o=x");
    }
}
