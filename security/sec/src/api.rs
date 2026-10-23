// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the security APIs ask the storage service and what it answers:
//! one request carries a [`Query`], so `QSYRUSRI`, `QSYCUSRA`,
//! `QSYGETPH` and the rest share one path. The formats the APIs return
//! are built in `sysapi` from the [`Answer`]. The answers that need only
//! the store are here; profile handles need the job and live in the
//! service.

use ipc::{wire_enum, wire_struct};
use store::obj::header::text_of;
use store::obj::{ObjectId, auth, ops, types};
use store::txn::Txn;

use crate::auth::autl;
use crate::auth::bits::Auth;
use crate::auth::principal::Principal;
use crate::auth::required::Need;
use crate::auth::search;
use crate::profile::index::{self, Kind};
use crate::profile::spcaut::SpecialAuth;
use crate::profile::{groups, objects};
use crate::{Result, SecError};

/// A question an API asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Query {
    /// `QSYRUSRI`: a profile.
    Profile {
        /// The profile, `*CURRENT` for the job's.
        user: String,
    },
    /// `QSYCUSRS`: whether a user has every special authority named.
    Special {
        /// The user.
        user: String,
        /// `*ALLOBJ` ...
        auths: Vec<String>,
    },
    /// `QSYCUSRA`: whether a user has every authority named to an object.
    Check {
        /// The user.
        user: String,
        /// Object, library, type.
        object: (String, String, String),
        /// `*USE`, `*OBJOPR` ...
        auths: Vec<String>,
    },
    /// `QSYRUSRA`: what a user has to an object and from where.
    UserAuth {
        /// The user.
        user: String,
        /// Object, library, type.
        object: (String, String, String),
    },
    /// `QSYLOBJA` (`kind` 0, owned and private) and `QSYLOBJP` (`kind`
    /// 1, primary group): the objects tied to a user.
    Objects {
        /// The user.
        user: String,
        /// 0 authorized, 1 primary group.
        kind: u8,
    },
    /// `QSYRAUTU`: who is authorized to an object.
    Holders {
        /// Object, library, type.
        object: (String, String, String),
    },
    /// `QSYLAUTU`: the profiles, `*ALL`, `*USRPRF` (not groups),
    /// `*GROUP` (groups) or `*MEMBER` (of `group`).
    Users {
        /// The selection.
        select: String,
        /// The group for `*MEMBER`.
        group: String,
    },
    /// `QSYLATLO`: the objects a list secures.
    ListObjects {
        /// The authorization list.
        list: String,
    },
    /// `QSYGETPH`: a profile handle for `user`.
    GetHandle {
        /// The user.
        user: String,
        /// The password, or `*NOPWD`, `*NOPWDCHK`, `*NOPWDSTS`.
        password: String,
    },
    /// `QWTSETP`: swap the job to the handle's user.
    SetHandle {
        /// The handle.
        handle: Vec<u8>,
    },
    /// `QSYRLSPH`: release a handle.
    ReleaseHandle {
        /// The handle.
        handle: Vec<u8>,
    },
    /// `QSYCHGPW`: change a password; the job ran the validation
    /// program already.
    ChangePassword {
        /// The user.
        user: String,
        /// The current password.
        current: String,
        /// The new password.
        new: String,
    },
    /// `QSYRUPWD`: the stored password data, our format.
    GetPassword {
        /// The user.
        user: String,
    },
    /// `QSYSUPWD`: set the stored password data.
    SetPassword {
        /// The user.
        user: String,
        /// What `QSYRUPWD` returned.
        data: Vec<u8>,
    },
}

wire_enum!(Query {
    0 => Profile { user },
    1 => Special { user, auths },
    2 => Check { user, object, auths },
    3 => UserAuth { user, object },
    4 => Objects { user, kind },
    5 => Holders { object },
    6 => Users { select, group },
    7 => ListObjects { list },
    8 => GetHandle { user, password },
    9 => SetHandle { handle },
    10 => ReleaseHandle { handle },
    11 => ChangePassword { user, current, new },
    12 => GetPassword { user },
    13 => SetPassword { user, data },
});

