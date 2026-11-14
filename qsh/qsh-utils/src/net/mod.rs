// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Network utilities: `hostname`, `nslookup`, `host`, `dig`, `rexec` and the
//! LDAP tools ([`ldap`]).

pub mod dig;
pub mod dns;
pub mod host;
pub mod hostname;
pub mod ldap;
pub mod nslookup;
pub mod rexec;
