// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Binds (docs/DIRSRV.md §9): anonymous when allowed; simple against the
//! administrator's password, a DN the service answers for (the
//! [`Identity`](super::Identity) hook), or an entry's `userPassword`
//! under the password policy; SASL EXTERNAL with the TLS client
//! certificate's subject. Every other mechanism is authMethodNotSupported
//! (7), as IBM's server answers one it lacks.
//!
//! A successful bind finds the static groups (`groupOfNames`,
//! `groupOfUniqueNames`, `accessGroup`, `accessRole`) that list the DN;
//! they are the session's groups for access control until the next bind.

use ldap::ResultCode;
use ldap::control::{ControlValue, PasswordPolicyResponse, PolicyError, PolicyWarning};
use ldap::dn::Dn;
use ldap::filter::Filter;
use ldap::msg::{BindAuth, LdapMessage, LdapResult, ModOp, Modification, ProtocolOp, Scope};
use store::txn::Txn;

use super::controls::{self, Op};
use super::{AuditOp, Conn, Directory, Method, OpError, OpResult, Outcome, Session};
use crate::backend::{DirEntry, Scan};
use crate::pwd::{self, Partial, Policy, State};

/// The static group classes and their member attributes.
const GROUP_CLASSES: [&str; 4] = ["groupOfNames", "groupOfUniqueNames", "accessGroup", "accessRole"];

fn first_text(e: &DirEntry, name: &str) -> Option<String> {
    e.get(name)
        .and_then(|a| a.vals.first())
        .map(|v| String::from_utf8_lossy(v).trim().to_string())
}

impl Directory {
    /// The static groups of the btree backend that list `key` as a
    /// `member` or `uniqueMember`.
    pub(crate) fn static_groups(&self, txn: &mut Txn<'_>, key: &[u8]) -> OpResult<Vec<DirEntry>> {
        let filter = Filter::Or(
            GROUP_CLASSES
                .iter()
                .map(|c| Filter::Equality("objectClass".into(), c.as_bytes().to_vec()))
                .collect(),
        );
        let b = &self.backends[0];
        let mut out = Vec::new();
        for s in b.suffixes() {
            let Some(root) = b.resolve(txn, &self.schema, &self.sys, &s)?.entry else {
                continue;
            };
            let mut scan = Scan::Start;
            loop {
                let ids =
                    b.scan(txn, &self.schema, &self.sys, root, Scope::Sub, &filter, &mut scan, 256)?;
                for id in ids {
                    let Some(e) = b.entry(txn, &self.schema, &self.sys, id)? else {
                        continue;
                    };
                    if !GROUP_CLASSES.iter().any(|c| self.has_class(&e, c)) {
                        continue;
                    }
                    let member = ["member", "uniqueMember"].iter().any(|m| {
                        e.get(m).is_some_and(|a| {
                            a.vals.iter().any(|v| {
                                let t = String::from_utf8_lossy(v);
                                // uniqueMember may carry `#'bits'B`.
                                let name = t.split('#').next().unwrap_or("");
                                Dn::parse(name.trim())
                                    .is_ok_and(|d| self.schema.normalize_parsed_dn(&d) == key)
                            })
                        })
                    });
                    if member {
                        out.push(e);
                    }
                }
                if matches!(scan, Scan::Done) {
                    break;
                }
            }
        }
        Ok(out)
    }

    /// An entry of the btree backend by DN, as the service sees it.
    pub(crate) fn sys_entry(&self, txn: &mut Txn<'_>, dn: &str) -> OpResult<Option<DirEntry>> {
        let Ok(d) = Dn::parse(dn) else { return Ok(None) };
        let b = &self.backends[0];
        let Some(id) = b.resolve(txn, &self.schema, &self.sys, &d)?.entry else {
            return Ok(None);
        };
        b.entry(txn, &self.schema, &self.sys, id)
    }

    /// The password policy for user entry `e` with groups `groups`;
    /// `None` when none is in force.
    pub(crate) fn policy_for(
        &self,
        txn: &mut Txn<'_>,
        e: &DirEntry,
        groups: &[DirEntry],
    ) -> OpResult<Option<Policy>> {
        let Some(g) = self.sys_entry(txn, pwd::policy::GLOBAL_DN)? else {
            return Ok(None);
        };
        let global = Partial::from_attrs(&g.attrs);
        let per_user = first_text(&g, "ibm-pwdGroupAndIndividualEnabled")
            .is_some_and(|v| v.eq_ignore_ascii_case("TRUE"));
        let mut group_pols = Vec::new();
        let mut individual = None;
        if per_user {
            for gr in groups {
                if let Some(dn) = first_text(gr, "ibm-pwdGroupPolicyDN")
                    && let Some(p) = self.sys_entry(txn, &dn)?
                {
                    group_pols.push(Partial::from_attrs(&p.attrs));
                }
            }
            if let Some(dn) = first_text(e, "ibm-pwdIndividualPolicyDN")
                && let Some(p) = self.sys_entry(txn, &dn)?
            {
                individual = Some(Partial::from_attrs(&p.attrs));
            }
        }
        let pol = Policy::effective(&global, &group_pols, individual.as_ref());
        Ok(pol.enabled.then_some(pol))
    }