/// One object an answer lists.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObjRow {
    /// Name.
    pub name: String,
    /// Library.
    pub lib: String,
    /// Type, `*FILE`.
    pub ty: String,
    /// Owner.
    pub owner: String,
    /// The authority bits that apply (private, group, or owner's).
    pub bits: u32,
    /// The user owns it.
    pub owned: bool,
}

wire_struct!(ObjRow {
    name,
    lib,
    ty,
    owner,
    bits,
    owned
});

/// One profile an answer lists.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UserRow {
    /// Name, or `*PUBLIC`.
    pub name: String,
    /// It is a group profile.
    pub group: bool,
    /// It has members.
    pub members: bool,
    /// Text.
    pub text: String,
    /// Bits, for `Holders`.
    pub bits: u32,
}

wire_struct!(UserRow {
    name,
    group,
    members,
    text,
    bits
});

/// What a user has to an object.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuthInfo {
    /// The bits.
    pub bits: u32,
    /// The step that decided (`sec::auth::search::Step::name`).
    pub source: String,
    /// The object is an authorization list.
    pub autl: bool,
    /// The list securing it, blank for none.
    pub list: String,
    /// The owner.
    pub owner: String,
    /// The primary group, blank for none.
    pub pgp: String,
}

wire_struct!(AuthInfo {
    bits,
    source,
    autl,
    list,
    owner,
    pgp
});

/// An answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// A profile's payload, encoded (`UsrPrf::encode`).
    Profile {
        /// The payload.
        data: Vec<u8>,
    },
    /// Yes or no.
    Yes {
        /// The answer.
        yes: bool,
    },
    /// An object authority.
    Auth {
        /// What the user has.
        info: AuthInfo,
    },
    /// Objects.
    Objects {
        /// One per object.
        rows: Vec<ObjRow>,
    },
    /// Profiles.
    Users {
        /// One per profile.
        rows: Vec<UserRow>,
    },
    /// A handle or password data.
    Bytes {
        /// The bytes.
        data: Vec<u8>,
    },
    /// Done.
    Done,
}

wire_enum!(Answer {
    0 => Profile { data },
    1 => Yes { yes },
    2 => Auth { info },
    3 => Objects { rows },
    4 => Users { rows },
    5 => Bytes { data },
    6 => Done,
});

fn object_id(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    (name, lib, ty): &(String, String, String),
) -> Result<ObjectId> {
    let code = types::type_by_name(ty)
        .ok_or_else(|| SecError::msg("CPF9827", format!("{ty} is not an object type.")))?
        .code;
    let n = qsname::Name::parse(name)
        .map_err(|_| SecError::msg("CPF9801", format!("{name} not found.")))?;
    let lib_id = if lib.is_empty() || lib == "QSYS" {
        qsys
    } else {
        let l = qsname::Name::parse(lib)
            .map_err(|_| SecError::msg("CPF9810", format!("{lib} not found.")))?;
        ops::find_library(txn, qsys, l)?.id
    };
    let library = store::obj::library::Library::load(txn, lib_id)?;
    library.lookup(txn, code, n)?.ok_or_else(|| {
        SecError::msg(
            "CPF9801",
            format!("Object {name} in {lib} type {ty} not found."),
        )
    })
}

fn row(txn: &mut Txn<'_>, id: ObjectId, bits: u32, owned: bool) -> Result<ObjRow> {
    let h = ops::read_header(txn, id)?.1;
    let lib = if h.id == h.library {
        "QSYS".to_string()
    } else {
        ops::read_header(txn, h.library)?.1.name.to_string()
    };
    Ok(ObjRow {
        name: h.name.to_string(),
        lib,
        ty: types::type_by_code(id.type_code)
            .map_or("*UNKNOWN", |t| t.name)
            .into(),
        owner: text_of(&h.owner),
        bits,
        owned,
    })
}

