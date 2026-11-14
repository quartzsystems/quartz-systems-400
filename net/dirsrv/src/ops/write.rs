// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `dir_op`: add, modify (add, delete, replace, increment), delete (a
//! leaf, or a subtree with the tree delete control) and modify DN (new
//! RDN, old RDN kept or dropped, new superior), with the schema check,
//! the operational attributes the server keeps, access control, the
//! password scheme and policy, the change log and the controls of §7.
//!
//! Rights (§12): add needs `object:a` on the parent (a suffix's root,
//! the administrator); delete `object:d` on the entry (each entry, for a
//! tree delete); modify `w` on each attribute changed; modify DN `w` on
//! the old and new RDN's attributes, and with a new superior `object:a`
//! there and `object:d` on the entry. A user may always change its own
//! `userPassword` unless the password policy says `pwdAllowUserChange:
//! FALSE`. Writing `aclEntry` or `entryOwner` takes `w` on the
//! restricted class (owners have it); filter-based ACLs are refused (53).

use std::fmt::Write as _;

use ldap::ResultCode;
use ldap::control::{
    ControlValue, PasswordPolicyResponse, PolicyError, PostReadResponse, PreReadResponse, ReadEntry,
};
use ldap::dn::{Dn, Rdn};
use ldap::ldif::{self, AttrValue, Body, ModSpec, Record, Value};
use ldap::msg::{Attribute, LdapMessage, LdapResult, ModOp, Modification, ProtocolOp};
use store::txn::Txn;

use super::attrs::Wanted;
use super::controls::{self, Ctl, Op};
use super::{
    AclCache, AuditOp, Change, Directory, Found, OpError, OpResult, Outcome, Place, Session,
};
use crate::acl;
use crate::backend::DirEntry;
use crate::pwd::policy::{self, Changer};
use crate::pwd::{State, scheme};
use crate::schema::check::split_description;
use crate::schema::{AttributeType, Level};
use crate::store::{ChangeKind, ChangeRec};

/// Attributes a user's request may not hold at all.
const FILTER_ACLS: [&str; 2] = ["ibm-filterAclEntry", "ibm-filterAclInherit"];

/// Operational attributes the server sets on every entry.
const KEPT: [&str; 7] = [
    "creatorsName",
    "createTimestamp",
    "modifiersName",
    "modifyTimestamp",
    "entryUUID",
    "ibm-entryUuid",
    "structuralObjectClass",
];

fn response(kind: Op, result: LdapResult) -> ProtocolOp {
    match kind {
        Op::Add => ProtocolOp::AddResponse(result),
        Op::Modify => ProtocolOp::ModifyResponse(result),
        Op::Delete => ProtocolOp::DelResponse(result),
        Op::ModDn => ProtocolOp::ModDnResponse(result),
        Op::Compare => ProtocolOp::CompareResponse(result),
        _ => ProtocolOp::ExtendedResponse {
            result,
            name: None,
            value: None,
        },
    }
}

fn set(attrs: &mut Vec<Attribute>, ty: &str, vals: Vec<Vec<u8>>) {
    attrs.retain(|a| !a.ty.eq_ignore_ascii_case(ty));
    if !vals.is_empty() {
        attrs.push(Attribute {
            ty: ty.into(),
            vals,
        });
    }
}

/// What a write did, for the response and the change feed.
struct Done {
    changes: Vec<Change>,
    controls: Vec<ldap::control::Control>,
}

