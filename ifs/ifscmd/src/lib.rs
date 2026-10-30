// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The IFS commands as library functions over an
//! [`ifs::client::IfsClient`]: the storage service runs them for a job
//! against its `Ifs` (`lic/slic/src/cpp/ifscmd.rs`), the tests against
//! an `Ifs` in memory. Each answers a [`msg::Reply`]: text, IBM's
//! completion and diagnostic messages, the escape. [`display`] is what
//! the displays show (links, attributes, authorities), shared by the
//! job's panels and the text forms of [`text`]; [`stmf`] copies
//! members to stream files and back.

#![deny(missing_docs)]

pub mod cmds;
pub mod display;
pub mod msg;
pub mod stmf;
pub mod text;

pub use msg::{Msg, Reply, quote};