/// Answer the queries that need only the store and the asking user.
/// `None` for the ones the service answers itself (handles, password
/// changes).
#[allow(clippy::too_many_lines)] // one arm per query
pub fn answer(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    me: &Principal,
    q: &Query,
) -> Result<Option<Answer>> {
    let who = |u: &str| {
        if u == "*CURRENT" || u.is_empty() {
            me.user.clone()
        } else {
            u.to_ascii_uppercase()
        }
    };
    let other = |u: &str| who(u) != me.user;
    Ok(Some(match q {
        Query::Profile { user } => {
            let u = who(user);
            let (id, p) = objects::read(txn, qsys, &u)?;
            if other(user) {
                let h = ops::read_header(txn, id)?.1;
                if !search::check(
                    txn,
                    qsys,
                    me,
                    &h,
                    Need::Bits(Auth(store::obj::header::Authority::READ)),
                )?
                .allowed
                {
                    return Err(SecError::msg(
                        "CPF2217",
                        format!("Not authorized to user profile {u}."),
                    ));
                }
            }
            Answer::Profile { data: p.encode() }
        }
        Query::Special { user, auths } => {
            let p = Principal::load(txn, qsys, &who(user), me.qsecurity)?;
            let refs: Vec<&str> = auths.iter().map(String::as_str).collect();
            let want = SpecialAuth::parse(&refs).map_err(|e| SecError::msg("CPF2209", e))?;
            Answer::Yes {
                yes: p.special().has(want),
            }
        }
        Query::Check {
            user,
            object,
            auths,
        } => {
            let p = if other(user) {
                Principal::load(txn, qsys, &who(user), me.qsecurity)?
            } else {
                me.clone()
            };
            let id = object_id(txn, qsys, object)?;
            let h = ops::read_header(txn, id)?.1;
            let refs: Vec<&str> = auths.iter().map(String::as_str).collect();
            let need = if refs == ["*ANY"] {
                Need::Any
            } else {
                Need::Bits(
                    Auth::parse(&refs, id.type_code == types::AUTL)
                        .map_err(|e| SecError::msg("CPF2209", e))?,
                )
            };
            Answer::Yes {
                yes: search::check(txn, qsys, &p, &h, need)?.allowed,
            }
        }
        Query::UserAuth { user, object } => {
            let p = Principal::load(txn, qsys, &who(user), me.qsecurity)?;
            let id = object_id(txn, qsys, object)?;
            let h = ops::read_header(txn, id)?.1;
            let d = search::check(txn, qsys, &p, &h, Need::Bits(Auth::EXCLUDE))?;
            let found = search::find(txn, qsys, &p, &h)?;
            Answer::Auth {
                info: AuthInfo {
                    bits: found.have.0,
                    source: d.step.name().into(),
                    autl: id.type_code == types::AUTL,
                    list: text_of(&h.autl),
                    owner: text_of(&h.owner),
                    pgp: text_of(&h.primary_group),
                },
            }
        }
        Query::Objects { user, kind } => {
            let u = who(user);
            if other(user) {
                me.require(SpecialAuth::SECADM, "list another user's objects")?;
            }
            let mut rows = Vec::new();
            if *kind == 1 {
                for (id, bits) in index::list(txn, qsys, &u, Kind::Group)? {
                    rows.push(row(txn, id, u32::try_from(bits).unwrap_or(0), false)?);
                }
            } else {
                for (id, _) in index::list(txn, qsys, &u, Kind::Owned)? {
                    let h = ops::read_header(txn, id)?.1;
                    rows.push(row(txn, id, h.owner_authority.0, true)?);
                }
                for (id, bits) in index::list(txn, qsys, &u, Kind::Private)? {
                    rows.push(row(txn, id, u32::try_from(bits).unwrap_or(0), false)?);
                }
            }
            Answer::Objects { rows }
        }
        Query::Holders { object } => {
            let id = object_id(txn, qsys, object)?;
            let h = ops::read_header(txn, id)?.1;
            let mut v = vec![UserRow {
                name: text_of(&h.owner),
                bits: h.owner_authority.0,
                ..UserRow::default()
            }];
            let pgp = text_of(&h.primary_group);
            if !pgp.is_empty() {
                v.push(UserRow {
                    name: pgp,
                    group: true,
                    bits: h.group_authority.0,
                    ..UserRow::default()
                });
            }
            for pa in auth::private_list(txn, &h)? {
                v.push(UserRow {
                    name: text_of(&pa.profile),
                    bits: pa.bits.0,
                    ..UserRow::default()
                });
            }
            v.push(UserRow {
                name: "*PUBLIC".into(),
                bits: h.public_authority.0,
                ..UserRow::default()
            });
            Answer::Users { rows: v }
        }
        Query::Users { select, group } => {
            let mut v = Vec::new();
            for n in objects::list(txn, qsys)? {
                let (_, p) = objects::read(txn, qsys, &n)?;
                let members = groups::is_group(txn, qsys, &n)?;
                let keep = match select.as_str() {
                    "*USRPRF" => !members,
                    "*GROUP" => members,
                    "*MEMBER" => p.groups().iter().any(|g| g == group),
                    _ => true,
                };
                if keep {
                    v.push(UserRow {
                        name: n,
                        group: members,
                        members,
                        text: p.text,
                        bits: 0,
                    });
                }
            }
            Answer::Users { rows: v }
        }
        Query::ListObjects { list } => {
            let mut rows = Vec::new();
            for id in autl::secured(txn, qsys, list)? {
                rows.push(row(txn, id, 0, false)?);
            }
            Answer::Objects { rows }
        }
        Query::GetPassword { user } => {
            me.require(
                SpecialAuth::ALLOBJ.union(SpecialAuth::SECADM),
                "retrieve password data",
            )?;
            let p = objects::read(txn, qsys, user)?.1;
            Answer::Bytes {
                data: password_blob(&p),
            }
        }
        Query::SetPassword { user, data } => {
            me.require(
                SpecialAuth::ALLOBJ.union(SpecialAuth::SECADM),
                "set password data",
            )?;
            let (id, mut p) = objects::read(txn, qsys, user)?;
            p.hashes = blob_hashes(data)?;
            p.pwd_none = p.hashes.is_empty();
            objects::write(txn, id, &p)?;
            Answer::Done
        }
        Query::GetHandle { .. }
        | Query::SetHandle { .. }
        | Query::ReleaseHandle { .. }
        | Query::ChangePassword { .. } => {
            return Ok(None);
        }
    }))
}

