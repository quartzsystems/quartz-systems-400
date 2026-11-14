// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Attributes as a search returns them: the selection (`*`, `+`, `1.1`,
//! names, IBM's `+ibmaci`-style classes of `ibm-opAttrClasses`), the
//! operational attributes the server computes, and the read rights.

use ldap::msg::Attribute;
use store::txn::Txn;

use super::{AclCache, Directory, OpResult};
use crate::acl::{self, Effective};
use crate::backend::DirEntry;
use crate::schema::check::split_description;
use crate::schema::{AttributeType, Schema};

/// IBM's operational attribute classes (`ibm-opAttrClasses`): `+NAME`
/// and `++NAME` select these.
pub(crate) const OP_CLASSES: &[(&str, &[&str])] = &[
    (
        "ibmaci",
        &[
            "aclEntry",
            "aclPropagate",
            "aclSource",
            "entryOwner",
            "ownerPropagate",
            "ownerSource",
            "ibm-effectiveAcl",
        ],
    ),
    (
        "ibmentry",
        &[
            "createTimestamp",
            "modifyTimestamp",
            "creatorsName",
            "modifiersName",
            "entryUUID",
            "ibm-entryUuid",
            "entryDN",
            "hasSubordinates",
            "numSubordinates",
            "subschemaSubentry",
            "structuralObjectClass",
        ],
    ),
    (
        "ibmpwdpolicy",
        &[
            "pwdChangedTime",
            "pwdAccountLockedTime",
            "pwdExpirationWarned",
            "pwdFailureTime",
            "pwdHistory",
            "pwdGraceUseTime",
            "pwdReset",
            "ibm-pwdAccountLocked",
        ],
    ),
    (
        "ibmrepl",
        &[
            "ibm-replicationThisServerIsMaster",
            "ibm-replicationIsQuiesced",
        ],
    ),
];

/// The attributes computed for every entry (none of them stored).
pub(crate) const COMPUTED: &[&str] = &[
    "entryDN",
    "hasSubordinates",
    "numSubordinates",
    "subschemaSubentry",
    "aclEntry",
    "aclPropagate",
    "aclSource",
    "entryOwner",
    "ownerPropagate",
    "ownerSource",
    "ibm-effectiveAcl",
    "ibm-replicationThisServerIsMaster",
    "ibm-replicationIsQuiesced",
];

/// What a request's attribute list asks for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Wanted {
    pub all_user: bool,
    pub all_op: bool,
    pub names: Vec<String>,
}

impl Wanted {
    pub(crate) fn from_list(list: &[String]) -> Self {
        let mut w = Self::default();
        let only_none = list.iter().all(|a| a.trim() == "1.1");
        if list.is_empty() {
            w.all_user = true;
            return w;
        }
        if only_none {
            return w;
        }
        for a in list {
            let a = a.trim();
            match a {
                "*" => w.all_user = true,
                "+" | "++" => w.all_op = true,
                "1.1" | "" => {}
                _ => {
                    let class = a.strip_prefix("++").or_else(|| a.strip_prefix('+'));
                    if let Some(c) = class {
                        if let Some((_, names)) =
                            OP_CLASSES.iter().find(|(n, _)| n.eq_ignore_ascii_case(c))
                        {
                            w.names.extend(names.iter().map(|n| (*n).to_string()));
                        }
                    } else {
                        w.names.push(a.to_string());
                    }
                }
            }
        }
        w
    }

    /// Whether attribute type `name` (operational or not) is wanted.
    pub(crate) fn wants(&self, schema: &Schema, desc: &str, operational: bool) -> bool {
        if operational && self.all_op || !operational && self.all_user {
            return true;
        }
        let (ty, _) = split_description(desc);
        let at = schema.attribute(ty);
        self.names.iter().any(|n| {
            let (nt, _) = split_description(n);
            if nt.eq_ignore_ascii_case(ty) {
                return true;
            }
            match (at, schema.attribute(nt)) {
                (Some(a), Some(want)) => schema.is_subtype_of(a, want.oid()),
                _ => false,
            }
        })
    }
}

fn bool_attr(ty: &str, v: bool) -> Attribute {
    Attribute::new(ty, &[if v { "TRUE" } else { "FALSE" }])
}

