// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The directory server's logic, without the job or the service
//! (docs/DIRSRV.md): the pieces the service and the instance job run.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod acl;
pub mod backend;
pub mod config;
pub mod ops;
pub mod publish;
pub mod pwd;
pub mod schema;
pub mod server;
pub mod store;
