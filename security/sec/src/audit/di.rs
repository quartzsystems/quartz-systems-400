// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DI` entries: the directory server's operations (docs/DIRSRV.md §12,
//! docs/SECURITY.md §7). The service calls [`audit_di`]; the entry's
//! operation code (`DIOPER`) picks the action auditing value that
//! selects it ([`Op::category`]).

use crate::audit::{Event, Kind, TAIL_PATH, record};

/// Tail tag of the bound user's DN (`DIUSRN`).
pub const TAIL_USER_DN: u8 = 3;
/// Tail tag of the object's DN (`DIOBJN`).
pub const TAIL_OBJECT_DN: u8 = 4;
/// Tail tag of the old owner or old value (`DIOOWN`).
pub const TAIL_OLD: u8 = 5;
/// Tail tag of the new owner or new DN (`DINEWN`).
pub const TAIL_NEW: u8 = 6;

/// The variable fields kept in the tail: the outfile's name, the tag
/// and IBM's maximum length (`QASYDIJ4`).
pub const TAIL_FIELDS: &[(&str, u8, u32)] = &[
    ("DIUSRN", TAIL_USER_DN, 2000),
    ("DIOBJN", TAIL_OBJECT_DN, 2000),
    ("DIOOWN", TAIL_OLD, 2000),
    ("DINEWN", TAIL_NEW, 2000),
    ("DIPNAM", TAIL_PATH, 5000),
];

/// A directory operation: IBM's `DIOPER` codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Op {
    /// `AD`: the audit configuration changed.
    AuditChange,
    /// `AF`: an operation refused by access control.
    AuthFail,
    /// `BN`: a bind that succeeded.
    Bind,
    /// `CA`: an ACL changed.
    AclChange,
    /// `CF`: the configuration changed.
    ConfigChange,
    /// `CI`: an instance created.
    CreateInstance,
    /// `CO`: an entry added.
    Create,
    /// `CP`: a password changed.
    PasswordChange,
    /// `DI`: an instance deleted.
    DeleteInstance,
    /// `DO`: an entry deleted.
    Delete,
    /// `EX`: the directory exported (LDIF).
    Export,
    /// `IM`: the directory imported (LDIF).
    Import,
    /// `OM`: an entry renamed or moved (modify DN).
    Rename,
    /// `OW`: an entry's owner changed.
    OwnerChange,
    /// `PO`: a password policy changed.
    PolicyChange,
    /// `PW`: a bind with a password not valid.
    PasswordFail,
    /// `RM`: replication management.
    Replication,
    /// `UB`: an unbind.
    Unbind,
    /// `ZC`: an entry modified.
    Change,
    /// `ZR`: an entry read (search, compare).
    Read,
}

/// Every operation, in code order.
pub const OPS: [Op; 20] = [
    Op::AuditChange,
    Op::AuthFail,
    Op::Bind,
    Op::AclChange,
    Op::ConfigChange,
    Op::CreateInstance,
    Op::Create,
    Op::PasswordChange,
    Op::DeleteInstance,
    Op::Delete,
    Op::Export,
    Op::Import,
    Op::Rename,
    Op::OwnerChange,
    Op::PolicyChange,
    Op::PasswordFail,
    Op::Replication,
    Op::Unbind,
    Op::Change,
    Op::Read,
];

impl Op {
    /// The two letters of `DIOPER`.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::AuditChange => "AD",
            Self::AuthFail => "AF",
            Self::Bind => "BN",
            Self::AclChange => "CA",
            Self::ConfigChange => "CF",
            Self::CreateInstance => "CI",
            Self::Create => "CO",
            Self::PasswordChange => "CP",
            Self::DeleteInstance => "DI",
            Self::Delete => "DO",
            Self::Export => "EX",
            Self::Import => "IM",
            Self::Rename => "OM",
            Self::OwnerChange => "OW",
            Self::PolicyChange => "PO",
            Self::PasswordFail => "PW",
            Self::Replication => "RM",
            Self::Unbind => "UB",
            Self::Change => "ZC",
            Self::Read => "ZR",
        }
    }

    /// By its two letters.
    #[must_use]
    pub fn by_code(c: &str) -> Option<Self> {
        OPS.into_iter()
            .find(|o| o.code().eq_ignore_ascii_case(c.trim()))
    }

    /// The action auditing value that selects it, as IBM's; `None` for
    /// `ZC` and `ZR`, which `QAUDCTL(*OBJAUD)` selects.
    #[must_use]
    pub fn category(self) -> Option<&'static str> {
        Some(match self {
            Self::AuthFail | Self::PasswordFail => "*AUTFAIL",
            Self::Create => "*CREATE",
            Self::Delete => "*DELETE",
            Self::Rename => "*OBJMGT",
            Self::Change | Self::Read => return None,
            _ => "*SECDIRSRV",
        })
    }
}

/// The category of a `DI` event, by its `DIOPER`; an operation not
/// known is `*SECDIRSRV`'s.
#[must_use]
pub fn category(oper: &str) -> Option<&'static str> {
    Op::by_code(oper).map_or(Some("*SECDIRSRV"), Op::category)
}

