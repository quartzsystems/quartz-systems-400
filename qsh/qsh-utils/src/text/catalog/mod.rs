// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Message catalogs: `gencat` builds them from X/Open source
//! ([`gencat`]), `dspmsg` shows a message ([`dspmsg`]); [`format`] is
//! the catalog file.

pub mod dspmsg;
pub mod format;
pub mod gencat;

pub use dspmsg::run as run_dspmsg;
pub use gencat::run as run_gencat;
