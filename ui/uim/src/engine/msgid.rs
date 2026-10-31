// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The message IDs the engine puts on the message line. Their texts
//! live in `QCPFMSG` (`runtime/msgf/msgs/qcpfmsg.toml`); the `CPD6Axx`
//! ones are IBM's, read on pub400 (`tools/conformance/golden/audit/msgids`).

/// A function key the panel does not define.
pub const FKEY_NOT_ALLOWED: &str = "CPD6A60";
/// An option no `:LISTACT.` of the list has.
pub const OPTION_NOT_VALID: &str = "CPD6A7C";
/// A menu selection the menu does not have.
pub const SELECTION_NOT_VALID: &str = "CPD6A64";
/// Page down at the bottom of an area.
pub const AT_BOTTOM: &str = "CPD6A69";
/// Page up at the top of an area.
pub const AT_TOP: &str = "CPD6A66";
/// Position to: no entry at or after `&1`.
pub const NO_ENTRY_AT: &str = "CPD9B80";
/// A dialog action this release does not have.
pub const NOT_AVAILABLE: &str = "CPD9B81";
/// Help with no module behind it.
pub const NO_HELP: &str = "CPD9B82";
/// The Information Assistant cannot be opened.
pub const NO_ASSISTANT: &str = "CPD9B83";
/// The display cannot be printed here.
pub const NO_PRINT: &str = "CPD9B84";
/// The display was printed to `QSYSPRT`.
pub const PRINTED: &str = "CPI9B80";
/// Retrieve with no earlier command.
pub const NO_RETRIEVE: &str = "CPD9B85";
