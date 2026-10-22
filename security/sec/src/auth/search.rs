// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The authority search, in IBM's order. The first source that has an
//! entry for the user decides (even when it grants less than `*PUBLIC`
//! would); groups add up; `*PUBLIC` counts only when neither the user
//! nor any group has an entry; adopted authority is added last.
//! `docs/SECURITY.md` has the step table.

use store::obj::ObjectId;
use store::obj::header::{Authority as A, ObjectHeader};
use store::txn::Txn;

use crate::Result;
use crate::auth::autl;
use crate::auth::bits::Auth;
use crate::auth::grant;
use crate::auth::principal::{Adopted, Principal};
use crate::auth::required::Need;
use crate::profile::spcaut::SpecialAuth;
use crate::rec::text_of;

/// The source that decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Step {
    /// The user has `*ALLOBJ`.
    AllObj,
    /// The user owns the object.
    Owner,
    /// The user's private authority.
    Private,
    /// The user's entry on the authorization list.
    Autl,
    /// A group has `*ALLOBJ`.
    GroupAllObj,
    /// A group owns the object.
    GroupOwner,
    /// A group is the object's primary group.
    PrimaryGroup,
    /// A group's private authority.
    GroupPrivate,
    /// A group's entry on the authorization list.
    GroupAutl,
    /// `*PUBLIC` on the object.
    Public,
    /// `*PUBLIC` of the authorization list.
    PublicAutl,
    /// Adopted authority (with what came before).
    Adopted,
    /// Nothing found.
    None,
}

impl Step {
    /// A short name for displays and audit entries.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::AllObj => "*ALLOBJ",
            Self::Owner => "OWNER",
            Self::Private => "PRIVATE",
            Self::Autl => "AUTL",
            Self::GroupAllObj => "GROUP *ALLOBJ",
            Self::GroupOwner => "GROUP OWNER",
            Self::PrimaryGroup => "PRIMARY GROUP",
            Self::GroupPrivate => "GROUP PRIVATE",
            Self::GroupAutl => "GROUP AUTL",
            Self::Public => "*PUBLIC",
            Self::PublicAutl => "*PUBLIC AUTL",
            Self::Adopted => "ADOPTED",
            Self::None => "NONE",
        }
    }
}

/// What the user and its groups have, before adoption: cacheable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Found {
    /// The authorities.
    pub have: Auth,
    /// Where they came from.
    pub step: Step,
    /// `*ALLOBJ` decided: everything is allowed.
    pub all: bool,
}

/// A decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decision {
    /// Allowed.
    pub allowed: bool,
    /// The step that decided.
    pub step: Step,
    /// The authorities found, adoption included.
    pub have: Auth,
}

/// One profile's own entry on the object: owner, private, list. `None`
/// when it has none.
fn own_entry(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    h: &ObjectHeader,
    who: &str,
) -> Result<Option<(Auth, Step)>> {
    if text_of(&h.owner) == who {
        return Ok(Some((Auth::of(h.owner_authority), Step::Owner)));
    }
    if let Some(b) = grant::private(txn, h, who)? {
        return Ok(Some((b, Step::Private)));
    }
    if let Some(b) = autl_entry(txn, qsys, h, who)? {
        return Ok(Some((b, Step::Autl)));
    }
    Ok(None)
}

/// `who`'s entry on the list securing `h`, if any.
fn autl_entry(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    h: &ObjectHeader,
    who: &str,
) -> Result<Option<Auth>> {
    let list = text_of(&h.autl);
    if list.is_empty() {
        return Ok(None);
    }
    let Ok(id) = autl::find(txn, qsys, &list) else {
        return Ok(None);
    };
    let lh = grant::header(txn, id)?;
    if text_of(&lh.owner) == who {
        return Ok(Some(Auth::of(lh.owner_authority)));
    }
    // A list entry grants object authorities, never *AUTLMGT.
    Ok(grant::private(txn, &lh, who)?.map(|b| Auth(b.0 & Auth::ALL.0)))
}

