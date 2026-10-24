// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Authority edits: `CHGAUT`, `DSPAUT`/`WRKAUT`'s list, `CHGOWN`,
//! `CHGPGP`, and `chown()` by UID and GID.

use sec::auth::bits::Auth;
use sec::profile::spcaut::SpecialAuth;

use super::audit::AuditKind;
use super::{Ctx, Ifs};
use crate::auth::data::{self, DataAuth};
use crate::auth::need::Op;
use crate::backend::Backend;
use crate::errno::{Errno, Result};
use crate::meta::Meta;

/// Whose authority `CHGAUT` changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Who {
    /// `*PUBLIC`.
    Public,
    /// A user or group profile.
    User(String),
}

/// `CHGAUT`'s parameters.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChgAut {
    /// `USER`.
    pub who: Vec<Who>,
    /// `DTAAUT`: `None` is `*SAME`.
    pub data: Option<DataAuth>,
    /// `DTAAUT(*AUTL)`: `*PUBLIC` from the authorization list.
    pub data_autl: bool,
    /// `OBJAUT`: `None` is `*SAME`; an empty set is `*NONE`.
    pub objects: Option<Auth>,
    /// `AUTL`: `None` is `*SAME`, `Some(None)` is `*NONE`.
    pub autl: Option<Option<String>>,
}

/// One row of `DSPAUT`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthEntry {
    /// `*PUBLIC`, a profile, or the owner or group's name.
    pub who: String,
    /// What the row is: `*PUBLIC`, owner, primary group or private.
    pub role: Role,
    /// Data authority name (`*RWX`, `*AUTL`, `USER DEF` ...).
    pub data: String,
    /// The object authorities.
    pub objects: Auth,
    /// The whole set.
    pub auth: Auth,
}

/// A row's role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// `*PUBLIC`.
    Public,
    /// The owner.
    Owner,
    /// The primary group.
    Group,
    /// A private authority.
    Private,
}

/// `DSPAUT`'s answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthList {
    /// The object's path.
    pub path: String,
    /// Its type.
    pub objtype: String,
    /// Owner.
    pub owner: String,
    /// Primary group.
    pub group: Option<String>,
    /// Authorization list.
    pub autl: Option<String>,
    /// Rows: `*PUBLIC`, owner, primary group, then privates by name.
    pub entries: Vec<AuthEntry>,
}

fn entry(who: &str, role: Role, a: Auth) -> AuthEntry {
    AuthEntry {
        who: who.to_string(),
        role,
        data: data::data_name(a).to_string(),
        objects: data::objects(a),
        auth: a,
    }
}

/// The rows a record shows.
fn rows(m: &Meta) -> Vec<AuthEntry> {
    let mut v = Vec::new();
    let mut public = entry("*PUBLIC", Role::Public, m.public);
    if m.public_from_autl {
        public.data = "*AUTL".into();
    }
    v.push(public);
    v.push(entry(&m.owner, Role::Owner, m.owner_auth));
    if let Some(g) = &m.group {
        v.push(entry(g, Role::Group, m.group_auth));
    }
    for (who, a) in &m.private {
        v.push(entry(who, Role::Private, *a));
    }
    v
}

impl<B: Backend> Ifs<B> {
    /// `DSPAUT`: the full list for the owner, `*ALLOBJ` or `*OBJMGT`;
    /// otherwise only `*PUBLIC` and the caller's own row.
    pub fn dspaut(&self, cx: &Ctx<'_>, p: &str) -> Result<AuthList> {
        let (r, m) = self.resolve_meta(cx, p, true)?;
        let mut entries = rows(&m);
        if !cx.meets(
            &m,
            crate::auth::need::ObjNeed::OwnerOr(crate::auth::need::OBJMGT),
        ) {
            entries.retain(|e| e.role == Role::Public || e.who == cx.principal.user);
        }
        Ok(AuthList {
            path: r.path(),
            objtype: r.node.objtype(),
            owner: m.owner.clone(),
            group: m.group.clone(),
            autl: m.autl.clone(),
            entries,
        })
    }

