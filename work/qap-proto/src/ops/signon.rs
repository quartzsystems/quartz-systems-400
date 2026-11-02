// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `SignOn`: sign on, change a password, resume and sign off.
//! `SignOff` has no bodies.

use ipc::wire_struct;

use crate::secret::{Secret, Token};

/// `SignOn`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SignOn {
    /// The user profile.
    pub user: String,
    /// Its password.
    pub password: Secret,
}

wire_struct!(SignOn { user, password });

/// `ChangePassword`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangePassword {
    /// The user profile.
    pub user: String,
    /// The password now.
    pub old_password: Secret,
    /// The password wanted.
    pub new_password: Secret,
}

wire_struct!(ChangePassword {
    user,
    old_password,
    new_password
});

/// `Resume`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Resume {
    /// The session's token.
    pub token: Token,
}

wire_struct!(Resume { token });

/// The reply to `SignOn`, `ChangePassword` and `Resume`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SignOnReply {
    /// The session's token.
    pub token: Token,
    /// The session ends this long after its last connection closes.
    pub idle_timeout_secs: u32,
    /// The profile signed on.
    pub profile: String,
    /// Its description.
    pub text: String,
    /// Its CCSID.
    pub ccsid: u16,
    /// `*MDY`, `*YMD`, ...
    pub date_format: String,
    /// The date separator.
    pub date_separator: String,
    /// The time separator.
    pub time_separator: String,
    /// The home directory.
    pub home_directory: String,
    /// `*ALLOBJ`, `*SECADM`, ...
    pub special_authorities: Vec<String>,
}

wire_struct!(SignOnReply {
    token,
    idle_timeout_secs,
    profile,
    text,
    ccsid,
    date_format,
    date_separator,
    time_separator,
    home_directory,
    special_authorities
});