impl Directory {
    /// `dir_op`: one add, modify, delete, modify DN, compare or extended
    /// operation in the caller's transaction. Commit when
    /// [`Outcome::succeeded`], abort otherwise.
    pub fn op(&mut self, txn: &mut Txn<'_>, session: &Session, msg: &LdapMessage) -> Outcome {
        self.counters.ops_initiated += 1;
        let (kind, target, audit) = match &msg.op {
            ProtocolOp::AddRequest { dn, .. } => {
                self.counters.adds_requested += 1;
                (Op::Add, dn.clone(), AuditOp::Add)
            }
            ProtocolOp::ModifyRequest { dn, .. } => {
                self.counters.modifies_requested += 1;
                (Op::Modify, dn.clone(), AuditOp::Modify)
            }
            ProtocolOp::DelRequest(dn) => {
                self.counters.deletes_requested += 1;
                (Op::Delete, dn.clone(), AuditOp::Delete)
            }
            ProtocolOp::ModDnRequest { dn, .. } => {
                self.counters.modrdns_requested += 1;
                (Op::ModDn, dn.clone(), AuditOp::ModDn)
            }
            ProtocolOp::CompareRequest { dn, .. } => {
                self.counters.compares_requested += 1;
                (Op::Compare, dn.clone(), AuditOp::Compare)
            }
            ProtocolOp::ExtendedRequest { name, .. } => {
                self.counters.extops_requested += 1;
                let out = self.extended(txn, session, msg);
                self.counters.extops_completed += 1;
                self.counters.ops_completed += 1;
                let code = out.code.unwrap_or(ResultCode::Success);
                self.audit(AuditOp::Extended, name, session, code);
                return out;
            }
            _ => {
                self.counters.unknown_requested += 1;
                self.counters.unknown_completed += 1;
                return Outcome::one(
                    ProtocolOp::ExtendedResponse {
                        result: Self::result(ResultCode::ProtocolError, "not an operation request"),
                        name: None,
                        value: None,
                    },
                    Vec::new(),
                );
            }
        };
        let want_policy = msg
            .controls
            .iter()
            .any(|c| c.oid == ldap::control::PASSWORD_POLICY);
        let r = controls::parse(msg, kind, &self.settings, session.admin).and_then(|ctl| {
            let acting = self.acting(txn, session, ctl.proxy.as_deref())?;
            match &msg.op {
                ProtocolOp::CompareRequest { dn, attr, value } => self
                    .compare(txn, &acting, &ctl, dn, attr, value)
                    .map(|code| (code, None)),
                _ => self
                    .write(txn, &acting, &ctl, &msg.op)
                    .map(|d| (ResultCode::Success, Some(d))),
            }
        });
        let mut out = match r {
            Ok((code, d)) => {
                let d = d.unwrap_or(Done {
                    changes: Vec::new(),
                    controls: Vec::new(),
                });
                let mut o = Outcome::one(response(kind, LdapResult::new(code)), d.controls);
                o.changes = d.changes;
                o
            }
            Err(e) => {
                let mut ctls = Vec::new();
                if want_policy && e.policy.is_some() {
                    ctls.push(
                        PasswordPolicyResponse {
                            warning: None,
                            error: e.policy,
                        }
                        .to_control(false),
                    );
                }
                Outcome::one(response(kind, e.result()), ctls)
            }
        };
        if matches!(&msg.op, ProtocolOp::ModifyRequest { dn, .. } if crate::backend::schema::is_schema_dn(&self.schema, &Dn::parse(dn).unwrap_or_else(|_| Dn::root())))
            && out.succeeded()
        {
            out.schema_changed = true;
        }
        let c = &mut self.counters;
        match kind {
            Op::Add => c.adds_completed += 1,
            Op::Modify => c.modifies_completed += 1,
            Op::Delete => c.deletes_completed += 1,
            Op::ModDn => c.modrdns_completed += 1,
            _ => c.compares_completed += 1,
        }
        c.ops_completed += 1;
        let code = out.code.unwrap_or(ResultCode::Success);
        self.audit(audit, &target, session, code);
        out
    }

    fn write(
        &mut self,
        txn: &mut Txn<'_>,
        s: &Session,
        ctl: &Ctl,
        op: &ProtocolOp,
    ) -> OpResult<Done> {
        if s.must_change {
            let own_password = match op {
                ProtocolOp::ModifyRequest { dn, changes } => {
                    Dn::parse(dn).is_ok_and(|d| Some(self.schema.normalize_parsed_dn(&d)) == s.key)
                        && changes.iter().all(|m| {
                            self.schema
                                .attribute(split_description(&m.attr.ty).0)
                                .is_some_and(|a| a.is_named("userPassword"))
                        })
                }
                _ => false,
            };
            if !own_password {
                return Err(OpError {
                    policy: Some(PolicyError::ChangeAfterReset),
                    ..OpError::unwilling("the password must be changed first")
                });
            }
        }
        match op {
            ProtocolOp::AddRequest { dn, attrs } => self.add(txn, s, ctl, dn, attrs),
            ProtocolOp::ModifyRequest { dn, changes } => self.modify(txn, s, ctl, dn, changes),
            ProtocolOp::DelRequest(dn) => self.delete(txn, s, ctl, dn),
            ProtocolOp::ModDnRequest {
                dn,
                new_rdn,
                delete_old_rdn,
                new_superior,
            } => self.moddn(
                txn,
                s,
                ctl,
                dn,
                new_rdn,
                *delete_old_rdn,
                new_superior.as_deref(),
            ),
            _ => Err(OpError::new(ResultCode::ProtocolError, "not a write")),
        }
    }

    fn writable(&self, b: usize) -> OpResult<()> {
        if self.settings.read_only || self.backends[b].read_only() {
            return Err(OpError::unwilling(
                "this part of the directory is read-only",
            ));
        }
        Ok(())
    }

    fn uuid(&self) -> String {
        let mut b = [0u8; 16];
        self.env.random(&mut b);
        b[6] = (b[6] & 0x0f) | 0x40;
        b[8] = (b[8] & 0x3f) | 0x80;
        let h: String = b.iter().fold(String::new(), |mut s, x| {
            let _ = write!(s, "{x:02x}");
            s
        });
        format!(
            "{}-{}-{}-{}-{}",
            &h[..8],
            &h[8..12],
            &h[12..16],
            &h[16..20],
            &h[20..]
        )
    }

    fn hash_password(&self, v: &[u8]) -> Vec<u8> {
        if scheme::is_hashed(v) {
            return v.to_vec();
        }
        let mut salt = [0u8; scheme::SALT_LEN];
        self.env.random(&mut salt);
        self.settings.pw_scheme.hash(v, &salt)
    }

