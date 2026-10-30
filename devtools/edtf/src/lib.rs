// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The stream file editor (`EDTF`) and browser (`DSPF`): a file's text
//! decoded from its CCSID with its line ends noted ([`text`]), the
//! lines being edited ([`buffer`]), the line commands typed in the
//! `CMD` area ([`line`]), the `Control` field's commands and find and
//! change ([`control`]), the session that ties them to the display's
//! rows ([`session`]), the screens on a 5250 screen model ([`screen`])
//! and the F15 services display ([`services`]). The messages are IBM's
//! identifiers with Quartz's texts ([`msg`]). Nothing here touches the
//! IFS: the job's driver reads and writes the file.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod buffer;
pub mod control;
pub mod line;
pub mod msg;
pub mod screen;
pub mod services;
pub mod session;
pub mod text;

pub use buffer::Buffer;
pub use msg::Msg;
pub use session::{Action, Input, Key, Mode, RowInput, Session};
pub use text::{Eol, Text};
