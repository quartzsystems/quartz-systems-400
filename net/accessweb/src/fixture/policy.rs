// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The prototype's policies and preferences: group `WEBUSERS` denied
//! what `web/policies.html` shows as "Group WEBUSERS"; `TESTUSER`
//! prefers 27 x 132 screens and last signed on from 10.0.10.55.

use std::collections::BTreeMap;
use std::sync::Mutex;

use qsweb::msg::Message;

use crate::env::policy::{PolicyStore, Setting};
use crate::env::prefs::PrefStore;
use crate::policy as p;

/// What group `WEBUSERS` is denied.
pub const WEBUSERS_DENIED: &[&str] = &[
    p::DB_WRITE,
    p::DB_SQL,
    p::DB_COPY_TO,
    p::DB_SHARE,
    p::FILES_EDIT,
    p::FILES_DELETE,
    p::FILES_OUTSIDE,
    p::PRINT_MOVE,
    p::PRINT_OTHERS,
    p::MSGS_INQUIRY,
    p::JOBS_END_SERVER,
    p::CMD_RUN,
    p::CMD_PROMPT,
    p::CMD_SAVE,
];

/// The sample policy store.
pub struct FixturePolicy {
    /// Subject to settings.
    pub map: Mutex<BTreeMap<String, Vec<(String, Setting)>>>,
}

impl Default for FixturePolicy {
    fn default() -> Self {
        let mut m = BTreeMap::new();
        m.insert(
            "WEBUSERS".to_string(),
            WEBUSERS_DENIED
                .iter()
                .map(|f| ((*f).to_string(), Setting::Deny))
                .collect(),
        );
        Self { map: Mutex::new(m) }
    }
}

impl PolicyStore for FixturePolicy {
    fn settings(&self, subject: &str) -> Vec<(String, Setting)> {
        self.map
            .lock()
            .expect("policies")
            .get(subject)
            .cloned()
            .unwrap_or_default()
    }

    fn put(&self, subject: &str, entries: &[(String, Setting)]) -> Result<(), Message> {
        self.map
            .lock()
            .expect("policies")
            .insert(subject.to_string(), entries.to_vec());
        Ok(())
    }
}

/// The sample preference store.
pub struct FixturePrefs {
    /// User to values.
    pub map: Mutex<BTreeMap<String, Vec<(String, String)>>>,
}

impl Default for FixturePrefs {
    fn default() -> Self {
        let mut m = BTreeMap::new();
        m.insert(
            "TESTUSER".to_string(),
            vec![
                (crate::prefs::SCREEN.to_string(), "27".to_string()),
                (
                    crate::prefs::SIGNON_FROM.to_string(),
                    "10.0.10.55".to_string(),
                ),
            ],
        );
        Self { map: Mutex::new(m) }
    }
}

impl PrefStore for FixturePrefs {
    fn get(&self, user: &str) -> Vec<(String, String)> {
        self.map
            .lock()
            .expect("prefs")
            .get(user)
            .cloned()
            .unwrap_or_default()
    }

    fn put(&self, user: &str, values: &[(String, String)]) -> Result<(), Message> {
        self.map
            .lock()
            .expect("prefs")
            .insert(user.to_string(), values.to_vec());
        Ok(())
    }

    fn schemas(&self, _user: &str) -> Vec<String> {
        super::db::SCHEMAS.iter().map(ToString::to_string).collect()
    }
}
