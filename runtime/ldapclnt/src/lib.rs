// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QGLDCLNT`: the LDAP client API for ILE programs (docs/DIRSRV.md
//! §13), bound as `*SYSTEM` imports like `QSQCLI`'s, over the `ldap`
//! crate's client.
//!
//! [`api`] holds the functions as Rust over the job ([`state`] keeps a
//! job's handles, message chains, `BerElement`s and the storage it hands
//! out; [`mem`] reads and lays out the program's structures); [`abi`]
//! gives each its C-linkage entry. [`header`] generates QS/400's own
//! `QSYSINC` members from the same table. Secure connections get their
//! configuration from the job ([`set_tls_source`]): the `*SYSTEM`
//! certificate store, a key ring, or a DCM application ID.

#![deny(missing_docs)]
// The functions take IBM's arguments, one by one; the interface's
// numbers are used by name throughout (`consts::*`).
#![allow(
    clippy::too_many_arguments,
    clippy::similar_names,
    clippy::many_single_char_names,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::wildcard_imports,
    clippy::single_match_else
)]

pub mod abi;
pub mod api;
pub mod consts;
pub mod header;
pub mod mem;
pub mod state;

pub use state::{TlsSource, TlsWant, set_tls_source};

/// The service program the procedures are shown under.
pub const SRVPGM: &str = "QGLDCLNT";

/// Every procedure by export name, with its code address.
#[must_use]
pub fn procedures() -> Vec<(&'static str, usize)> {
    abi::procedures()
}

/// Installs the procedures into a job.
pub fn install(job: &mut mi::Job) {
    for (name, addr) in procedures() {
        job.act.system.insert(name.to_string(), addr);
        // RPG upper-cases an EXTPROC name it was not given quoted.
        job.act.system.insert(name.to_ascii_uppercase(), addr);
    }
}

/// Sets the job's CCSID, the local code page strings cross in.
pub fn configure(job: &mi::Job, ccsid: u16) {
    state::set_ccsid(job, ccsid);
}

/// Forgets a job's handles as the job goes away; its connections close.
pub fn uninstall(job: &mut mi::Job) {
    state::forget(job);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// The oracle's names: the three headers' and `QGLDCLNT`'s exports.
    fn oracle() -> (HashSet<String>, HashSet<String>) {
        let text = include_str!("../../../tools/conformance/oracle/dirsrv/api-names.txt");
        let (mut hdr, mut exp) = (HashSet::new(), HashSet::new());
        let mut in_exports = false;
        for l in text.lines() {
            if l.starts_with("[QSYS/QGLDCLNT") {
                in_exports = true;
                continue;
            }
            if l.starts_with('[') || l.starts_with('#') || l.trim().is_empty() {
                continue;
            }
            let n = l.split_whitespace().next().unwrap_or_default().to_string();
            if in_exports {
                exp.insert(n);
            } else {
                hdr.insert(n);
            }
        }
        (hdr, exp)
    }

    /// Every documented name is a procedure; the exports QS/400 leaves
    /// out are IBM's internal ones, which no header declares.
    #[test]
    fn every_name_of_the_oracle() {
        let (hdr, exp) = oracle();
        let ours: HashSet<String> = procedures().iter().map(|(n, _)| (*n).to_string()).collect();
        assert_eq!(ours.len(), procedures().len(), "a name twice");
        for n in &hdr {
            assert!(ours.contains(n), "{n} (header) has no procedure");
        }
        for n in &exp {
            let internal =
                (!n.starts_with("ldap_") && !n.starts_with("ber_")) || n == "ldap_client_table";
            assert_eq!(ours.contains(n), !internal, "{n}");
        }
        for n in &ours {
            assert!(hdr.contains(n) || exp.contains(n), "{n} is not IBM's");
        }
    }

    #[test]
    fn every_procedure_is_qgldclnt_s() {
        for (n, _) in procedures() {
            assert_eq!(ile::system_srvpgm(n), Some(SRVPGM), "{n}");
        }
        let ours: HashSet<&str> = procedures().iter().map(|(n, _)| *n).collect();
        let listed: HashSet<&str> = ile::LDAP_CLIENT_PROCS.iter().copied().collect();
        assert_eq!(ours, listed);
    }
}
