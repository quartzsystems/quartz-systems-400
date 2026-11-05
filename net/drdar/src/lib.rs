// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The DRDA application requester's SQL connections: `CONNECT`, `SET
//! CONNECTION`, `RELEASE`, `DISCONNECT` and `COMMIT` across them, and a
//! job's statements sent to the current server over `net/ddm`'s client
//! when that is not the local one.

// A failure is the SQLCA a program sees, whole.
#![allow(clippy::result_large_err)]

pub mod remote;
pub mod session;
pub mod values;
pub mod verbs;

pub use remote::Remote;
pub use session::{Connector, Current, Method, Routed, Server, Sessions};
