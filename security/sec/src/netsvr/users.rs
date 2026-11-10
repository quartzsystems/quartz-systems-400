// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Users disabled for `NetServer`. As on IBM i the count of passwords not
//! valid and the disabled state are `NetServer`'s own, kept in memory by
//! the storage service and not in the profile: the profile stays
//! enabled, and a restart of `NetServer` or an IPL enables everyone. The
//! limit is `QMAXSIGN` as it was when `NetServer` started.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, OnceLock, PoisonError};

/// Which `QMAXSIGN` counts.
#[derive(Debug, Default, Clone, Copy)]
enum Limit {
    /// `NetServer` has not started: the live value.
    #[default]
    Live,
    /// The value when `NetServer` started.
    AtStart(Option<u32>),
}

#[derive(Debug, Default)]
struct State {
    /// Passwords not valid since the last good sign-on.
    failures: HashMap<String, u32>,
    /// Disabled users and when (Unix seconds).
    disabled: BTreeMap<String, u64>,
    /// `QMAXSIGN` taken at start (`None` in it for `*NOMAX`).
    limit: Limit,
}

/// The table.
#[derive(Debug, Default)]
pub struct NetUsers {
    state: Mutex<State>,
}

fn key(user: &str) -> String {
    user.trim().to_ascii_uppercase()
}

impl NetUsers {
    /// An empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn with<T>(&self, f: impl FnOnce(&mut State) -> T) -> T {
        f(&mut self.state.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// `NetServer` started: every user enabled, `QMAXSIGN` taken.
    pub fn started(&self, maxsign: Option<u32>) {
        self.with(|s| {
            s.failures.clear();
            s.disabled.clear();
            s.limit = Limit::AtStart(maxsign);
        });
    }

    /// Whether `user` is disabled for `NetServer`.
    #[must_use]
    pub fn is_disabled(&self, user: &str) -> bool {
        self.with(|s| s.disabled.contains_key(&key(user)))
    }

    /// The disabled users with when, by name.
    #[must_use]
    pub fn disabled(&self) -> Vec<(String, u64)> {
        self.with(|s| s.disabled.iter().map(|(k, v)| (k.clone(), *v)).collect())
    }

    /// Passwords not valid counted for `user`.
    #[must_use]
    pub fn failures(&self, user: &str) -> u32 {
        self.with(|s| s.failures.get(&key(user)).copied().unwrap_or(0))
    }

    /// Enable `user` again; whether it was disabled.
    pub fn enable(&self, user: &str) -> bool {
        self.with(|s| {
            let k = key(user);
            s.failures.remove(&k);
            s.disabled.remove(&k).is_some()
        })
    }

    /// A good sign-on: the count starts over.
    pub fn succeeded(&self, user: &str) {
        self.with(|s| {
            s.failures.remove(&key(user));
        });
    }

    /// A password not valid; `live` is `QMAXSIGN` now, used when
    /// `NetServer` has not started. Whether this one disabled the user.
    pub fn failed(&self, user: &str, live: Option<u32>, now: u64) -> bool {
        self.with(|s| {
            let k = key(user);
            let n = s.failures.entry(k.clone()).or_insert(0);
            *n += 1;
            let max = match s.limit {
                Limit::Live => live,
                Limit::AtStart(m) => m,
            };
            if max.is_some_and(|m| *n >= m.max(1)) && !s.disabled.contains_key(&k) {
                s.disabled.insert(k, now);
                true
            } else {
                false
            }
        })
    }
}

/// The storage service's table.
pub fn users() -> &'static NetUsers {
    static T: OnceLock<NetUsers> = OnceLock::new();
    T.get_or_init(NetUsers::new)
}

/// Enable `user` for `NetServer` again (`QZLSCHSI`, Navigator); whether
/// it was disabled.
pub fn enable_netserver_user(user: &str) -> bool {
    users().enable(user)
}

/// Whether `user` is disabled for `NetServer` (`USER_INFO`).
#[must_use]
pub fn netserver_disabled(user: &str) -> bool {
    users().is_disabled(user)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_reset_and_restart() {
        let t = NetUsers::new();
        assert!(!t.failed("bob", Some(3), 1));
        assert!(!t.failed("BOB", Some(3), 2));
        t.succeeded("Bob");
        assert_eq!(t.failures("BOB"), 0);
        assert!(!t.failed("BOB", Some(3), 3));
        assert!(!t.failed("BOB", Some(3), 4));
        assert!(t.failed("BOB", Some(3), 5));
        assert!(t.is_disabled("bob"));
        assert!(!t.failed("BOB", Some(3), 6), "disabled once");
        assert_eq!(t.disabled(), vec![("BOB".to_string(), 5)]);
        assert!(t.enable("BOB"));
        assert!(!t.enable("BOB"));
        // *NOMAX never disables; the value at start wins over the live one.
        for i in 0..10 {
            assert!(!t.failed("ANN", None, i));
        }
        t.started(Some(2));
        assert!(!t.failed("ANN", None, 1));
        assert!(t.failed("ANN", None, 2));
        t.started(Some(2));
        assert!(!t.is_disabled("ANN"));
    }
}
