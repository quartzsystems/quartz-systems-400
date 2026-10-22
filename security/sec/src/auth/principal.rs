// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Who is asking: a user, its groups and special authorities, and the
//! owners whose authority the call stack adopts. The storage service
//! keeps one per job and refreshes it when a profile changes.

use ipc::wire_struct;
use store::obj::ObjectId;
use store::txn::Txn;

use crate::profile::objects;
use crate::profile::spcaut::SpecialAuth;
use crate::{Result, SecError};

/// An adopted owner: the program that adopts and its owner's authorities.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Adopted {
    /// The adopting program, `lib/name`.
    pub program: String,
    /// The owner profile.
    pub owner: String,
    /// The owner's groups.
    pub groups: Vec<String>,
    /// The owner's special authorities, with its groups'.
    pub spcaut: u32,
}

wire_struct!(Adopted {
    program,
    owner,
    groups,
    spcaut
});

/// A user with everything the authority search needs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Principal {
    /// The user profile.
    pub user: String,
    /// Group profiles in search order: `GRPPRF`, then `SUPGRPPRF`.
    pub groups: Vec<String>,
    /// The user's own special authorities.
    pub spcaut: u32,
    /// The groups' special authorities together.
    pub group_spcaut: u32,
    /// `QSECURITY`.
    pub qsecurity: u8,
    /// Owners the call stack adopts, nearest first.
    pub adopted: Vec<Adopted>,
}

wire_struct!(Principal {
    user,
    groups,
    spcaut,
    group_spcaut,
    qsecurity,
    adopted
});

impl Principal {
    /// Load `user` and its groups from the store.
    pub fn load(txn: &mut Txn<'_>, qsys: ObjectId, user: &str, qsecurity: u8) -> Result<Self> {
        let (_, p) = objects::read(txn, qsys, user)?;
        let groups = p.groups();
        let mut group_spcaut = SpecialAuth::NONE;
        for g in &groups {
            if let Ok((_, gp)) = objects::read(txn, qsys, g) {
                group_spcaut = group_spcaut.union(gp.spcaut);
            }
        }
        Ok(Self {
            user: p.name,
            groups,
            spcaut: p.spcaut.0,
            group_spcaut: group_spcaut.0,
            qsecurity,
            adopted: Vec::new(),
        })
    }

    /// An owner's authorities for adoption by `program`.
    pub fn adopt(txn: &mut Txn<'_>, qsys: ObjectId, owner: &str, program: &str) -> Result<Adopted> {
        let o = Self::load(txn, qsys, owner, 0)?;
        Ok(Adopted {
            program: program.into(),
            owner: o.user,
            groups: o.groups,
            spcaut: o.spcaut | o.group_spcaut,
        })
    }

    /// The special authorities in effect: the user's, its groups', and
    /// at level 20 and below `*ALLOBJ` and `*SAVSYS` for everyone.
    #[must_use]
    pub fn special(&self) -> SpecialAuth {
        let mut s = SpecialAuth(self.spcaut | self.group_spcaut);
        if self.qsecurity != 0 && self.qsecurity <= 20 {
            s = s.union(SpecialAuth::ALLOBJ).union(SpecialAuth::SAVSYS);
        }
        s
    }

    /// [`Principal::special`] with what adoption adds.
    #[must_use]
    pub fn special_adopted(&self) -> SpecialAuth {
        self.adopted
            .iter()
            .fold(self.special(), |s, a| s.union(SpecialAuth(a.spcaut)))
    }

    /// Whether the user (with groups and adoption) has `a`.
    #[must_use]
    pub fn has(&self, a: SpecialAuth) -> bool {
        self.special_adopted().has(a)
    }

    /// Require special authority `a`, or `CPF2222`-style refusal.
    pub fn require(&self, a: SpecialAuth, what: &str) -> Result<()> {
        if self.has(a) {
            Ok(())
        } else {
            Err(SecError::msg(
                "CPF2222",
                format!(
                    "{} special authority is needed to {what}.",
                    a.names().join(" ")
                ),
            ))
        }
    }

    /// Whether this is `name` or one of its groups.
    #[must_use]
    pub fn is_or_member(&self, name: &str) -> bool {
        self.user == name || self.groups.iter().any(|g| g == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn special_with_groups_and_levels() {
        let mut p = Principal {
            user: "BOB".into(),
            groups: vec!["OPS".into()],
            spcaut: SpecialAuth::SPLCTL.0,
            group_spcaut: SpecialAuth::JOBCTL.0,
            qsecurity: 40,
            adopted: Vec::new(),
        };
        assert!(p.has(SpecialAuth::JOBCTL));
        assert!(!p.has(SpecialAuth::ALLOBJ));
        assert!(p.require(SpecialAuth::SECADM, "create profiles").is_err());
        p.adopted.push(Adopted {
            program: "APP/PGM".into(),
            owner: "QSECOFR".into(),
            groups: Vec::new(),
            spcaut: SpecialAuth::ALL.0,
        });
        assert!(p.has(SpecialAuth::ALLOBJ));
        assert!(!p.special().has(SpecialAuth::ALLOBJ));
        p.qsecurity = 20;
        assert!(p.special().has(SpecialAuth::ALLOBJ));
        let back: Principal = ipc::decode(&ipc::encode(&p)).unwrap();
        assert_eq!(back, p);
        assert!(p.is_or_member("OPS"));
    }
}
