// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The prototype's sample system, so served pages can be compared with
//! the golden pages and the demo server has something to show: system
//! `QSHQ400` in US Eastern time, a clock stopped at 10/29/26 14:03:40,
//! user `CWELLMAN` (Cody Wellman, `*SECOFR`, group `ADMINS`), and each
//! area's sample data in its own module.

pub mod cmd;
pub mod db;
pub mod download;
pub mod files;
pub mod jobs;
pub mod msgs;
pub mod policy;
pub mod print;
pub mod session5250;
pub mod text;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use qsweb::msg::{Kind, Message, SecondLevel};

use crate::env::{Audit, Counts, Env, Profile, SignOn, System};
use crate::util::secs;

/// The fixture's password for every user.
pub const PASSWORD: &str = "QS400";

/// The user the golden pages show.
pub const USER: &str = "CWELLMAN";

/// The fixture clock (14:03:40 EDT).
pub fn now() -> u64 {
    secs(2026, 10, 29, 18, 3, 40)
}

/// US Eastern daylight time.
pub const OFFSET: i64 = -4 * 3600;

/// A profile the sample system has.
pub struct Sample {
    /// The name.
    pub name: &'static str,
    /// Its text.
    pub text: &'static str,
    /// The user class.
    pub class: &'static str,
    /// Special authorities.
    pub special: &'static [&'static str],
    /// Group profiles, primary first.
    pub groups: &'static [&'static str],
    /// Whether it is a group.
    pub group: bool,
}

const ALL: &[&str] = &["*ALLOBJ", "*AUDIT", "*IOSYSCFG", "*JOBCTL", "*SAVSYS", "*SECADM", "*SERVICE", "*SPLCTL"];

/// The profiles, in the order the system lists them.
pub const PROFILES: &[Sample] = &[
    Sample { name: "CWELLMAN", text: "Cody Wellman", class: "*SECOFR", special: ALL, groups: &["ADMINS"], group: false },
    Sample { name: "QSECOFR", text: "Security officer", class: "*SECOFR", special: ALL, groups: &[], group: false },
    Sample { name: "WEBUSER", text: "Web user", class: "*USER", special: &[], groups: &["WEBUSERS"], group: false },
    Sample { name: "WEBUSERS", text: "Access for Web users", class: "*USER", special: &[], groups: &[], group: true },
    Sample { name: "WAREHOUSE", text: "Warehouse staff", class: "*USER", special: &[], groups: &[], group: true },
    Sample { name: "ADMINS", text: "Administrators", class: "*SECOFR", special: ALL, groups: &[], group: true },
];

fn sample(user: &str) -> Option<&'static Sample> {
    PROFILES.iter().find(|p| p.name == user && !p.group)
}

/// The sample system.
pub struct FixtureSystem {
    /// Audit entries written.
    pub audit: Mutex<Vec<Audit>>,
    /// Passwords changed from [`PASSWORD`].
    pub passwords: Mutex<BTreeMap<String, String>>,
    /// Profiles whose password has expired.
    pub expired: Mutex<Vec<String>>,
}

impl Default for FixtureSystem {
    fn default() -> Self {
        Self {
            audit: Mutex::new(Vec::new()),
            passwords: Mutex::new(BTreeMap::new()),
            expired: Mutex::new(Vec::new()),
        }
    }
}

impl FixtureSystem {
    fn password(&self, user: &str) -> String {
        self.passwords.lock().expect("passwords").get(user).cloned().unwrap_or_else(|| PASSWORD.into())
    }

    fn refuse(id: &str, user: &str) -> Message {
        let text = text::second_level(id, &[user]).map(|s| s.text).unwrap_or_default();
        Message::new(Kind::Error, id, &text)
    }
}

impl System for FixtureSystem {
    fn name(&self) -> String {
        "QSHQ400".into()
    }

    fn now(&self) -> u64 {
        now()
    }

    fn offset(&self, _secs: u64) -> i64 {
        OFFSET
    }

    fn zone_label(&self) -> String {
        "Eastern, US DST".into()
    }

