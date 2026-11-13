// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Calls made for someone else: a server job's thread runs IFS calls as
//! the user a request names (`IfsClient::as_caller`), without changing
//! its job's identity. The storage service honours it only for the
//! server jobs it trusts. Who a [`Caller`] is comes from the profiles:
//! [`resolve`] has the one set of rules, [`PrincipalCache`] keeps the
//! principals built so they are built once per change of the profiles.
//! Authority is the profile's and its group profiles', never anything
//! the client sent; auditing names the profile.

use std::collections::HashMap;

use ipc::wire_enum;
use sec::auth::principal::Principal;

use crate::auth::AuthEnv;
use crate::errno::{Errno, Result};

/// Whom a call is made for.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Caller {
    /// The user profile with this UID.
    Uid {
        /// The UID.
        uid: u32,
    },
    /// The user profile with this name.
    Profile {
        /// The name.
        name: String,
    },
    /// The system's anonymous profile (the service names it).
    Anonymous,
}

wire_enum!(Caller {
    0 => Uid { uid },
    1 => Profile { name },
    2 => Anonymous,
});

/// The profile a [`Caller`] is: the profile with the UID (whatever its
/// status: a `*DISABLED` profile still maps, as only sign-on checks the
/// status), the named profile, or `anonymous`.
///
/// # Errors
///
/// `ENOTENROLL` when no profile has the UID or name, or there is no
/// anonymous profile: the caller then decides (a file server maps the
/// request to its anonymous user or refuses it).
pub fn resolve(who: &Caller, env: &dyn AuthEnv, anonymous: Option<&str>) -> Result<String> {
    match who {
        Caller::Uid { uid } => env.profile_of_uid(*uid).ok_or(Errno::ENOTENROLL),
        Caller::Profile { name } => {
            let n = name.trim().to_ascii_uppercase();
            if env.profile_exists(&n) {
                Ok(n)
            } else {
                Err(Errno::ENOTENROLL)
            }
        }
        Caller::Anonymous => anonymous
            .filter(|n| env.profile_exists(n))
            .map(str::to_string)
            .ok_or(Errno::ENOTENROLL),
    }
}

/// Principals by profile name, kept while the profiles do not change:
/// `stamp` is whatever changes with any profile, group membership or
/// authorization list (the service passes its security epoch and its
/// count of profile writes).
#[derive(Debug, Default)]
pub struct PrincipalCache {
    stamp: Option<(u64, u64)>,
    by_name: HashMap<String, Principal>,
    loads: u64,
}

/// Principals kept at most; past it the cache starts again.
const CAP: usize = 4096;

impl PrincipalCache {
    /// The principal of `name` at `stamp`, built by `load` when it is
    /// not kept.
    ///
    /// # Errors
    ///
    /// `load`'s.
    pub fn get(
        &mut self,
        stamp: (u64, u64),
        name: &str,
        load: impl FnOnce(&str) -> Result<Principal>,
    ) -> Result<Principal> {
        if self.stamp != Some(stamp) || self.by_name.len() >= CAP {
            self.by_name.clear();
            self.stamp = Some(stamp);
        }
        if let Some(p) = self.by_name.get(name) {
            return Ok(p.clone());
        }
        let p = load(name)?;
        self.loads += 1;
        self.by_name.insert(name.to_string(), p.clone());
        Ok(p)
    }

    /// How many principals were built (for tests).
    #[must_use]
    pub fn loads(&self) -> u64 {
        self.loads
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::MemEnv;

    fn env() -> MemEnv {
        MemEnv::default()
            .user("QSECOFR", 0)
            .user("BOB", 1000)
            .user("QNFSANON", 4_294_770_710)
    }

    #[test]
    fn the_rules() {
        let e = env();
        let r = |w: Caller, a| resolve(&w, &e, a);
        assert_eq!(r(Caller::Uid { uid: 1000 }, None).as_deref(), Ok("BOB"));
        assert_eq!(r(Caller::Uid { uid: 0 }, None).as_deref(), Ok("QSECOFR"));
        assert_eq!(r(Caller::Uid { uid: 7 }, None), Err(Errno::ENOTENROLL));
        assert_eq!(
            r(
                Caller::Profile {
                    name: " bob ".into()
                },
                None
            )
            .as_deref(),
            Ok("BOB")
        );
        assert_eq!(
            r(
                Caller::Profile {
                    name: "NOBODY".into()
                },
                None
            ),
            Err(Errno::ENOTENROLL)
        );
        assert_eq!(
            r(Caller::Anonymous, Some("QNFSANON")).as_deref(),
            Ok("QNFSANON")
        );
        assert_eq!(r(Caller::Anonymous, None), Err(Errno::ENOTENROLL));
        assert_eq!(r(Caller::Anonymous, Some("GONE")), Err(Errno::ENOTENROLL));
    }

    #[test]
    fn principals_are_built_once_per_stamp() {
        let mut c = PrincipalCache::default();
        let load = |n: &str| {
            Ok(Principal {
                user: n.into(),
                ..Principal::default()
            })
        };
        assert_eq!(c.get((1, 1), "BOB", load).unwrap().user, "BOB");
        assert_eq!(c.get((1, 1), "BOB", load).unwrap().user, "BOB");
        assert_eq!(c.loads(), 1);
        c.get((1, 2), "BOB", load).unwrap();
        assert_eq!(c.loads(), 2, "a profile changed");
        assert_eq!(
            c.get((1, 2), "X", |_| Err(Errno::ENOTENROLL)),
            Err(Errno::ENOTENROLL)
        );
    }
}
