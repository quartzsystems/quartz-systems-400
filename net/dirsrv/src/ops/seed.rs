// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The entries a new instance holds, as the primary has them
//! (`tools/conformance/oracle/dirsrv/anon-entries.ldif`): `cn=localhost`
//! and `cn=ibmpolicies` with their `cn=replication` containers, and the
//! global password policy `cn=pwdpolicy,cn=ibmpolicies` (off, as IBM
//! ships it). No ACL or owner is stored: the defaults apply.

use ldap::msg::{Attribute, LdapMessage, ProtocolOp};
use store::txn::Txn;

use super::{Directory, OpError, OpResult, Session};

fn a(ty: &str, vals: &[&str]) -> Attribute {
    Attribute::new(ty, vals)
}

/// The seeded entries: DN as the primary writes it, and attributes.
#[must_use]
pub fn entries() -> Vec<(&'static str, Vec<Attribute>)> {
    vec![
        (
            "cn=localhost",
            vec![
                a("objectClass", &["container", "top"]),
                a("cn", &["localhost"]),
            ],
        ),
        (
            "CN=REPLICATION,CN=LOCALHOST",
            vec![
                a("objectClass", &["container", "top"]),
                a("cn", &["REPLICATION"]),
            ],
        ),
        (
            "CN=IBMPOLICIES",
            vec![
                a(
                    "objectClass",
                    &["container", "top", "ibm-replicationContext"],
                ),
                a("cn", &["IBMpolicies"]),
            ],
        ),
        (
            "cn=pwdpolicy,cn=ibmpolicies",
            vec![
                a(
                    "objectClass",
                    &[
                        "container",
                        "pwdPolicy",
                        "ibm-pwdPolicyExt",
                        "ibm-pwdGroupAndIndividualPolicies",
                        "top",
                    ],
                ),
                a("cn", &["pwdPolicy"]),
                a("pwdAttribute", &["userPassword"]),
                a("ibm-pwdPolicy", &["false"]),
                a("pwdAllowUserChange", &["true"]),
                a("pwdMustChange", &["true"]),
                a("pwdLockout", &["false"]),
                a("pwdSafeModify", &["false"]),
                a("ibm-pwdGroupAndIndividualEnabled", &["false"]),
            ],
        ),
        (
            "CN=REPLICATION,CN=IBMPOLICIES",
            vec![
                a("objectClass", &["container", "top"]),
                a("cn", &["REPLICATION"]),
            ],
        ),
    ]
}

impl Directory {
    /// Add the seeded entries that are missing (the first start of an
    /// instance; later starts find them).
    ///
    /// # Errors
    /// The add's failure.
    pub fn seed(&mut self, txn: &mut Txn<'_>) -> OpResult<()> {
        let ro = std::mem::replace(&mut self.settings.read_only, false);
        let r = self.seed_entries(txn);
        self.settings.read_only = ro;
        r
    }

    fn seed_entries(&mut self, txn: &mut Txn<'_>) -> OpResult<()> {
        let admin: Session = self.sys.clone();
        for (dn, attrs) in entries() {
            if self.sys_entry(txn, dn)?.is_some() {
                continue;
            }
            let msg = LdapMessage::new(
                0,
                ProtocolOp::AddRequest {
                    dn: dn.into(),
                    attrs,
                },
            );
            let out = self.op(txn, &admin, &msg);
            if !out.succeeded() {
                let r = out.replies.first().and_then(|r| r.op.result()).cloned();
                return Err(r.map_or_else(
                    || OpError::unwilling("the seed failed"),
                    |r| OpError::new(r.code, r.message),
                ));
            }
        }
        Ok(())
    }
}