impl Directory {
    /// The computed operational attributes `want` names (all of them for
    /// `+`).
    pub(crate) fn computed(
        &self,
        txn: &mut Txn<'_>,
        b: usize,
        e: &DirEntry,
        eff: &Effective,
        want: &dyn Fn(&str) -> bool,
    ) -> OpResult<Vec<Attribute>> {
        let mut out = Vec::new();
        let stored = |n: &str| e.attrs.iter().any(|a| a.ty.eq_ignore_ascii_case(n));
        if want("entryDN") {
            out.push(Attribute::new("entryDN", &[&e.dn]));
        }
        if want("hasSubordinates") || want("numSubordinates") {
            let n = self.backends[b].children(txn, e.id, usize::MAX)?;
            if want("hasSubordinates") {
                out.push(bool_attr("hasSubordinates", n > 0));
            }
            if want("numSubordinates") {
                out.push(Attribute::new("numSubordinates", &[&n.to_string()]));
            }
        }
        if want("subschemaSubentry") {
            out.push(Attribute::new(
                "subschemaSubentry",
                &[crate::backend::schema::DN],
            ));
        }
        if want("aclEntry") && !stored("aclEntry") {
            let vals: Vec<String> = if eff.acl_source.is_none() {
                vec![acl::DEFAULT_ACL.to_string()]
            } else {
                eff.acl.iter().map(ToString::to_string).collect()
            };
            out.push(Attribute {
                ty: "aclEntry".into(),
                vals: vals.into_iter().map(String::into_bytes).collect(),
            });
        }
        if want("aclPropagate") && !stored("aclPropagate") {
            out.push(bool_attr("aclPropagate", eff.acl_propagate));
        }
        if want("aclSource") {
            out.push(Attribute::new(
                "aclSource",
                &[eff.acl_source.as_deref().unwrap_or("default")],
            ));
        }
        if want("entryOwner") && !stored("entryOwner") {
            out.push(Attribute {
                ty: "entryOwner".into(),
                vals: eff
                    .owners
                    .iter()
                    .map(|o| o.to_string().into_bytes())
                    .collect(),
            });
        }
        if want("ownerPropagate") && !stored("ownerPropagate") {
            out.push(bool_attr("ownerPropagate", eff.owner_propagate));
        }
        if want("ownerSource") {
            out.push(Attribute::new(
                "ownerSource",
                &[eff.owner_source.as_deref().unwrap_or("default")],
            ));
        }
        if want("ibm-effectiveAcl") {
            out.push(Attribute {
                ty: "ibm-effectiveAcl".into(),
                vals: eff.acl.iter().map(|a| a.to_string().into_bytes()).collect(),
            });
        }
        if want("ibm-replicationThisServerIsMaster") {
            out.push(bool_attr("ibm-replicationThisServerIsMaster", true));
        }
        if want("ibm-replicationIsQuiesced") {
            out.push(bool_attr("ibm-replicationIsQuiesced", false));
        }
        Ok(out)
    }

    /// The attributes of `attrs` a request with `want` gets: selected,
    /// readable, values dropped for `types_only`. `user_like` treats every
    /// attribute as a user attribute (the root DSE, as IBM's server has
    /// it).
    pub(crate) fn select(
        &self,
        attrs: Vec<Attribute>,
        want: &Wanted,
        types_only: bool,
        readable: &dyn Fn(&AttributeType) -> bool,
        user_like: bool,
    ) -> Vec<Attribute> {
        let mut out: Vec<Attribute> = Vec::new();
        for mut a in attrs {
            if a.vals.is_empty() {
                continue;
            }
            let at = self.schema.attribute(split_description(&a.ty).0);
            let op = !user_like && at.is_some_and(AttributeType::is_operational);
            if !want.wants(&self.schema, &a.ty, op) {
                continue;
            }
            if let Some(at) = at
                && !readable(at)
            {
                continue;
            }
            if types_only {
                a.vals.clear();
            }
            if let Some(prev) = out.iter_mut().find(|p| p.ty.eq_ignore_ascii_case(&a.ty)) {
                prev.vals.extend(a.vals);
            } else {
                out.push(a);
            }
        }
        out
    }

    /// An entry as a search or a read control returns it to `session`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn view(
        &self,
        txn: &mut Txn<'_>,
        cache: &mut AclCache,
        b: usize,
        e: &DirEntry,
        session: &super::Session,
        want: &Wanted,
        types_only: bool,
    ) -> OpResult<Vec<Attribute>> {
        let eff = self.effective_of(txn, cache, b, e)?;
        let key = self.entry_key(&e.dn);
        let comp = self.computed(txn, b, e, &eff, &|n| want.wants(&self.schema, n, true))?;
        let mut all = e.attrs.clone();
        all.extend(comp);
        let readable = |at: &AttributeType| self.attr_rights(&eff, session, &key, at) & acl::R != 0;
        Ok(self.select(all, want, types_only, &readable, false))
    }
}