    /// Apply a change to a record under the state lock, auditing `kind`.
    fn edit(
        &self,
        cx: &Ctx<'_>,
        p: &str,
        op: Op,
        kind: AuditKind,
        oper: &'static str,
        f: impl FnOnce(&mut Meta, &Ctx<'_>) -> Result<String>,
    ) -> Result<()> {
        let mut st = self.lock();
        let cwd = Self::cwd_of(&mut st, cx.owner);
        let (mut sys, _) = self.split(&mut st);
        let r = sys.resolve(&cwd, p, true, Some(cx))?;
        let (mut m, _) = sys.meta(&r.node, Some(cx))?;
        cx.require(&m, &r.node, &r.path(), op)?;
        if Self::read_only(&sys, r.node.fs()) {
            return Err(Errno::EROFS);
        }
        let detail = f(&mut m, cx)?;
        self.put_meta(cx, &r.node, &m)?;
        cx.audit_always(kind, &m, &r.node, &r.path(), oper, detail);
        Ok(())
    }

    /// `CHGAUT`. A caller that is neither the owner nor `*ALLOBJ` (it has
    /// `*OBJMGT`) may give only authorities it has, and may not change
    /// the authorization list.
    pub fn chgaut(&self, cx: &Ctx<'_>, p: &str, c: &ChgAut) -> Result<()> {
        if c.data_autl && c.who.iter().any(|w| *w != Who::Public) {
            return Err(Errno::EINVAL);
        }
        let c = c.clone();
        self.edit(
            cx,
            p,
            Op::ChgAut,
            AuditKind::AuthChange,
            "CHGAUT",
            move |m, cx| {
                let privileged = crate::auth::owner_or_allobj(cx.principal, m);
                let have = crate::auth::have(cx.principal, m, cx.env);
                if let Some(list) = &c.autl {
                    if !privileged {
                        return Err(Errno::EPERM);
                    }
                    if let Some(name) = list
                        && cx.env.autl(name).is_none()
                    {
                        return Err(Errno::ENOENT);
                    }
                    m.autl.clone_from(list);
                    if m.autl.is_none() {
                        m.public_from_autl = false;
                    }
                }
                for w in &c.who {
                    let cur = match w {
                        Who::Public => m.public,
                        Who::User(u) if *u == m.owner => m.owner_auth,
                        Who::User(u) if m.group.as_deref() == Some(u.as_str()) => m.group_auth,
                        Who::User(u) => {
                            if !cx.env.profile_exists(u) {
                                return Err(Errno::EINVAL);
                            }
                            m.private_of(u).unwrap_or(Auth::EXCLUDE)
                        }
                    };
                    let mut new = cur;
                    if let Some(d) = c.data {
                        new = d.with_objects(new);
                    }
                    if let Some(o) = c.objects {
                        new = Auth(data::DataAuth::of(new).auth().0 | (o.0 & data::OBJECT));
                    }
                    if !privileged && !have.has(Auth(new.0 & !cur.0)) {
                        return Err(Errno::EPERM);
                    }
                    match w {
                        Who::Public => {
                            m.public = new;
                            if c.data_autl {
                                if m.autl.is_none() {
                                    return Err(Errno::EINVAL);
                                }
                                m.public_from_autl = true;
                            } else if c.data.is_some() {
                                m.public_from_autl = false;
                            }
                        }
                        Who::User(u) if *u == m.owner => m.owner_auth = new,
                        Who::User(u) if m.group.as_deref() == Some(u.as_str()) => {
                            m.group_auth = new;
                        }
                        Who::User(u) => m.set_private(u, Some(new)),
                    }
                }
                Ok(String::new())
            },
        )
    }

    /// Remove a private authority (`RVKOBJAUT`-style, `CHGAUT ... *NONE
    /// *NONE` refused by IBM): the owner, `*ALLOBJ` or `*OBJMGT`.
    pub fn rvkaut(&self, cx: &Ctx<'_>, p: &str, user: &str) -> Result<()> {
        let user = user.to_string();
        self.edit(
            cx,
            p,
            Op::ChgAut,
            AuditKind::AuthChange,
            "RVKAUT",
            move |m, _| {
                if m.private_of(&user).is_none() {
                    return Err(Errno::ENOENT);
                }
                m.set_private(&user, None);
                Ok(user)
            },
        )
    }

    /// `CHGOWN`: the new owner gets the owner's authority (and loses its
    /// private one); with `revoke_old` the old owner keeps nothing,
    /// otherwise its authority becomes private.
    pub fn chgown(&self, cx: &Ctx<'_>, p: &str, new_owner: &str, revoke_old: bool) -> Result<()> {
        if !cx.env.profile_exists(new_owner) {
            return Err(Errno::EINVAL);
        }
        let new_owner = new_owner.to_string();
        self.edit(cx, p, Op::Chown, AuditKind::Owner, "CHGOWN", move |m, _| {
            if m.owner == new_owner {
                return Ok(new_owner);
            }
            let old = std::mem::replace(&mut m.owner, new_owner.clone());
            m.set_private(&new_owner, None);
            if !revoke_old {
                m.set_private(&old, Some(m.owner_auth));
            }
            if m.group.as_deref() == Some(new_owner.as_str()) {
                m.group = None;
                m.group_auth = Auth::EXCLUDE;
            }
            Ok(format!("{old} {new_owner}"))
        })
    }

    /// `CHGPGP`: a group profile (not the owner) or `None`. A private
    /// authority the new group had becomes its group authority, else it
    /// takes the old group's; the old group keeps its authority
    /// privately unless `revoke_old`. Without `*ALLOBJ` the caller must
    /// be the owner and a member of the new group.
    pub fn chgpgp(
        &self,
        cx: &Ctx<'_>,
        p: &str,
        new_group: Option<&str>,
        revoke_old: bool,
    ) -> Result<()> {
        if let Some(g) = new_group
            && !cx.env.is_group(g)
        {
            return Err(Errno::EINVAL);
        }
        let new_group = new_group.map(str::to_string);
        self.edit(
            cx,
            p,
            Op::Chgrp,
            AuditKind::Group,
            "CHGPGP",
            move |m, cx| {
                if let Some(g) = &new_group {
                    if *g == m.owner {
                        return Err(Errno::EINVAL);
                    }
                    if !cx.principal.has(SpecialAuth::ALLOBJ) && !cx.principal.groups.contains(g) {
                        return Err(Errno::EPERM);
                    }
                }
                if m.group == new_group {
                    return Ok(String::new());
                }
                let old = m.group.take();
                let old_auth = m.group_auth;
                if let Some(o) = &old
                    && !revoke_old
                {
                    m.set_private(o, Some(old_auth));
                }
                if let Some(g) = &new_group {
                    m.group_auth = m.private_of(g).unwrap_or(old_auth);
                    m.set_private(g, None);
                }
                m.group.clone_from(&new_group);
                Ok(new_group.clone().unwrap_or_else(|| "*NONE".into()))
            },
        )
    }

    /// `chown()`: by UID and GID (`u32::MAX` leaves one alone). The old
    /// owner's and group's authorities are not kept (POSIX).
    pub fn chown(&self, cx: &Ctx<'_>, p: &str, uid: u32, gid: u32) -> Result<()> {
        if uid != u32::MAX {
            let owner = cx.env.profile_of_uid(uid).ok_or(Errno::EINVAL)?;
            self.chgown(cx, p, &owner, true)?;
        }
        if gid != u32::MAX {
            let group = if gid == 0 {
                None
            } else {
                Some(cx.env.profile_of_gid(gid).ok_or(Errno::EINVAL)?)
            };
            self.chgpgp(cx, p, group.as_deref(), true)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_order() {
        let mut m = Meta::new("BOB", 1208, 0);
        m.group = Some("DEV".into());
        m.group_auth = data::RX;
        m.set_private("ZED", Some(Auth::EXCLUDE));
        m.set_private("ANN", Some(data::R));
        let r = rows(&m);
        let who: Vec<&str> = r.iter().map(|e| e.who.as_str()).collect();
        assert_eq!(who, vec!["*PUBLIC", "BOB", "DEV", "ANN", "ZED"]);
        assert_eq!(r[1].data, "*RWX");
        assert_eq!(r[2].data, "*RX");
        assert_eq!(r[4].data, "*EXCLUDE");
    }
}