    /// Server-side changes to entry `id` of backend `b` (password policy
    /// state): replaces, an empty replace removing the attribute.
    pub(crate) fn server_mods(
        &self,
        txn: &mut Txn<'_>,
        b: usize,
        id: u64,
        mods: &[Modification],
    ) -> OpResult<()> {
        if mods.is_empty() {
            return Ok(());
        }
        let backend = &self.backends[b];
        let Some(e) = backend.entry(txn, &self.schema, &self.sys, id)? else {
            return Ok(());
        };
        let mut attrs = e.attrs;
        for m in mods {
            attrs.retain(|a| !a.ty.eq_ignore_ascii_case(&m.attr.ty));
            if m.op != ModOp::Delete && !m.attr.vals.is_empty() {
                attrs.push(m.attr.clone());
            }
        }
        backend.replace(txn, &self.schema, id, &attrs)
    }

    /// A session for `dn` with its static groups (and `extra` group DNs).
    pub(crate) fn session_for(
        &self,
        txn: &mut Txn<'_>,
        dn: &Dn,
        method: Method,
        extra: &[String],
    ) -> OpResult<(Session, Vec<DirEntry>)> {
        let mut s = Session::bound(&self.schema, dn, method);
        let key = s.key.clone().unwrap_or_default();
        let groups = self.static_groups(txn, &key)?;
        for g in &groups {
            s.groups.push(self.entry_key(&g.dn));
        }
        for g in extra {
            s.groups.push(self.entry_key(g));
        }
        Ok((s, groups))
    }

    /// `dir_bind`: a bind request. The outcome's session is the
    /// connection's from now on (anonymous after a failure). Commit the
    /// transaction whatever the result.
    pub fn bind(&mut self, txn: &mut Txn<'_>, msg: &LdapMessage, conn: &Conn) -> Outcome {
        self.counters.ops_initiated += 1;
        self.counters.binds_requested += 1;
        let ProtocolOp::BindRequest { version, name, auth } = &msg.op else {
            return Outcome::one(
                ProtocolOp::BindResponse {
                    result: Self::result(ResultCode::ProtocolError, "not a bind request"),
                    server_sasl_creds: None,
                },
                Vec::new(),
            );
        };
        let want_policy = msg.controls.iter().any(|c| c.oid == ldap::control::PASSWORD_POLICY);
        let r = controls::parse(msg, Op::Bind, &self.settings, false)
            .and_then(|_| self.do_bind(txn, *version, name, auth, conn));
        let (result, session, warning, perr) = match r {
            Ok((s, w, pe)) => (LdapResult::success(), s, w, pe),
            Err(e) => {
                let p = e.policy;
                (e.result(), Session::anonymous(), None, p)
            }
        };
        let mut ctls = Vec::new();
        if want_policy && (warning.is_some() || perr.is_some()) {
            ctls.push(
                PasswordPolicyResponse {
                    warning,
                    error: perr,
                }
                .to_control(false),
            );
        }
        let code = result.code;
        self.audit(AuditOp::Bind, name, &session, code);
        self.counters.binds_completed += 1;
        self.counters.ops_completed += 1;
        let mut o = Outcome::one(
            ProtocolOp::BindResponse {
                result,
                server_sasl_creds: None,
            },
            ctls,
        );
        o.session = Some(session);
        o
    }

