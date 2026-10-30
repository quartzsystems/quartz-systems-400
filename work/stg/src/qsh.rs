// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QSH and the environment over the link: one request (`qsh`, tag
//! 1300) carries a [`QshOp`] and comes back as a [`QshReply`]. The
//! service keeps the system-level environment variables
//! (`ADDENVVAR LEVEL(*SYS)`) and answers the profiles' user and group
//! IDs the shell's utilities show.

use ipc::{wire_enum, wire_struct};

/// One environment variable.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnvVar {
    /// The name, case kept.
    pub name: String,
    /// The value; `None` is `*NULL`.
    pub value: Option<String>,
    /// The CCSID the value is tagged with (0 the job's).
    pub ccsid: i32,
}

wire_struct!(EnvVar { name, value, ccsid });

/// A profile's identity for `ls -l`, `id` and `chown`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProfileIds {
    /// The profile name.
    pub name: String,
    /// `UID`.
    pub uid: u32,
    /// `GID` (0 for none).
    pub gid: u32,
    /// The home directory.
    pub home: String,
}

wire_struct!(ProfileIds {
    name,
    uid,
    gid,
    home
});

/// How [`QshOp::SysEnvSet`] treats an existing variable.
pub mod set_mode {
    /// `ADDENVVAR REPLACE(*NO)`: fail when it exists.
    pub const ADD: u8 = 0;
    /// `CHGENVVAR`: fail when it does not exist.
    pub const CHANGE: u8 = 1;
    /// `ADDENVVAR REPLACE(*YES)`, `Qp0zPutSysEnv`: either.
    pub const PUT: u8 = 2;
}

/// One request.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)] // the fields are named as the commands name them
pub enum QshOp {
    /// The system-level environment variables.
    SysEnv,
    /// Add or change one system-level variable (`*ALLOBJ`); `mode` is
    /// one of [`set_mode`].
    SysEnvSet { var: EnvVar, mode: u8 },
    /// Remove one, or every one for `*ALL` (`*ALLOBJ`).
    SysEnvRemove { name: String },
    /// Every profile's name, UID, GID and home directory.
    Profiles,
}

wire_enum!(QshOp {
    0 => SysEnv,
    1 => SysEnvSet { var, mode },
    2 => SysEnvRemove { name },
    3 => Profiles,
});

/// The answer to a [`QshOp`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)] // one variant per kind of answer
pub enum QshReply {
    /// Done.
    Done,
    /// Refused: the escape message and its text.
    Failed { msgid: String, text: String },
    /// The system-level variables.
    Env { vars: Vec<EnvVar> },
    /// The profiles.
    Profiles { rows: Vec<ProfileIds> },
}

wire_enum!(QshReply {
    0 => Done,
    1 => Failed { msgid, text },
    2 => Env { vars },
    3 => Profiles { rows },
});

impl QshReply {
    /// A refusal.
    pub fn failed(msgid: &str, text: impl Into<String>) -> Self {
        Self::Failed {
            msgid: msgid.into(),
            text: text.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let var = EnvVar {
            name: "QIBM_MULTI_THREADED".into(),
            value: Some("Y".into()),
            ccsid: 37,
        };
        for op in [
            QshOp::SysEnv,
            QshOp::SysEnvSet {
                var: var.clone(),
                mode: set_mode::PUT,
            },
            QshOp::SysEnvRemove { name: "*ALL".into() },
            QshOp::Profiles,
        ] {
            assert_eq!(ipc::decode::<QshOp>(&ipc::encode(&op)).unwrap(), op);
        }
        let r = QshReply::Env {
            vars: vec![var, EnvVar::default()],
        };
        assert_eq!(ipc::decode::<QshReply>(&ipc::encode(&r)).unwrap(), r);
    }
}
