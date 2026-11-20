// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*QSI` (docs/QSID.md section 12): role `QSI` is the listener
//! `QSIDSRV` in `QSYSWRK`, which answers management consoles over QSI
//! (`net/qsisvr`); `console` is the system's console as its console
//! family reaches it.

pub mod console;
pub mod host;
pub mod listener;