/// Steps 1 to 5 for the user and its groups.
pub fn find(txn: &mut Txn<'_>, qsys: ObjectId, p: &Principal, h: &ObjectHeader) -> Result<Found> {
    let all = Auth(Auth::MASK);
    if SpecialAuth(p.spcaut).has(SpecialAuth::ALLOBJ) || (p.qsecurity != 0 && p.qsecurity <= 20) {
        return Ok(Found {
            have: all,
            step: Step::AllObj,
            all: true,
        });
    }
    if let Some((have, step)) = own_entry(txn, qsys, h, &p.user)? {
        return Ok(Found {
            have,
            step,
            all: false,
        });
    }
    if SpecialAuth(p.group_spcaut).has(SpecialAuth::ALLOBJ) {
        return Ok(Found {
            have: all,
            step: Step::GroupAllObj,
            all: true,
        });
    }
    let pgp = text_of(&h.primary_group);
    let mut group: Option<(Auth, Step)> = None;
    for g in &p.groups {
        let entry = if pgp == *g {
            Some((Auth::of(h.group_authority), Step::PrimaryGroup))
        } else {
            own_entry(txn, qsys, h, g)?.map(|(a, s)| {
                let s = match s {
                    Step::Owner => Step::GroupOwner,
                    Step::Private => Step::GroupPrivate,
                    _ => Step::GroupAutl,
                };
                (a, s)
            })
        };
        if let Some((a, s)) = entry {
            group = Some(match group {
                Some((acc, first)) => (acc.union(a), first),
                None => (a, s),
            });
        }
    }
    if let Some((have, step)) = group {
        return Ok(Found {
            have,
            step,
            all: false,
        });
    }
    if h.public_authority.0 & A::FROM_AUTL != 0 {
        let list = text_of(&h.autl);
        if let Ok(id) = autl::find(txn, qsys, &list) {
            let lh = grant::header(txn, id)?;
            let have = Auth(Auth::of(lh.public_authority).0 & Auth::ALL.0);
            return Ok(Found {
                have,
                step: Step::PublicAutl,
                all: false,
            });
        }
        return Ok(Found {
            have: Auth::EXCLUDE,
            step: Step::None,
            all: false,
        });
    }
    Ok(Found {
        have: Auth::of(h.public_authority),
        step: Step::Public,
        all: false,
    })
}

/// What one adopted owner has: its `*ALLOBJ`, or its own entry. The
/// owner's groups are not used (IBM: adopted authority is the owner's
/// own).
pub fn adopted(txn: &mut Txn<'_>, qsys: ObjectId, a: &Adopted, h: &ObjectHeader) -> Result<Auth> {
    if SpecialAuth(a.spcaut).has(SpecialAuth::ALLOBJ) {
        return Ok(Auth(Auth::MASK));
    }
    Ok(own_entry(txn, qsys, h, &a.owner)?.map_or(Auth::EXCLUDE, |(b, _)| b))
}

/// Whether `have` satisfies `need`; the owner always counts as having
/// some authority.
#[must_use]
pub fn satisfies(have: Auth, need: Need, all: bool) -> bool {
    all || match need {
        Need::Any => have.0 != 0,
        Need::Bits(b) => have.has(b),
    }
}

/// The whole search: `found` from steps 1 to 5 (cached or fresh), then
/// adoption.
pub fn decide(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    p: &Principal,
    h: &ObjectHeader,
    found: Found,
    need: Need,
) -> Result<Decision> {
    if satisfies(found.have, need, found.all) {
        return Ok(Decision {
            allowed: true,
            step: found.step,
            have: found.have,
        });
    }
    let mut have = found.have;
    for a in &p.adopted {
        have = have.union(adopted(txn, qsys, a, h)?);
        if satisfies(have, need, false) {
            return Ok(Decision {
                allowed: true,
                step: Step::Adopted,
                have,
            });
        }
    }
    Ok(Decision {
        allowed: false,
        step: found.step,
        have,
    })
}

/// The search without a cache.
pub fn check(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    p: &Principal,
    h: &ObjectHeader,
    need: Need,
) -> Result<Decision> {
    let found = find(txn, qsys, p, h)?;
    decide(txn, qsys, p, h, found, need)
}
