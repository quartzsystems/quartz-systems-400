// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! TCP/IP configuration (interfaces, routes, domain) and the TCP servers.

/// What the network pages need.
pub trait Net: Send + Sync {}