    #[allow(clippy::type_complexity)]
    fn do_bind(
        &mut self,
        txn: &mut Txn<'_>,
        version: i32,
        name: &str,
        auth: &BindAuth,
        conn: &Conn,
    ) -> OpResult<(Session, Option<PolicyWarning>, Option<PolicyError>)> {
        if version != 2 && version != 3 {
            return Err(OpError::new(ResultCode::ProtocolError, "LDAP versions 2 and 3 only"));
        }
        let invalid = || OpError::new(ResultCode::InvalidCredentials, "the credentials are not valid");
        match auth {
            BindAuth::Simple(pw) => {
                if name.is_empty() {
                    if !pw.is_empty() {
                        return Err(invalid());
                    }
                    if !self.settings.allow_anon {
                        return Err(OpError::new(
                            ResultCode::InappropriateAuthentication,
                            "anonymous binds are not allowed",
                        ));
                    }
                    return Ok((Session::anonymous(), None, None));
                }
                if pw.is_empty() {
                    return Err(OpError::unwilling("a bind with a name needs a password"));
                }
                let dn = Self::parse_dn(name)?;
                self.simple(txn, &dn, pw)
            }
            BindAuth::Sasl { mechanism, credentials } => {
                if version == 2 || !mechanism.eq_ignore_ascii_case("EXTERNAL") {
                    return Err(OpError::new(
                        ResultCode::AuthMethodNotSupported,
                        format!("SASL mechanism {mechanism} is not supported"),
                    ));
                }
                let (true, Some(subject)) = (conn.tls, conn.client_subject.as_deref()) else {
                    return Err(OpError::new(
                        ResultCode::InappropriateAuthentication,
                        "EXTERNAL needs a TLS client certificate",
                    ));
                };
                let dn = Self::parse_dn(subject)?;
                if let Some(c) = credentials.as_deref().filter(|c| !c.is_empty()) {
                    let authz = String::from_utf8_lossy(c);
                    let ok = authz
                        .strip_prefix("dn:")
                        .and_then(|d| Dn::parse(d).ok())
                        .is_some_and(|d| self.schema.dn_eq(&d, &dn));
                    if !ok {
                        return Err(OpError::new(
                            ResultCode::InappropriateAuthentication,
                            "EXTERNAL authorizes only the certificate's subject",
                        ));
                    }
                }
                if self.is_admin_dn(&dn) {
                    let mut s = Session::admin(&self.schema, &dn);
                    s.method = Method::External;
                    return Ok((s, None, None));
                }
                if let Some(id) = self.identity.as_mut()
                    && id.owns(&self.schema, &dn)
                {
                    let g = id.external(txn, &dn).ok_or_else(invalid)?;
                    let (mut s, _) = self.session_for(txn, &dn, Method::External, &g.groups)?;
                    s.admin = g.admin;
                    return Ok((s, None, None));
                }
                let found = self.find_visible(txn, &self.sys.clone(), &dn, true).map_err(|_| invalid())?;
                let dn = Self::parse_dn(&found.entry.dn)?;
                let (s, _) = self.session_for(txn, &dn, Method::External, &[])?;
                Ok((s, None, None))
            }
        }
    }

    #[allow(clippy::type_complexity)]
    fn simple(
        &mut self,
        txn: &mut Txn<'_>,
        dn: &Dn,
        pw: &[u8],
    ) -> OpResult<(Session, Option<PolicyWarning>, Option<PolicyError>)> {
        let invalid = || OpError::new(ResultCode::InvalidCredentials, "the credentials are not valid");
        if self.is_admin_dn(dn) {
            let ok = self.settings.admin_pw.as_deref().is_some_and(|h| pwd::verify(h, pw));
            if !ok {
                return Err(invalid());
            }
            return Ok((Session::admin(&self.schema, dn), None, None));
        }
        if let Some(id) = self.identity.as_mut()
            && id.owns(&self.schema, dn)
        {
            return match id.bind(txn, dn, pw) {
                Ok(g) => {
                    let (mut s, _) = self.session_for(txn, dn, Method::Simple, &g.groups)?;
                    s.admin = g.admin;
                    Ok((s, g.warning, None))
                }
                Err(r) => Err(OpError {
                    policy: r.policy,
                    ..OpError::new(r.code, "the credentials are not valid")
                }),
            };
        }
        let sys = self.sys.clone();
        let found = match self.find(txn, &sys, dn, true) {
            Ok(Some(f)) => f,
            _ => return Err(invalid()),
        };
        let e = &found.entry;
        let stored: Vec<Vec<u8>> = e.get("userPassword").map(|a| a.vals.clone()).unwrap_or_default();
        let ok = stored.iter().any(|h| pwd::verify(h, pw));
        let edn = Self::parse_dn(&e.dn)?;
        let (mut session, groups) = self.session_for(txn, &edn, Method::Simple, &[])?;
        let policy = self.policy_for(txn, e, &groups)?;
        let Some(policy) = policy else {
            return if ok && !stored.is_empty() {
                Ok((session, None, None))
            } else {
                Err(invalid())
            };
        };
        let st = State::from_attrs(&e.attrs);
        let v = pwd::policy::on_bind(&policy, &st, self.now(), ok);
        self.server_mods(txn, found.backend, e.id, &v.mods)?;
        if !v.allowed {
            return Err(OpError {
                policy: v.error,
                ..invalid()
            });
        }
        session.must_change = v.must_change;
        Ok((session, v.warning, v.error))
    }
}
