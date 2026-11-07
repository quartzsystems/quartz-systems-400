// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SQL call level interface: the procedures IBM i's service program
//! `QSQCLI` exports (`SQLAllocEnv`, `SQLConnect`, `SQLPrepare`,
//! `SQLFetch`, ...), bound as `*SYSTEM` imports like `QLEAWI`'s. A
//! program reaches them as IBM i's do, through `QUSAPIBD` (there is no
//! `QSQCLI` binding directory on IBM i either).
//!
//! [`exec::Cli`] holds a job's handles and does the work over a
//! [`exec::Host`]; [`abi`] is the C-linkage face over the job's storage;
//! [`server`] runs a connection's statements in a `QSQSRVR` job in
//! server mode. `docs/SYSAPI.md` §13 has what is supported.

// The functions take IBM's arguments, one by one.
#![allow(
    clippy::too_many_arguments,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::match_same_arms,
    clippy::struct_excessive_bools
)]

pub mod abi;
pub mod consts;
pub mod data;
pub mod exec;
pub mod model;
pub mod server;

pub use exec::{Cli, Host};

/// The service program the procedures are shown under.
pub const SRVPGM: &str = "QSQCLI";

/// Every procedure by export name, with its code address.
#[must_use]
pub fn procedures() -> Vec<(&'static str, usize)> {
    abi::calls::procedures()
}

/// Install the procedures into a job.
pub fn install(job: &mut mi::Job) {
    for (name, addr) in procedures() {
        job.act.system.insert(name.to_string(), addr);
        // RPG upper-cases an EXTPROC name it was not given quoted.
        job.act.system.insert(name.to_ascii_uppercase(), addr);
    }
}

/// Forget a job's handles when the job goes away; its server-mode
/// connections end.
pub fn uninstall(job: &mi::Job) {
    abi::drop_state(job);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_procedure_is_qsqcli_s_and_back() {
        let ours: Vec<&str> = procedures().iter().map(|(n, _)| *n).collect();
        let mut seen = std::collections::HashSet::new();
        for n in &ours {
            assert!(seen.insert(*n), "{n} registered twice");
            assert_eq!(
                ile::system_srvpgm(n),
                Some(SRVPGM),
                "{n} is not bound to {SRVPGM}"
            );
        }
        for (n, _) in ile::SYSTEM_SRVPGM_PROCS
            .iter()
            .filter(|(_, s)| *s == SRVPGM)
        {
            assert!(ours.contains(n), "{n} has no procedure");
        }
    }
}
