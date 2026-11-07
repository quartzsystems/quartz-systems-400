// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the product needs from the system, as traits by area: the
//! running system answers them in the `ACCESSWEB` job (`work/qsjob`)
//! as the signed-on user, the fixture ([`crate::fixture`]) in tests and
//! the demo server. Each area's trait is its own agent's to grow.

pub mod cmd;
pub mod db;
pub mod download;
pub mod files;
pub mod jobs;
pub mod msgs;
pub mod policy;
pub mod prefs;
pub mod print;
pub mod session5250;

use std::sync::Arc;

use qsweb::msg::{Message, SecondLevel};

/// A special authority an action needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    /// `*ALLOBJ`.
    AllObj,
    /// `*IOSYSCFG`.
    IoSysCfg,
    /// `*JOBCTL`.
    JobCtl,
    /// `*SECADM`.
    SecAdm,
    /// `*SPLCTL`.
    SplCtl,
}

impl Need {
    /// The special authority's name.
    pub fn name(self) -> &'static str {
        match self {
            Need::AllObj => "*ALLOBJ",
            Need::IoSysCfg => "*IOSYSCFG",
            Need::JobCtl => "*JOBCTL",
            Need::SecAdm => "*SECADM",
            Need::SplCtl => "*SPLCTL",
        }
    }
}

/// An audit journal entry the product writes itself. The commands it
/// runs write their own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Audit {
    /// `PW`: a refused sign-on (user, the reason code).
    Password {
        /// The user named.
        user: String,
        /// `P` wrong password, `U` no such user, `D` disabled, ...
        code: char,
    },
    /// `AF`: a refused action.
    Refused {
        /// The user.
        user: String,
        /// The object or function.
        object: String,
        /// What was missing (`*SECADM`, `policy db.write`).
        need: String,
    },
}

/// An accepted sign-on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SignOn {
    /// The profile name.
    pub user: String,
    /// The sign-on before this one, seconds since 1970, 0 for none.
    pub previous: u64,
    /// Days until the password expires, when the system warns
    /// (`QPWDEXPWRN`).
    pub expires_in: Option<i64>,
    /// The password has expired: the user may only change it.
    pub expired: bool,
}

/// A profile, as the Policies page lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Profile {
    /// `WEBUSER`.
    pub name: String,
    /// `*USER`, `*SECOFR`, ...
    pub class: String,
    /// Whether it is a group profile.
    pub group: bool,
}

/// What the home page counts for a user.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    /// Messages on the user's message queue.
    pub messages: u32,
    /// The user's spooled files.
    pub spooled: u32,
    /// The user's active jobs.
    pub active_jobs: u32,
}

/// The system: its name and clock, users, authority, audit, messages.
pub trait System: Send + Sync {
    /// The system name (`QSHQ400`).
    fn name(&self) -> String;
    /// Seconds since 1970.
    fn now(&self) -> u64;
    /// The system time zone's offset from UTC at `secs`, in seconds.
    fn offset(&self, secs: u64) -> i64;
    /// The system time zone as Preferences names it (`Eastern, US DST`).
    fn zone_label(&self) -> String;
    /// Check a user and password with the 5250 sign-on rules
    /// (`QMAXSIGN`, disabled profiles, expired passwords; `PW` entries).
    /// `from` is the client's address.
    fn sign_on(&self, user: &str, password: &str, from: &str) -> Result<SignOn, Message>;
    /// The user signed off (or the session ended): release what sign-on
    /// took.
    fn sign_off(&self, user: &str);
    /// Change `user`'s password (`QSYCHGPW`): the system's message on
    /// refusal (`CPF22E2`, `CPF2234`...).
    fn change_password(&self, user: &str, current: &str, new: &str) -> Result<(), Message>;
    /// The password rules in force, as one line of system values
    /// (`QPWDMINLEN 8, QPWDMAXLEN 128, ...`).
    fn password_rules(&self) -> String;
    /// The user's special authorities.
    fn special(&self, user: &str) -> Vec<String>;
    /// The user's class (`*SECOFR`, `*USER`, ...).
    fn user_class(&self, user: &str) -> String;
    /// The user's group profiles, primary first.
    fn groups(&self, user: &str) -> Vec<String>;
    /// The profile's text (`Cody Wellman`).
    fn text(&self, user: &str) -> String;
    /// The profiles policies are kept for, users and groups, as `user`
    /// may list them.
    fn profiles(&self, user: &str) -> Vec<Profile>;
    /// What the home page counts for `user`, read as the user.
    fn counts(&self, user: &str) -> Counts;
    /// Write an audit journal entry.
    fn audit(&self, entry: Audit);
    /// A message's text from its message file, `data` substituted.
    fn message(&self, id: &str, data: &[&str]) -> Option<SecondLevel>;
}

/// The system Access for Web runs over.
#[derive(Clone)]
pub struct Env {
    /// Name, clock, sign-on, authority, audit, messages.
    pub system: Arc<dyn System>,
    /// The policy store.
    pub policy: Arc<dyn policy::PolicyStore>,
    /// Per-user preferences.
    pub prefs: Arc<dyn prefs::PrefStore>,
    /// 5250 sessions.
    pub session5250: Arc<dyn session5250::Session5250>,
    /// The database.
    pub db: Arc<dyn db::Db>,
    /// The integrated file system.
    pub files: Arc<dyn files::Files>,
    /// Printer output and printers.
    pub print: Arc<dyn print::Print>,
    /// Messages.
    pub msgs: Arc<dyn msgs::Msgs>,
    /// Jobs.
    pub jobs: Arc<dyn jobs::Jobs>,
    /// Commands.
    pub cmd: Arc<dyn cmd::Cmd>,
    /// The download page's files.
    pub download: Arc<dyn download::Download>,
}
