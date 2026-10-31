// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The message identifiers the engine reports, settled against IBM i
//! V7R1M0 (`tools/conformance/oracle/save/primary/README.md`, rules in
//! brackets). The few not settled there are marked.

/// Objects saved from one library, all of them [5].
pub const SAVED_LIB: &str = "CPC3722";
/// Objects saved, some not included (`OBJJRN(*NO)`, `FILEMBR`) [5].
pub const SAVED_NOT_INCLUDED: &str = "CPC3723";
/// Objects saved, some not saved: the escape [5].
pub const SAVED_LIB_PARTIAL: &str = "CPF3701";
/// Several libraries, some objects not saved (not settled).
pub const SAVED_LIBS_PARTIAL: &str = "CPF3777";
/// Nothing was saved or restored for the library [1, 5].
pub const NONE_FOR_LIB: &str = "CPF3770";
/// A library to save or restore into does not exist [5].
pub const NO_LIBRARY: &str = "CPF3781";
/// A named object to save was not found [5].
pub const NOT_FOUND_SAVE: &str = "CPF3764";
/// An object in use, not saved or not restored [9].
pub const IN_USE: &str = "CPF3761";
/// Not saved: not authorized (not settled).
pub const NOT_SAVED_AUTH: &str = "CPF3741";
/// The save file being written, inside the saved library, saved
/// without its data [21].
pub const SAVF_NO_DATA: &str = "CPI374B";
/// Objects restored to a library, all of them [1].
pub const RESTORED_LIB: &str = "CPC3703";
/// Objects restored, some not, or differences accepted: the escape [1, 4].
pub const RESTORED_LIB_PARTIAL: &str = "CPF3773";
/// An object not restored, after its own reason [6-11].
pub const NOT_RESTORED: &str = "CPF3756";
/// Not selected by `OPTION(*NEW)`: it exists [2].
pub const SKIP_EXISTS: &str = "CPF3725";
/// Not selected by `OPTION(*OLD)`: it does not exist [2].
pub const SKIP_MISSING: &str = "CPF3724";
/// Not selected by `OPTION(*FREE)`: its storage was not freed [2].
pub const SKIP_NOT_FREED: &str = "CPF3726";
/// An existing journal is never replaced [6].
pub const JOURNAL_EXISTS: &str = "CPF7085";
/// An existing data queue is never replaced [7].
pub const DTAQ_EXISTS: &str = "CPD2415";
/// An attached receiver is never replaced [8].
pub const RECEIVER_ATTACHED: &str = "CPF7081";
/// An existing detached receiver is not replaced [8].
pub const RECEIVER_EXISTS: &str = "CPF3734";
/// A logical file whose physical file is missing [10].
pub const NOT_RESTORED_BASED_ON: &str = "CPF3204";
/// `MBROPT(*MATCH)` and the member lists differ [11].
pub const NOT_RESTORED_MEMBERS: &str = "CPF3284";
/// Level identifiers differ [12].
pub const NOT_RESTORED_LEVEL: &str = "CPF3283";
/// `ALWOBJDIF(*ALL)` renamed the existing file [12].
pub const FILE_RENAMED: &str = "CPI320A";
/// The owner differs and `ALWOBJDIF` does not allow it [13].
pub const NOT_RESTORED_OWNER: &str = "CPF3766";
/// Restored; the existing owner was kept [13].
pub const OWNER_KEPT: &str = "CPF370B";
/// The primary group differs and `ALWOBJDIF` does not allow it [13].
pub const NOT_RESTORED_PGP: &str = "CPD37A2";
/// Restored; the existing primary group was kept [13].
pub const PGP_KEPT: &str = "CPI3811";
/// Security or data format changes occurred [4].
pub const CHANGES: &str = "CPF3848";
/// The saved object is damaged (not settled).
pub const NOT_RESTORED_DAMAGED: &str = "CPF3782";
/// Not restored: not authorized (not settled).
pub const NOT_RESTORED_AUTH: &str = "CPF3757";
/// A named object is not in the save [24].
pub const NOT_IN_SAVE: &str = "CPF3801";
/// Nothing named was in the save: the escape [24].
pub const NONE_IN_SAVE: &str = "CPF3780";
/// An owner that does not exist: the default owner (not settled).
pub const DEFAULT_OWNER: &str = "CPI3722";
/// Journaling could not be started again [18].
pub const JOURNAL_MISSING: &str = "CPF3294";
/// `QALWOBJRST` does not allow the object (not settled).
pub const NOT_RESTORED_SYSVAL: &str = "CPF3846";
/// `QVFYOBJRST`: unsigned, or a signature that does not verify (not
/// settled).
pub const NOT_RESTORED_SIGNATURE: &str = "CPF3847";
/// Existing spooled files not restored again [27].
pub const SPOOL_EXISTS: &str = "CPI3733";

/// The old names, kept so callers read as before.
pub const NOT_SAVED_IN_USE: &str = IN_USE;
/// See [`IN_USE`].
pub const NOT_RESTORED_IN_USE: &str = IN_USE;
