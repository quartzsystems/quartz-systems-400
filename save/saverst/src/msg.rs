// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The message identifiers the engine reports, in one place so the
//! oracle captures (`tools/conformance/oracle/save/`) settle them once.

/// Objects saved from one library, all of them.
pub const SAVED_LIB: &str = "CPC3722";
/// Objects saved from one library, some not.
pub const SAVED_LIB_PARTIAL: &str = "CPF3771";
/// Objects saved from several libraries, some not.
pub const SAVED_LIBS_PARTIAL: &str = "CPF3777";
/// Nothing was saved or restored for the library.
pub const NONE_FOR_LIB: &str = "CPF3770";
/// An object is in use and was not saved.
pub const NOT_SAVED_IN_USE: &str = "CPF3761";
/// The requester may not save the object.
pub const NOT_SAVED_AUTH: &str = "CPF3741";
/// Objects restored to a library, all of them.
pub const RESTORED_LIB: &str = "CPC3703";
/// Objects restored to a library, some not.
pub const RESTORED_LIB_PARTIAL: &str = "CPF3773";
/// An object is in use and was not restored.
pub const NOT_RESTORED_IN_USE: &str = "CPF3756";
/// The saved object is damaged.
pub const NOT_RESTORED_DAMAGED: &str = "CPF3782";
/// The object differs from the one on the system in a way `ALWOBJDIF`
/// does not allow.
pub const NOT_RESTORED_DIFFERS: &str = "CPF3748";
/// The members of a file do not match (`MBROPT(*MATCH)`).
pub const NOT_RESTORED_MEMBERS: &str = "CPF3812";
/// A logical file's based-on file is missing.
pub const NOT_RESTORED_BASED_ON: &str = "CPF3220";
/// The restore library does not exist.
pub const NO_LIBRARY: &str = "CPF3780";
/// The requester may not restore the object.
pub const NOT_RESTORED_AUTH: &str = "CPF3757";
/// An owner that does not exist: the object went to the default owner.
pub const DEFAULT_OWNER: &str = "CPI3722";
/// A file's journal was not found: the file is not journaled.
pub const JOURNAL_MISSING: &str = "CPF7033";