/// What the service knows of the operation. Empty fields are written
/// blank.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiFields {
    /// The instance (`DIINST`).
    pub instance: String,
    /// The connection's ID (`DICONN`).
    pub connection: String,
    /// The client's IP address (`DIIPAD`).
    pub client: String,
    /// The LDAP version of the bind (`DILVRS`, `V3`).
    pub ldap_version: String,
    /// The connection is TLS (`DISSLI` `Y`).
    pub tls: bool,
    /// The bind's authentication choice (`DIBNDC`: `SIMPLE`,
    /// `EXTERNAL`).
    pub auth: String,
    /// Authority failure code (`DIACOD`), for `AF`.
    pub fail_code: String,
    /// Change code (`DICCOD`): `A` added, `D` deleted, `R` replaced.
    pub change_code: String,
    /// The configuration change (`DICCHG`), for `CF`.
    pub config_change: String,
    /// ACL or owner propagation (`DIPROP` `T` or `F`), for `CA`, `OW`.
    pub propagate: Option<bool>,
    /// Request type (`DIRTYP`).
    pub request: String,
    /// The bound DN (`DIUSRN`); empty for anonymous.
    pub user_dn: String,
    /// The entry's DN (`DIOBJN`).
    pub object_dn: String,
    /// The old owner or value (`DIOOWN`).
    pub old: String,
    /// The new owner or DN (`DINEWN`).
    pub new: String,
    /// The LDIF file of an import or export (`DIPNAM`).
    pub path: String,
    /// The profile to charge, when not the job's (a bound OS profile).
    pub profile: Option<String>,
}

fn chars(s: &str) -> String {
    s.chars().count().to_string()
}

/// The `DI` event for `op`.
#[must_use]
pub fn event(op: Op, f: &DiFields) -> Event {
    let mut e = Event::new(Kind::Di)
        .set("ETYP", "L")
        .set("OPER", op.code())
        .set("ACOD", f.fail_code.as_str())
        .set("CCHG", f.config_change.as_str())
        .set("CCOD", f.change_code.as_str())
        .set(
            "PROP",
            match f.propagate {
                Some(true) => "T",
                Some(false) => "F",
                None => "",
            },
        )
        .set("BNDC", f.auth.as_str())
        .set("LVRS", f.ldap_version.as_str())
        .set("SSLI", if f.tls { "Y" } else { "N" })
        .set("RTYP", f.request.as_str())
        .set("CONN", f.connection.as_str())
        .set("IPAD", f.client.as_str())
        .set("UNLN", chars(&f.user_dn))
        .set("OBJL", chars(&f.object_dn))
        .set("OOLN", chars(&f.old))
        .set("NEWL", chars(&f.new))
        .set("PLEN", chars(&f.path))
        .set(
            "APIN",
            if f.path.is_empty() {
                ""
            } else if f.path.starts_with('/') {
                "Y"
            } else {
                "N"
            },
        )
        .set("INST", f.instance.as_str());
    for (tag, s) in [
        (TAIL_USER_DN, &f.user_dn),
        (TAIL_OBJECT_DN, &f.object_dn),
        (TAIL_OLD, &f.old),
        (TAIL_NEW, &f.new),
        (TAIL_PATH, &f.path),
    ] {
        if !s.is_empty() {
            e.tail.push((tag, s.clone()));
        }
    }
    if let Some(p) = &f.profile {
        e = e.by(p);
    }
    e
}

/// Audit a directory operation: the `DI` entry is recorded for the
/// request in progress and written when `QAUDCTL` and `QAUDLVL` (or the
/// user's `AUDLVL`) select its operation's category.
pub fn audit_di(op: Op, f: &DiFields) {
    record(event(op, f));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_and_categories() {
        for o in OPS {
            assert_eq!(Op::by_code(o.code()), Some(o));
        }
        assert_eq!(Op::PasswordFail.category(), Some("*AUTFAIL"));
        assert_eq!(Op::Bind.category(), Some("*SECDIRSRV"));
        assert_eq!(Op::Rename.category(), Some("*OBJMGT"));
        assert_eq!(Op::Read.category(), None);
        assert_eq!(category("??"), Some("*SECDIRSRV"));
    }

    #[test]
    fn entries_round_trip_and_select() {
        let f = DiFields {
            instance: "QUSRDIR".into(),
            connection: "17".into(),
            client: "10.0.0.9".into(),
            ldap_version: "V3".into(),
            auth: "SIMPLE".into(),
            user_dn: "cn=Bob,o=Acme".into(),
            object_dn: "cn=Alice,o=Acme".into(),
            ..DiFields::default()
        };
        let e = event(Op::Change, &f);
        let b = e.encode();
        assert!(b.len() > crate::audit::layout::DI.size.unwrap());
        let back = Event::decode(Kind::Di, &b).unwrap();
        assert_eq!(back.get("OPER"), "ZC");
        assert_eq!(back.get("INST"), "QUSRDIR");
        assert_eq!(back.get("UNLN"), "13");
        assert_eq!(back.get("SSLI"), "N");
        assert_eq!(back.tail_str(TAIL_USER_DN), "cn=Bob,o=Acme");
        assert_eq!(back.tail_str(TAIL_OBJECT_DN), "cn=Alice,o=Acme");
        let get = |n: &str| match n {
            "QAUDCTL" => "*AUDLVL".to_string(),
            "QAUDLVL" => "*AUTFAIL *SECDIRSRV".to_string(),
            _ => String::new(),
        };
        let c = crate::audit::Control::from_sysvals(&get);
        assert!(!c.wanted(&e, &[]), "ZC needs *OBJAUD");
        assert!(c.wanted(&event(Op::Bind, &f), &[]));
        assert!(c.wanted(&event(Op::PasswordFail, &f), &[]));
        assert!(!c.wanted(&event(Op::Create, &f), &[]));
        assert!(c.wanted(&event(Op::Create, &f), &["*CREATE".into()]));
        let s = |n: &str| match n {
            "QAUDCTL" => "*AUDLVL".to_string(),
            "QAUDLVL" => "*SECURITY".to_string(),
            _ => String::new(),
        };
        let c = crate::audit::Control::from_sysvals(&s);
        assert!(c.wanted(&event(Op::OwnerChange, &f), &[]));
    }
}
