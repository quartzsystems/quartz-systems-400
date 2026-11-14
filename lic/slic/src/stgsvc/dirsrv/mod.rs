// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The directory server's side in the service (docs/DIRSRV.md §4):
//! `serve` the requests 1560-1567 and the sessions, `instance` one
//! instance's store, schema, configuration and directory, `profiles` the
//! projected backend's profiles, `hooks` profile binds and `DI` auditing,
//! `publish` the publishing agents' requests 1570-1572.

pub(crate) mod hooks;
pub(crate) mod instance;
pub(crate) mod profiles;
pub(crate) mod publish;
pub(crate) mod serve;

#[cfg(test)]
mod tests;