    /// The attribute type of a request's description, refusing what no
    /// user may write.
    fn writable_type(&self, desc: &str) -> OpResult<&AttributeType> {
        let at = self.schema.attribute_for(desc)?;
        if FILTER_ACLS.iter().any(|f| at.is_named(f)) {
            return Err(OpError::unwilling("filter-based ACLs are not supported"));
        }
        self.schema.check_user_modification(at)?;
        Ok(at)
    }

    fn check_acl_values(&self, at: &AttributeType, vals: &[Vec<u8>]) -> OpResult<()> {
        for v in vals {
            let t = String::from_utf8_lossy(v);
            if at.is_named("aclEntry") {
                acl::model::parse_acl(&self.schema, &t)?;
            } else if at.is_named("entryOwner") {
                acl::model::parse_owner(&self.schema, &t)?;
            }
        }
        Ok(())
    }

    /// The entry read for a pre-read or post-read control.
    fn read_entry(
        &self,
        txn: &mut Txn<'_>,
        s: &Session,
        b: usize,
        e: &DirEntry,
        attrs: &[String],
    ) -> OpResult<ReadEntry> {
        let mut cache = AclCache::default();
        let want = Wanted::from_list(attrs);
        let attrs = self.view(txn, &mut cache, b, e, s, &want, false)?;
        Ok(ReadEntry {
            dn: e.dn.clone(),
            attrs,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn log(
        &self,
        txn: &mut Txn<'_>,
        b: usize,
        kind: ChangeKind,
        id: u64,
        target: &str,
        modifier: &str,
        changes: Vec<u8>,
        rename: Option<(&str, bool, Option<&str>)>,
    ) -> OpResult<Option<u64>> {
        let rec = ChangeRec {
            kind,
            time: self.now() / 1000,
            entry: id,
            target_dn: target.into(),
            new_rdn: rename.map(|r| r.0.to_string()),
            delete_old_rdn: rename.is_some_and(|r| r.1),
            new_superior: rename.and_then(|r| r.2.map(String::from)),
            modifier: modifier.into(),
            changes,
        };
        self.backends[b].log(txn, &rec)
    }

    fn stamp(&self, attrs: &mut Vec<Attribute>, s: &Session, creating: bool) {
        let now = super::clock::gentime(self.now());
        let who = s.who();
        if creating {
            set(attrs, "creatorsName", vec![who.clone().into_bytes()]);
            set(attrs, "createTimestamp", vec![now.clone().into_bytes()]);
            let u = self.uuid();
            set(attrs, "entryUUID", vec![u.clone().into_bytes()]);
            set(attrs, "ibm-entryUuid", vec![u.into_bytes()]);
        }
        set(attrs, "modifiersName", vec![who.into_bytes()]);
        set(attrs, "modifyTimestamp", vec![now.into_bytes()]);
    }

    /// Check an entry's attributes and store its classes as the check
    /// resolved them.
    fn checked(
        &self,
        dn: &Dn,
        attrs: &mut Vec<Attribute>,
        old_structural: Option<&str>,
    ) -> OpResult<()> {
        let user: Vec<Attribute> = attrs
            .iter()
            .filter(|a| !KEPT.iter().any(|k| a.ty.eq_ignore_ascii_case(k)))
            .cloned()
            .collect();
        let c = self
            .schema
            .check_entry(dn, &user, self.settings.schema_check)?;
        if let (Some(old), Level::V3) = (old_structural, self.settings.schema_check)
            && c.structural
                .as_deref()
                .is_some_and(|n| !n.eq_ignore_ascii_case(old))
        {
            return Err(OpError::new(
                ResultCode::ObjectClassModsProhibited,
                "the structural object class cannot change",
            ));
        }
        let oc = attrs
            .iter()
            .position(|a| {
                self.schema
                    .attribute(split_description(&a.ty).0)
                    .is_some_and(|t| t.oid() == "2.5.4.0")
            })
            .map_or_else(|| "objectClass".to_string(), |i| attrs[i].ty.clone());
        set(
            attrs,
            &oc,
            c.object_classes
                .iter()
                .map(|n| n.clone().into_bytes())
                .collect(),
        );
        set(
            attrs,
            "structuralObjectClass",
            c.structural.into_iter().map(String::into_bytes).collect(),
        );
        Ok(())
    }

    /// The parent of a new entry: `None` for a suffix's root.
    fn parent_for(
        &self,
        txn: &mut Txn<'_>,
        s: &Session,
        b: usize,
        dn: &Dn,
        manage: bool,
    ) -> OpResult<Option<Found>> {
        let is_suffix = self.backends[b]
            .suffixes()
            .iter()
            .any(|x| self.schema.dn_eq(x, dn));
        if is_suffix {
            return Ok(None);
        }
        let parent = dn.parent().unwrap_or_else(Dn::root);
        match self.find(txn, s, &parent, manage) {
            Ok(Some(f)) => Ok(Some(f)),
            Ok(None) => Err(OpError::new(
                ResultCode::NoSuchObject,
                "the parent does not exist",
            )),
            Err(e) => Err(e),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn add(
        &mut self,
        txn: &mut Txn<'_>,
        s: &Session,
        ctl: &Ctl,
        dn: &str,
        attrs: &[Attribute],
    ) -> OpResult<Done> {
        let d = Self::parse_dn(dn)?;
        let b = match self.place(&d) {
            Place::Backend(b) => b,
            Place::Outside => return Err(self.outside(&d)),
            _ => return Err(OpError::unwilling("entries cannot be added here")),
        };
        self.writable(b)?;
        if self.backends[b].admin_only() && !s.admin {
            return Err(OpError::access());
        }
        if self.backends[b]
            .resolve(txn, &self.schema, &self.sys, &d)?
            .entry
            .is_some()
        {
            return Err(OpError::new(
                ResultCode::EntryAlreadyExists,
                format!("{dn} exists"),
            ));
        }
        let parent = self.parent_for(txn, s, b, &d, ctl.manage)?;
        let mut cache = AclCache::default();
        if let Some(p) = &parent {
            if p.backend != b {
                return Err(OpError::new(
                    ResultCode::AffectsMultipleDsas,
                    "the parent is held elsewhere",
                ));
            }
            let eff = self.effective_of(txn, &mut cache, b, &p.entry)?;
            let key = self.entry_key(&p.entry.dn);
            need(eff.object_rights(&s.principal(), &key) & acl::A != 0)?;
        } else {
            need(s.admin)?;
        }
        let mut stored: Vec<Attribute> = Vec::new();
        let mut password: Vec<Vec<u8>> = Vec::new();
        for a in attrs {
            let at = self.writable_type(&a.ty)?;
            self.check_acl_values(at, &a.vals)?;
            let vals = if at.is_named("userPassword") {
                password.extend(a.vals.iter().cloned());
                a.vals.iter().map(|v| self.hash_password(v)).collect()
            } else {
                a.vals.clone()
            };
            match stored.iter_mut().find(|x| x.ty.eq_ignore_ascii_case(&a.ty)) {
                Some(x) => x.vals.extend(vals),
                None => stored.push(Attribute {
                    ty: a.ty.clone(),
                    vals,
                }),
            }
        }
        self.checked(&d, &mut stored, None)?;
        self.stamp(&mut stored, s, true);
        let new = DirEntry {
            id: 0,
            dn: dn.to_string(),
            attrs: stored.clone(),
        };
        if let Some(f) = &ctl.assertion {
            let parent_id = parent.as_ref().map(|p| p.entry.id);
            let eff = self.effective(txn, &mut cache, b, dn, &new.attrs, parent_id)?;
            let _ = eff;
            if !self.matches_new(txn, s, b, &new, parent_id, f)? {
                return Err(OpError::new(
                    super::error::ASSERTION_FAILED,
                    "the assertion is false for the entry",
                ));
            }
        }
        if !password.is_empty()
            && let Some(pol) = self.policy_for(txn, &new, &[])?
        {
            let by = if s.admin {
                Changer::Admin
            } else {
                Changer::Other
            };
            for p in &password {
                let mods = policy::on_change(
                    &pol,
                    &State::default(),
                    self.now(),
                    &policy::Change {
                        new: p,
                        old_clear: None,
                        current: &[],
                        by,
                    },
                )
                .map_err(Self::policy_error)?;
                apply_server(&mut stored, &mods);
            }
        }
        let id = self.backends[b].add(
            txn,
            &self.schema,
            parent.as_ref().map(|p| p.entry.id),
            &d,
            &stored,
        )?;
        let changes = ldif_text(&Body::Content(attr_lines(&stored)));
        let number = self.log(txn, b, ChangeKind::Add, id, dn, &s.who(), changes, None)?;
        let mut controls = Vec::new();
        if let Some(list) = &ctl.post_read
            && let Some(e) = self.backends[b].entry(txn, &self.schema, &self.sys, id)?
        {
            controls
                .push(PostReadResponse(self.read_entry(txn, s, b, &e, list)?).to_control(false));
        }
        Ok(Done {
            changes: vec![Change {
                kind: ChangeKind::Add,
                dn: dn.to_string(),
                previous_dn: None,
                number,
                backend: b,
                id,
                parent: parent.map(|p| p.entry.id),
                attrs: stored,
            }],
            controls,
        })
    }

    fn matches_new(
        &self,
        txn: &mut Txn<'_>,
        s: &Session,
        b: usize,
        e: &DirEntry,
        parent: Option<u64>,
        f: &ldap::filter::Filter,
    ) -> OpResult<bool> {
        let mut cache = AclCache::default();
        let eff = self.effective(txn, &mut cache, b, &e.dn, &e.attrs, parent)?;
        let key = self.entry_key(&e.dn);
        let dn = Dn::parse(&e.dn).unwrap_or_else(|_| Dn::root());
        let can = |at: &AttributeType| self.attr_rights(&eff, s, &key, at) & acl::S != 0;
        let t = super::filter::Target {
            dn: &dn,
            attrs: &e.attrs,
            can_search: &can,
        };
        Ok(super::filter::eval(&self.schema, f, &t) == Some(true))
    }

    pub(crate) fn policy_error(e: PolicyError) -> OpError {
        let code = match e {
            PolicyError::PasswordModNotAllowed | PolicyError::MustSupplyOldPassword => {
                ResultCode::InsufficientAccessRights
            }
            _ => ResultCode::ConstraintViolation,
        };
        OpError {
            policy: Some(e),
            ..OpError::new(code, "the password policy refuses the change")
        }
    }

    /// A target entry for a change: found, writable, not hidden.
    fn target(&self, txn: &mut Txn<'_>, s: &Session, ctl: &Ctl, dn: &Dn) -> OpResult<Found> {
        match self.place(dn) {
            Place::Backend(_) | Place::Outside => {}
            _ => return Err(OpError::unwilling("this entry cannot be changed so")),
        }
        let f = self.find_visible(txn, s, dn, ctl.manage)?;
        self.writable(f.backend)?;
        if let Some(a) = &ctl.assertion {
            self.assert(txn, s, f.backend, &f.entry, a)?;
        }
        Ok(f)
    }

    #[allow(clippy::too_many_lines)]
    fn modify(
        &mut self,
        txn: &mut Txn<'_>,
        s: &Session,
        ctl: &Ctl,
        dn: &str,
        mods: &[Modification],
    ) -> OpResult<Done> {
        let d = Self::parse_dn(dn)?;
        if self.place(&d) == Place::Schema {
            need(s.admin)?;
            crate::backend::schema::modify(&mut self.schema, mods)
                .map_err(|(c, m)| OpError::new(c, m))?;
            return Ok(Done {
                changes: Vec::new(),
                controls: Vec::new(),
            });
        }
        let f = self.target(txn, s, ctl, &d)?;
        let b = f.backend;
        if self.backends[b].admin_only() {
            need(s.admin)?;
            for m in mods {
                self.schema.attribute_for(&m.attr.ty)?;
            }
            if let Some(r) = self.backends[b].apply(txn, f.entry.id, mods) {
                r?;
                return Ok(Done {
                    changes: Vec::new(),
                    controls: Vec::new(),
                });
            }
        }
        let mut cache = AclCache::default();
        let eff = self.effective_of(txn, &mut cache, b, &f.entry)?;
        let key = self.entry_key(&f.entry.dn);
        let own = s.key.as_deref() == Some(key.as_slice());
        let mut attrs = f.entry.attrs.clone();
        let old_structural = f
            .entry
            .get("structuralObjectClass")
            .and_then(|a| a.vals.first())
            .map(|v| String::from_utf8_lossy(v).to_string());
        let rdn = d.rdn().cloned().unwrap_or_else(|| Rdn(Vec::new()));
        let mut new_pw: Vec<Vec<u8>> = Vec::new();
        let mut old_clear: Option<Vec<u8>> = None;
        let current_pw: Vec<Vec<u8>> = f
            .entry
            .get("userPassword")
            .map(|a| a.vals.clone())
            .unwrap_or_default();
        let mut pw_touched = false;
        for m in mods {
            let at = self.writable_type(&m.attr.ty)?;
            let is_pw = at.is_named("userPassword");
            let rights = self.attr_rights(&eff, s, &key, at);
            if rights & acl::W == 0 && !(is_pw && own) {
                return Err(OpError::access());
            }
            self.check_acl_values(at, &m.attr.vals)?;
            let (_, opts) = split_description(&m.attr.ty);
            let slot = attrs.iter().position(|a| {
                let (t, o) = split_description(&a.ty);
                self.schema
                    .attribute(t)
                    .is_some_and(|x| x.oid() == at.oid())
                    && o.len() == opts.len()
                    && o.iter()
                        .all(|x| opts.iter().any(|y| y.eq_ignore_ascii_case(x)))
            });
            let ek = |v: &[u8]| self.schema.equality_key(at, v);
            if is_pw {
                pw_touched = true;
            }
            match m.op {
                ModOp::Add => {
                    if m.attr.vals.is_empty() {
                        return Err(OpError::new(
                            ResultCode::ProtocolError,
                            "an add needs values",
                        ));
                    }
                    let vals: Vec<Vec<u8>> = if is_pw {
                        new_pw.extend(m.attr.vals.iter().cloned());
                        m.attr.vals.iter().map(|v| self.hash_password(v)).collect()
                    } else {
                        m.attr.vals.clone()
                    };
                    match slot {
                        Some(i) => {
                            for v in vals {
                                let k = ek(&v);
                                if !is_pw && attrs[i].vals.iter().any(|x| ek(x) == k) {
                                    return Err(OpError::new(
                                        ResultCode::AttributeOrValueExists,
                                        format!("{} already has that value", at.name()),
                                    ));
                                }
                                attrs[i].vals.push(v);
                            }
                        }
                        None => attrs.push(Attribute {
                            ty: m.attr.ty.clone(),
                            vals,
                        }),
                    }
                }
                ModOp::Delete => {
                    let Some(i) = slot else {
                        return Err(OpError::new(
                            ResultCode::NoSuchAttribute,
                            format!("the entry has no {}", at.name()),
                        ));
                    };
                    if m.attr.vals.is_empty() {
                        attrs.remove(i);
                    } else {
                        for v in &m.attr.vals {
                            let pos = if is_pw {
                                attrs[i]
                                    .vals
                                    .iter()
                                    .position(|x| scheme::verify(x, v) || x == v)
                            } else {
                                let k = ek(v);
                                attrs[i].vals.iter().position(|x| ek(x) == k)
                            };
                            let Some(p) = pos else {
                                return Err(OpError::new(
                                    ResultCode::NoSuchAttribute,
                                    format!("{} has no such value", at.name()),
                                ));
                            };
                            if is_pw && scheme::verify(&attrs[i].vals[p], v) {
                                old_clear = Some(v.clone());
                            }
                            attrs[i].vals.remove(p);
                        }
                        if attrs[i].vals.is_empty() {
                            attrs.remove(i);
                        }
                    }
                }
                ModOp::Replace => {
                    if let Some(i) = slot {
                        attrs.remove(i);
                    }
                    if !m.attr.vals.is_empty() {
                        let vals = if is_pw {
                            new_pw.extend(m.attr.vals.iter().cloned());
                            m.attr.vals.iter().map(|v| self.hash_password(v)).collect()
                        } else {
                            m.attr.vals.clone()
                        };
                        attrs.push(Attribute {
                            ty: m.attr.ty.clone(),
                            vals,
                        });
                    }
                }
                ModOp::Increment => {
                    let Some(i) = slot else {
                        return Err(OpError::new(
                            ResultCode::NoSuchAttribute,
                            format!("the entry has no {}", at.name()),
                        ));
                    };
                    if at.syntax.as_deref() != Some("1.3.6.1.4.1.1466.115.121.1.27") {
                        return Err(OpError::new(
                            ResultCode::ConstraintViolation,
                            format!("{} is not an integer", at.name()),
                        ));
                    }
                    let [delta] = m.attr.vals.as_slice() else {
                        return Err(OpError::new(
                            ResultCode::ProtocolError,
                            "an increment takes one value",
                        ));
                    };
                    let delta: i128 =
                        String::from_utf8_lossy(delta).trim().parse().map_err(|_| {
                            OpError::new(
                                ResultCode::InvalidAttributeSyntax,
                                "the increment is not an integer",
                            )
                        })?;
                    for v in &mut attrs[i].vals {
                        let n: i128 = String::from_utf8_lossy(v).trim().parse().map_err(|_| {
                            OpError::new(
                                ResultCode::ConstraintViolation,
                                "a value is not an integer",
                            )
                        })?;
                        *v = (n + delta).to_string().into_bytes();
                    }
                }
            }
        }
        // The RDN's values stay.
        for ava in &rdn.0 {
            let Some(at) = self.schema.attribute(&ava.ty) else {
                continue;
            };
            let k = self.schema.equality_key(at, &ava.value);
            let kept = attrs.iter().any(|a| {
                self.schema
                    .attribute(split_description(&a.ty).0)
                    .is_some_and(|t| t.oid() == at.oid())
                    && a.vals.iter().any(|v| self.schema.equality_key(at, v) == k)
            });
            if !kept {
                return Err(OpError::new(
                    ResultCode::NotAllowedOnRdn,
                    "the RDN's values cannot be removed",
                ));
            }
        }
        if pw_touched && !new_pw.is_empty() {
            let groups = self.static_groups(txn, &key)?;
            if let Some(pol) = self.policy_for(txn, &f.entry, &groups)? {
                let by = if s.admin {
                    Changer::Admin
                } else if own {
                    Changer::Self_
                } else {
                    Changer::Other
                };
                let st = State::from_attrs(&f.entry.attrs);
                for p in &new_pw {
                    let m = policy::on_change(
                        &pol,
                        &st,
                        self.now(),
                        &policy::Change {
                            new: p,
                            old_clear: old_clear.as_deref(),
                            current: &current_pw,
                            by,
                        },
                    )
                    .map_err(Self::policy_error)?;
                    apply_server(&mut attrs, &m);
                }
            }
        }
        self.checked(&d, &mut attrs, old_structural.as_deref())?;
        self.stamp(&mut attrs, s, false);
        let mut controls = Vec::new();
        if let Some(list) = &ctl.pre_read {
            controls.push(
                PreReadResponse(self.read_entry(txn, s, b, &f.entry, list)?).to_control(false),
            );
        }
        self.backends[b].replace(txn, &self.schema, f.entry.id, &attrs)?;
        let text = ldif_text(&Body::Modify(
            mods.iter()
                .map(|m| ModSpec {
                    op: m.op,
                    ty: m.attr.ty.clone(),
                    values: if self
                        .schema
                        .attribute(split_description(&m.attr.ty).0)
                        .is_some_and(|a| a.is_named("userPassword"))
                    {
                        Vec::new()
                    } else {
                        m.attr.vals.iter().cloned().map(Value::Data).collect()
                    },
                })
                .collect(),
        ));
        let number = self.log(
            txn,
            b,
            ChangeKind::Modify,
            f.entry.id,
            &f.entry.dn,
            &s.who(),
            text,
            None,
        )?;
        if let Some(list) = &ctl.post_read {
            let after = DirEntry {
                attrs: attrs.clone(),
                ..f.entry.clone()
            };
            controls.push(
                PostReadResponse(self.read_entry(txn, s, b, &after, list)?).to_control(false),
            );
        }
        let parent = self.backends[b].parent(txn, f.entry.id)?;
        Ok(Done {
            changes: vec![Change {
                kind: ChangeKind::Modify,
                dn: f.entry.dn.clone(),
                previous_dn: None,
                number,
                backend: b,
                id: f.entry.id,
                parent,
                attrs,
            }],
            controls,
        })
    }

    fn delete(&mut self, txn: &mut Txn<'_>, s: &Session, ctl: &Ctl, dn: &str) -> OpResult<Done> {
        let d = Self::parse_dn(dn)?;
        let f = self.target(txn, s, ctl, &d)?;
        let b = f.backend;
        let mut controls = Vec::new();
        if let Some(list) = &ctl.pre_read {
            controls.push(
                PreReadResponse(self.read_entry(txn, s, b, &f.entry, list)?).to_control(false),
            );
        }
        let backend = &self.backends[b];
        if backend.children(txn, f.entry.id, 1)? > 0 && !ctl.tree_delete {
            return Err(OpError::new(
                ResultCode::NotAllowedOnNonLeaf,
                "the entry has entries below it",
            ));
        }
        // Every entry of the subtree, leaves last in the list.
        let mut order = vec![f.entry.id];
        let mut i = 0;
        while i < order.len() {
            let mut scan = crate::backend::Scan::Start;
            loop {
                let kids = backend.scan(
                    txn,
                    &self.schema,
                    &self.sys,
                    order[i],
                    ldap::msg::Scope::One,
                    &ldap::filter::Filter::any(),
                    &mut scan,
                    1024,
                )?;
                order.extend(kids);
                if matches!(scan, crate::backend::Scan::Done) {
                    break;
                }
            }
            i += 1;
        }
        let mut cache = AclCache::default();
        let mut changes = Vec::new();
        for &id in order.iter().rev() {
            let Some(e) = backend.entry(txn, &self.schema, &self.sys, id)? else {
                continue;
            };
            let eff = self.effective_of(txn, &mut cache, b, &e)?;
            let key = self.entry_key(&e.dn);
            need(eff.object_rights(&s.principal(), &key) & acl::D != 0)?;
            let parent = backend.parent(txn, id)?;
            backend.delete(txn, &self.schema, id)?;
            let number = self.log(
                txn,
                b,
                ChangeKind::Delete,
                id,
                &e.dn,
                &s.who(),
                Vec::new(),
                None,
            )?;
            changes.push(Change {
                kind: ChangeKind::Delete,
                dn: e.dn.clone(),
                previous_dn: None,
                number,
                backend: b,
                id,
                parent,
                attrs: e.attrs,
            });
        }
        Ok(Done { changes, controls })
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn moddn(
        &mut self,
        txn: &mut Txn<'_>,
        s: &Session,
        ctl: &Ctl,
        dn: &str,
        new_rdn: &str,
        delete_old: bool,
        new_superior: Option<&str>,
    ) -> OpResult<Done> {
        let d = Self::parse_dn(dn)?;
        let rdn = Rdn::parse(new_rdn).map_err(|_| {
            OpError::new(
                ResultCode::InvalidDnSyntax,
                format!("{new_rdn:?} is not an RDN"),
            )
        })?;
        let f = self.target(txn, s, ctl, &d)?;
        let b = f.backend;
        let backend_suffix = self.backends[b]
            .suffixes()
            .iter()
            .any(|x| self.schema.dn_eq(x, &d));
        if backend_suffix {
            return Err(OpError::unwilling("a suffix's root is not renamed"));
        }
        let mut cache = AclCache::default();
        let eff = self.effective_of(txn, &mut cache, b, &f.entry)?;
        let key = self.entry_key(&f.entry.dn);
        let old_rdn = d.rdn().cloned().unwrap_or_else(|| Rdn(Vec::new()));
        for ava in old_rdn.0.iter().chain(&rdn.0) {
            let at = self.schema.attribute(&ava.ty).ok_or_else(|| {
                OpError::new(
                    ResultCode::InvalidDnSyntax,
                    format!("RDN type {} is not defined", ava.ty),
                )
            })?;
            need(self.attr_rights(&eff, s, &key, at) & acl::W != 0)?;
        }
        let mut new_parent = None;
        let parent_dn = match new_superior {
            Some(sup) => {
                let sd = Self::parse_dn(sup)?;
                let p = self.find_visible(txn, s, &sd, ctl.manage)?;
                if p.backend != b {
                    return Err(OpError::new(
                        ResultCode::AffectsMultipleDsas,
                        "the new superior is held elsewhere",
                    ));
                }
                let peff = self.effective_of(txn, &mut cache, b, &p.entry)?;
                let pkey = self.entry_key(&p.entry.dn);
                need(peff.object_rights(&s.principal(), &pkey) & acl::A != 0)?;
                need(eff.object_rights(&s.principal(), &key) & acl::D != 0)?;
                new_parent = Some(p.entry.id);
                sd
            }
            None => d.parent().unwrap_or_else(Dn::root),
        };
        let new_dn = parent_dn.child(rdn.clone());
        let mut attrs = f.entry.attrs.clone();
        if delete_old {
            for ava in &old_rdn.0 {
                if rdn.0.iter().any(|n| n.raw_eq(ava)) {
                    continue;
                }
                let Some(at) = self.schema.attribute(&ava.ty) else {
                    continue;
                };
                let k = self.schema.equality_key(at, &ava.value);
                for a in &mut attrs {
                    if self
                        .schema
                        .attribute(split_description(&a.ty).0)
                        .is_some_and(|t| t.oid() == at.oid())
                    {
                        a.vals.retain(|v| self.schema.equality_key(at, v) != k);
                    }
                }
                attrs.retain(|a| !a.vals.is_empty());
            }
        }
        for ava in &rdn.0 {
            let at = self.schema.attribute(&ava.ty).expect("checked above");
            let k = self.schema.equality_key(at, &ava.value);
            let slot = attrs.iter().position(|a| {
                self.schema
                    .attribute(split_description(&a.ty).0)
                    .is_some_and(|t| t.oid() == at.oid())
            });
            match slot {
                Some(i) => {
                    if !attrs[i]
                        .vals
                        .iter()
                        .any(|v| self.schema.equality_key(at, v) == k)
                    {
                        attrs[i].vals.push(ava.value.clone());
                    }
                }
                None => attrs.push(Attribute {
                    ty: ava.ty.clone(),
                    vals: vec![ava.value.clone()],
                }),
            }
        }
        let old_structural = f
            .entry
            .get("structuralObjectClass")
            .and_then(|a| a.vals.first())
            .map(|v| String::from_utf8_lossy(v).to_string());
        self.checked(&new_dn, &mut attrs, old_structural.as_deref())?;
        self.stamp(&mut attrs, s, false);
        let mut controls = Vec::new();
        if let Some(list) = &ctl.pre_read {
            controls.push(
                PreReadResponse(self.read_entry(txn, s, b, &f.entry, list)?).to_control(false),
            );
        }
        let backend = &self.backends[b];
        backend.rename(txn, &self.schema, f.entry.id, &rdn, new_parent)?;
        backend.replace(txn, &self.schema, f.entry.id, &attrs)?;
        let after = backend
            .entry(txn, &self.schema, &self.sys, f.entry.id)?
            .ok_or_else(|| {
                OpError::new(ResultCode::OperationsError, "the renamed entry is gone")
            })?;
        let number = self.log(
            txn,
            b,
            ChangeKind::ModDn,
            f.entry.id,
            &f.entry.dn,
            &s.who(),
            Vec::new(),
            Some((new_rdn, delete_old, new_superior)),
        )?;
        if let Some(list) = &ctl.post_read {
            controls.push(
                PostReadResponse(self.read_entry(txn, s, b, &after, list)?).to_control(false),
            );
        }
        let parent = backend.parent(txn, f.entry.id)?;
        Ok(Done {
            changes: vec![Change {
                kind: ChangeKind::ModDn,
                dn: after.dn.clone(),
                previous_dn: Some(f.entry.dn.clone()),
                number,
                backend: b,
                id: f.entry.id,
                parent,
                attrs: after.attrs,
            }],
            controls,
        })
    }
}

/// Apply the policy's server-side changes to a working attribute list.
fn apply_server(attrs: &mut Vec<Attribute>, mods: &[Modification]) {
    for m in mods {
        set(
            attrs,
            &m.attr.ty,
            if m.op == ModOp::Delete {
                Vec::new()
            } else {
                m.attr.vals.clone()
            },
        );
    }
}

fn attr_lines(attrs: &[Attribute]) -> Vec<AttrValue> {
    attrs
        .iter()
        .filter(|a| !a.ty.eq_ignore_ascii_case("userPassword"))
        .flat_map(|a| {
            a.vals.iter().map(|v| AttrValue {
                ty: a.ty.clone(),
                value: Value::Data(v.clone()),
            })
        })
        .collect()
}

/// A change record's `changes`: the LDIF lines without `dn:` and
/// `changetype:`.
fn ldif_text(body: &Body) -> Vec<u8> {
    let text = ldif::write_one(&Record {
        line: 0,
        dn: String::new(),
        controls: Vec::new(),
        body: body.clone(),
    });
    text.lines()
        .filter(|l| !l.starts_with("dn:") && !l.starts_with("changetype:") && !l.is_empty())
        .fold(String::new(), |mut s, l| {
            s.push_str(l);
            s.push('\n');
            s
        })
        .into_bytes()
}

impl Directory {
    /// A modify on behalf of an extended operation; its changes.
    pub(crate) fn modify_for_ext(
        &mut self,
        txn: &mut Txn<'_>,
        s: &Session,
        ctl: &Ctl,
        msg: &LdapMessage,
    ) -> OpResult<Vec<Change>> {
        self.write(txn, s, ctl, &msg.op).map(|d| d.changes)
    }
}

/// insufficientAccessRights unless `ok`.
fn need(ok: bool) -> OpResult<()> {
    if ok { Ok(()) } else { Err(OpError::access()) }
}
