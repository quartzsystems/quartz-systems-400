// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*QSI` (docs/QSID.md): role `QSI` is the listener `QSIDSRV` in
//! `QSYSWRK`, which answers management consoles over QSI.

#[allow(dead_code)] // the server calls it from its console family
pub mod console;
