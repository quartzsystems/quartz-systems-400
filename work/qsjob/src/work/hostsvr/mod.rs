// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The host server jobs: daemons that listen on the servers' ports and
//! hand each connection, through the storage service's descriptor
//! relay, to a prestart job of the server, which serves it as the user
//! who signed on (`net/hostserver` has the request loops).

pub mod auth;
pub mod daemon;
pub mod job;
pub mod prestart;