    fn sign_on(&self, user: &str, password: &str, _from: &str) -> Result<SignOn, Message> {
        let u = user.trim().to_ascii_uppercase();
        let Some(p) = sample(&u) else {
            self.audit(Audit::Password { user: u.clone(), code: 'U' });
            return Err(Self::refuse("CPF2204", &u));
        };
        if password != self.password(p.name) {
            self.audit(Audit::Password { user: u.clone(), code: 'P' });
            return Err(Self::refuse("CPF22E2", &u));
        }
        let expired = self.expired.lock().expect("expired").contains(&u);
        Ok(SignOn {
            user: u,
            previous: secs(2026, 10, 29, 17, 5, 0),
            expires_in: if expired { None } else { Some(5) },
            expired,
        })
    }

    fn sign_off(&self, _user: &str) {}

    fn change_password(&self, user: &str, current: &str, new: &str) -> Result<(), Message> {
        if current != self.password(user) {
            return Err(Self::refuse("CPF22E2", user));
        }
        if new.len() < 8 || new == current {
            return Err(Self::refuse("CPF2234", user));
        }
        self.passwords.lock().expect("passwords").insert(user.to_string(), new.to_string());
        self.expired.lock().expect("expired").retain(|u| u != user);
        Ok(())
    }

    fn password_rules(&self) -> String {
        "QPWDMINLEN 8, QPWDMAXLEN 128, QPWDRQDDIF 1, QPWDLVL 3".into()
    }

    fn special(&self, user: &str) -> Vec<String> {
        PROFILES
            .iter()
            .find(|p| p.name == user)
            .map(|p| p.special.iter().map(ToString::to_string).collect())
            .unwrap_or_default()
    }

    fn user_class(&self, user: &str) -> String {
        PROFILES.iter().find(|p| p.name == user).map_or("*USER", |p| p.class).to_string()
    }

    fn groups(&self, user: &str) -> Vec<String> {
        PROFILES
            .iter()
            .find(|p| p.name == user)
            .map(|p| p.groups.iter().map(ToString::to_string).collect())
            .unwrap_or_default()
    }

    fn text(&self, user: &str) -> String {
        PROFILES.iter().find(|p| p.name == user).map(|p| p.text.to_string()).unwrap_or_default()
    }

    fn profiles(&self) -> Vec<Profile> {
        // The order the prototype's select shows.
        let order = ["CWELLMAN", "QSECOFR", "WEBUSER", "WEBUSERS", "WAREHOUSE", "ADMINS"];
        order
            .iter()
            .filter_map(|n| PROFILES.iter().find(|p| p.name == *n))
            .map(|p| Profile {
                name: p.name.into(),
                class: p.class.into(),
                group: p.group,
            })
            .collect()
    }

    fn counts(&self, _user: &str) -> Counts {
        Counts {
            messages: u32::try_from(msgs::MESSAGES.len()).unwrap_or(0),
            spooled: u32::try_from(print::SPOOLED.len()).unwrap_or(0),
            active_jobs: 2,
        }
    }

    fn audit(&self, entry: Audit) {
        self.audit.lock().expect("audit").push(entry);
    }

    fn message(&self, id: &str, data: &[&str]) -> Option<SecondLevel> {
        text::second_level(id, data)
    }
}

/// The fixture's parts, for tests to look into.
pub struct Fixture {
    /// The environment.
    pub env: Env,
    /// The system.
    pub system: Arc<FixtureSystem>,
    /// Policies.
    pub policy: Arc<policy::FixturePolicy>,
    /// Preferences.
    pub prefs: Arc<policy::FixturePrefs>,
}

/// The sample system.
pub fn fixture() -> Fixture {
    let system = Arc::new(FixtureSystem::default());
    let pol = Arc::new(policy::FixturePolicy::default());
    let prefs = Arc::new(policy::FixturePrefs::default());
    let env = Env {
        system: system.clone(),
        policy: pol.clone(),
        prefs: prefs.clone(),
        session5250: Arc::new(session5250::Fixture5250),
        db: Arc::new(db::FixtureDb),
        files: Arc::new(files::FixtureFiles),
        print: Arc::new(print::FixturePrint),
        msgs: Arc::new(msgs::FixtureMsgs),
        jobs: Arc::new(jobs::FixtureJobs),
        cmd: Arc::new(cmd::FixtureCmd),
        download: Arc::new(download::FixtureDownload),
    };
    Fixture {
        env,
        system,
        policy: pol,
        prefs,
    }
}
