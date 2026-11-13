// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The NFS test client's scenarios (`docs/NFS.md` section 11).
//!
//! Nothing here depends on `net/nfs`: the portmap, MOUNT and NFS version 3
//! wire types are the `NFSWolf` crates' (other authors than the server), and
//! what they do not cover (NLM, NSM, NFS version 4) is written by hand from
//! the RFCs in [`wire`], so a misreading cannot hide in both the server and
//! this client.

#![deny(missing_docs)]

pub mod bench;
pub mod callback;
pub mod cli;
pub mod ctx;
pub mod error;
pub mod handle;
pub mod report;
pub mod rpc;
pub mod scenarios;
pub mod target;
pub mod v3;
pub mod wire;

pub use error::Error;
pub use report::{Outcome, Report};
pub use target::Target;
