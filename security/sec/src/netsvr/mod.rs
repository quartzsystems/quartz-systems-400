// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `NetServer`'s side of security (`docs/NETSERVER.md` section 5): the
//! `NTLMv2` sign-on against the sealed NT hash, the guest profile, and
//! the users disabled for `NetServer`.

pub mod logon;
pub mod users;

pub use logon::{DISABLED_MSGID, LoggedOn, Logon, Refusal, sign_on};
pub use users::{NetUsers, enable_netserver_user, netserver_disabled, users};
