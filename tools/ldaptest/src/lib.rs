// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The LDAP test client's scenarios (`docs/DIRSRV.md` section 13).
//!
//! Nothing here depends on `net/ldap` or `net/dirsrv`: the messages, BER,
//! filters and the controls and extended operations it knows are `ldap3`'s
//! (other authors than the server), and what it lacks (sort, persistent
//! search, entry change notification, tree delete, LDIF, schema
//! descriptions) is written by hand from the RFCs, so a misreading cannot
//! hide in both the server and this client.

#![deny(missing_docs)]

pub mod bench;
pub mod ber;
pub mod cli;
pub mod client;
pub mod code;
pub mod conn;
pub mod ctrl;
pub mod dse;
pub mod error;
pub mod ldif;
pub mod mask;
pub mod oracle;
pub mod report;
pub mod scenarios;
pub mod schema;
pub mod target;

pub use error::Error;
pub use report::{Outcome, Report};
pub use target::Target;
