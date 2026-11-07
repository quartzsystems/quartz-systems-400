// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! User and group profiles, authorization lists. Reads are what
//! `WRKUSRPRF`, `DSPUSRPRF`, `WRKAUTL`, `DSPAUTL` and `DSPAUTLOBJ` show
//! the user; every change (`CRTUSRPRF`, `CHGUSRPRF`, `DLTUSRPRF`,
//! `CRTAUTL`, `DLTAUTL`, `ADDAUTLE`, `CHGAUTLE`, `RMVAUTLE`, `CRTDIR`,
//! `CHGOWN`, `CRTMSGQ`) is a CL command run as the user through
//! [`Users::run`].

use qsweb::msg::Message;

/// Every special authority, in `SPCAUT`'s order.
pub const SPECIAL: [&str; 8] = [
    "*ALLOBJ",
    "*SECADM",
    "*JOBCTL",
    "*SPLCTL",
    "*SAVSYS",
    "*SERVICE",
    "*AUDIT",
    "*IOSYSCFG",
];

/// A user or group profile, as `DSPUSRPRF` shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Profile {
    /// `WEBUSER`.
    pub name: String,
    /// `TEXT`.
    pub text: String,
    /// `STATUS(*ENABLED)`.
    pub enabled: bool,
    /// `USRCLS`: `*USER`, `*SYSOPR`, `*PGMR`, `*SECADM`, `*SECOFR`.
    pub class: String,
    /// The last sign-on, seconds since 1970; `None` for never.
    pub last_signon: Option<u64>,
    /// `GRPPRF`, empty for `*NONE`.
    pub group: String,
    /// `SUPGRPPRF`.
    pub supplemental: Vec<String>,
    /// `GID`, `None` for `*NONE`.
    pub gid: Option<u32>,
    /// A group profile: other profiles may name it as their group.
    pub is_group: bool,
    /// When the password was last changed, seconds since 1970.
    pub pwd_changed: u64,
    /// Sign-on attempts not valid since the last good one.
    pub invalid_signons: u32,
    /// `PWDEXPITV`: `*SYSVAL`, `*NOMAX` or days.
    pub pwd_interval: String,
    /// `INLPGM`, qualified, or `*NONE`.
    pub initial_program: String,
    /// `INLMNU`.
    pub initial_menu: String,
    /// `CURLIB`.
    pub current_library: String,
    /// `LMTCPB`: `*YES`, `*PARTIAL`, `*NO`.
    pub limit_capabilities: String,
    /// `JOBD`, qualified.
    pub jobd: String,
    /// `OUTQ`.
    pub outq: String,
    /// `MSGQ`, qualified.
    pub msgq: String,
    /// `HOMEDIR`.
    pub home: String,
    /// `SPCAUT`: `*ALLOBJ`, ...
    pub special: Vec<String>,
    /// `LANGID`.
    pub language: String,
    /// `CNTRYID`.
    pub country: String,
    /// `CCSID`.
    pub ccsid: String,
    /// Objects the profile owns.
    pub owned: u32,
}

/// A private authority or ownership (`DSPUSRPRF TYPE(*OBJAUT)` and
/// `TYPE(*OBJOWN)`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObjectAuthority {
    /// The object.
    pub object: String,
    /// Its library, or the path's directory.
    pub lib: String,
    /// `*FILE`, ...
    pub typ: String,
    /// `*ALL`, `*CHANGE`, `*USE`, `*EXCLUDE`, `USER DEF`.
    pub authority: String,
    /// The profile owns it.
    pub owner: bool,
}

/// An authorization list (`WRKAUTL`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuthList {
    /// `INVDATA`.
    pub name: String,
    /// `TEXT`.
    pub text: String,
    /// The owner.
    pub owner: String,
    /// Objects the list secures.
    pub objects: u32,
    /// Its entries (`DSPAUTL`), the owner's first.
    pub entries: Vec<AuthEntry>,
}

/// One user's entry on an authorization list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuthEntry {
    /// A profile or `*PUBLIC`.
    pub user: String,
    /// `*ALL`, `*CHANGE`, `*USE`, `*EXCLUDE`, `USER DEF`.
    pub authority: String,
    /// `*AUTLMGT`: may manage the list.
    pub list_mgt: bool,
}

/// An object an authorization list secures (`DSPAUTLOBJ`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SecuredObject {
    /// The object.
    pub object: String,
    /// Its library.
    pub lib: String,
    /// Its type.
    pub typ: String,
    /// Its owner.
    pub owner: String,
}

/// What the user and group pages need.
pub trait Users: Send + Sync {
    /// Every user and group profile the user may see, in the system's
    /// order.
    fn profiles(&self, user: &str) -> Vec<Profile>;
    /// The objects profile `name` owns or is authorized to.
    fn authorities(&self, user: &str, name: &str) -> Vec<ObjectAuthority>;
    /// The authorization lists.
    fn lists(&self, user: &str) -> Vec<AuthList>;
    /// The objects list `name` secures.
    fn secured(&self, user: &str, name: &str) -> Vec<SecuredObject>;
    /// Run CL `command` as `user`: its completion or escape message.
    fn run(&self, user: &str, command: &str) -> Result<Message, Message>;
}
