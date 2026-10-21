// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The ILE object payloads and the records they share.

pub mod bnddir;
pub mod code;
pub mod module;
pub mod program;

use std::fmt;

pub use bnddir::{BndDir, BndDirEntry, BndEntryKind};
pub use module::{CreateData, ModuleObject};
pub use program::{BoundModule, ProgramObject};

use crate::binder::signature::Signature;

/// `*PGM` or `*SRVPGM`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PgmKind {
    /// A program with an entry procedure.
    Pgm,
    /// A service program with exports and signatures.
    SrvPgm,
}

impl PgmKind {
    /// The object type name.
    #[must_use]
    pub fn type_name(self) -> &'static str {
        match self {
            Self::Pgm => "*PGM",
            Self::SrvPgm => "*SRVPGM",
        }
    }
}

/// The activation group attribute of a program.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum ActGrpAttr {
    /// `ACTGRP(*NEW)`: one per call.
    New,
    /// `ACTGRP(*CALLER)`.
    #[default]
    Caller,
    /// A named activation group.
    Named(String),
    /// `DFTACTGRP(*YES)`: the default activation group.
    Default,
}

impl ActGrpAttr {
    /// From `*NEW`, `*CALLER`, `*DFTACTGRP` or a name.
    #[must_use]
    pub fn from_text(s: &str) -> Self {
        match s.to_ascii_uppercase().as_str() {
            "*NEW" => Self::New,
            "*CALLER" => Self::Caller,
            "*DFTACTGRP" => Self::Default,
            n => Self::Named(n.to_string()),
        }
    }
}

impl fmt::Display for ActGrpAttr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::New => f.write_str("*NEW"),
            Self::Caller => f.write_str("*CALLER"),
            Self::Named(n) => f.write_str(n),
            Self::Default => f.write_str("*DFTACTGRP"),
        }
    }
}

/// `USRPRF(*USER|*OWNER)`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum UserProfile {
    /// Run under the caller's profile.
    #[default]
    User,
    /// Run under the owner's profile; `MIGRATE(phase6)`.
    Owner,
}

/// The attributes of a program or service program.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct PgmAttr {
    /// Program or service program.
    pub kind: PgmKind,
    /// Activation group.
    pub actgrp: ActGrpAttr,
    /// User profile.
    pub usrprf: UserProfile,
    /// Use adopted authority; `MIGRATE(phase6)`.
    pub useadpaut: bool,
    /// User state rather than system state.
    pub user_state: bool,
    /// User domain rather than system domain.
    pub user_domain: bool,
    /// `ALWUPD(*YES)`.
    pub alwupd: bool,
    /// `ALWLIBUPD(*YES)`.
    pub alwlibupd: bool,
    /// Observable: the IR is kept so the program can be retranslated.
    pub observable: bool,
    /// `LVLCHK(*YES)`: callers check the signature; service programs.
    pub lvlchk: bool,
    /// Target release, `VxRyMz`.
    pub tgtrls: String,
    /// Unix seconds of creation.
    pub created: u64,
    /// Code generator version the native code came from.
    pub codegen_version: u32,
}

impl PgmAttr {
    /// Defaults for a kind: `*CALLER`, `*USER`, user state and domain,
    /// updates allowed, observable, level check on.
    #[must_use]
    pub fn new(kind: PgmKind) -> Self {
        Self {
            kind,
            actgrp: ActGrpAttr::Caller,
            usrprf: UserProfile::User,
            useadpaut: false,
            user_state: true,
            user_domain: true,
            alwupd: true,
            alwlibupd: false,
            observable: true,
            lvlchk: true,
            tgtrls: release::RELEASE.to_string(),
            created: 0,
            codegen_version: codegen::CODEGEN_VERSION,
        }
    }
}

/// What a symbol is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExportKind {
    /// A procedure.
    Proc,
    /// A static variable.
    Data,
}

impl ExportKind {
    /// The name shown in displays.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Proc => "*PROCEDURE",
            Self::Data => "*DATA",
        }
    }
}

/// A symbol a service program exports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Export {
    /// Symbol name.
    pub name: String,
    /// Procedure or data.
    pub kind: ExportKind,
    /// The bound module.
    pub module: u32,
    /// Procedure index in the module, for a procedure.
    pub index: u32,
    /// Offset in the program's static area, for data.
    pub offset: u64,
}

/// What an import resolved to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportTarget {
    /// A procedure of a bound module.
    Proc {
        /// The bound module.
        module: u32,
        /// Procedure index in it.
        proc: u32,
    },
    /// A static of a bound module, by its program static area offset.
    Data {
        /// Offset in the static area.
        offset: u64,
    },
    /// An export of a bound service program.
    SrvPgm {
        /// Index in the program's service program list.
        srvpgm: u32,
        /// Export name.
        export: String,
    },
    /// A procedure the runtime provides (`QLEAWI`).
    System,
    /// Nothing; the program was created with `OPTION(*UNRSLVREF)`.
    Unresolved,
}

/// An import of a bound module and where it went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportRes {
    /// The importing module.
    pub module: u32,
    /// Symbol name.
    pub name: String,
    /// Procedure or data.
    pub kind: ExportKind,
    /// Target.
    pub target: ImportTarget,
}

/// A service program a program is bound to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SrvPgmRef {
    /// Name.
    pub name: String,
    /// Library as bound, or `*LIBL`.
    pub lib: String,
    /// The signature the program was bound against.
    pub signature: Signature,
}
