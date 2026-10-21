// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Spooling for QS/400: the SCS data stream spooled files hold, the
//! line printer a printer file drives, and the views a spooled file is
//! read through. The objects themselves (`*OUTQ`, `*SPLF`, `*PRTF`) are
//! in `wm`; this crate knows nothing of the store.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod ddsprt;
pub mod printer;
pub mod scs;
pub mod view;

pub use printer::{Control, PageFormat, Printer};
