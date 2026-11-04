// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*QAP` (docs/QAP.md): role `QAP` is the listener `QAPD` in
//! `QSYSWRK`, which accepts QS/400 Access connections on `CHGQAPA`'s
//! port; role `QAPCNN` is one connection's job in `QUSRWRK`.

pub mod commands;
pub mod conn;
pub mod host;
pub mod listener;
pub mod manage;
pub mod messages;
pub mod sql;