/// Magic at the start of `QSYRUPWD`'s data.
pub const PWD_MAGIC: &[u8; 8] = b"QS400PWD";

/// A profile's password data: the magic, then each hash as a form byte,
/// a two-byte length and the PHC string. Not IBM's format.
#[must_use]
pub fn password_blob(p: &crate::profile::UsrPrf) -> Vec<u8> {
    let mut b = PWD_MAGIC.to_vec();
    for h in &p.hashes {
        b.push(match h.form {
            crate::profile::PwdForm::Short => 0,
            crate::profile::PwdForm::Long => 1,
            crate::profile::PwdForm::DesToken => 2,
            crate::profile::PwdForm::ShaToken => 3,
        });
        b.extend_from_slice(&u16::try_from(h.phc.len()).unwrap_or(0).to_le_bytes());
        b.extend_from_slice(h.phc.as_bytes());
    }
    b
}

fn blob_hashes(b: &[u8]) -> Result<Vec<crate::profile::PwdHash>> {
    let bad = || SecError::msg("CPF22C8", "The password data is not valid.");
    let rest = b.strip_prefix(PWD_MAGIC.as_slice()).ok_or_else(bad)?;
    let mut out = Vec::new();
    let mut at = 0;
    while at < rest.len() {
        let form = *rest.get(at).ok_or_else(bad)?;
        let n = rest
            .get(at + 1..at + 3)
            .map(|l| usize::from(u16::from_le_bytes([l[0], l[1]])))
            .ok_or_else(bad)?;
        let phc = rest.get(at + 3..at + 3 + n).ok_or_else(bad)?;
        out.push(crate::profile::PwdHash {
            form: match form {
                0 => crate::profile::PwdForm::Short,
                2 => crate::profile::PwdForm::DesToken,
                3 => crate::profile::PwdForm::ShaToken,
                _ => crate::profile::PwdForm::Long,
            },
            phc: String::from_utf8_lossy(phc).into_owned(),
        });
        at += 3 + n;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_and_blob() {
        let q = Query::Check {
            user: "BOB".into(),
            object: ("F".into(), "L".into(), "*FILE".into()),
            auths: vec!["*USE".into()],
        };
        let back: Query = ipc::decode(&ipc::encode(&q)).unwrap();
        assert_eq!(back, q);
        let mut p = crate::profile::UsrPrf::new("BOB", 40);
        p.hashes = crate::password::hashes_for("Secret12", 3);
        let blob = password_blob(&p);
        assert_eq!(blob_hashes(&blob).unwrap(), p.hashes);
        assert!(blob_hashes(b"nope").is_err());
    }
}
