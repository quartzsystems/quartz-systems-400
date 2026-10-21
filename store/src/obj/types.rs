// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The object type registry. Codes are part of the on-disk format.

/// Which module gives a type its payload semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Module {
    /// `*LIB`: a directory of objects.
    Library,
    /// `*DTAARA`.
    DataArea,
    /// `*DTAQ`.
    DataQueue,
    /// `*USRSPC`.
    UserSpace,
    /// `*USRIDX`.
    UserIndex,
    /// `*USRQ`.
    UserQueue,
    /// `*SYSVAL`: the system value object.
    SysVal,
    /// Header only in Phase 2; the payload belongs to a later phase.
    Opaque,
}

/// One registered type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct TypeInfo {
    /// The code in IDs, keys and headers.
    pub code: u16,
    /// The `*NAME`.
    pub name: &'static str,
    /// Whether CRT commands may create it.
    pub user_creatable: bool,
    /// Whether it may live in QTEMP (the temporary ASP).
    pub may_be_temporary: bool,
    /// Whether it must live in QSYS.
    pub qsys_only: bool,
    /// Whether `Q` names are reserved for the system.
    pub q_reserved: bool,
    /// Payload semantics.
    pub module: Module,
}

#[allow(clippy::fn_params_excessive_bools)]
const fn t(
    code: u16,
    name: &'static str,
    user_creatable: bool,
    may_be_temporary: bool,
    qsys_only: bool,
    q_reserved: bool,
    module: Module,
) -> TypeInfo {
    TypeInfo {
        code,
        name,
        user_creatable,
        may_be_temporary,
        qsys_only,
        q_reserved,
        module,
    }
}

/// Code of `*LIB`.
pub const LIB: u16 = 0x0001;
/// Code of `*DTAARA`.
pub const DTAARA: u16 = 0x0002;
/// Code of `*DTAQ`.
pub const DTAQ: u16 = 0x0003;
/// Code of `*USRSPC`.
pub const USRSPC: u16 = 0x0004;
/// Code of `*USRIDX`.
pub const USRIDX: u16 = 0x0005;
/// Code of `*USRQ`.
pub const USRQ: u16 = 0x0006;
/// Code of `*MSGQ`.
pub const MSGQ: u16 = 0x0007;
/// Code of `*MSGF`.
pub const MSGF: u16 = 0x0008;
/// Code of `*JOBD`.
pub const JOBD: u16 = 0x0009;
/// Code of `*CLS`.
pub const CLS: u16 = 0x000A;
/// Code of `*SBSD`.
pub const SBSD: u16 = 0x000B;
/// Code of `*JOBQ`.
pub const JOBQ: u16 = 0x000C;
/// Code of `*OUTQ`.
pub const OUTQ: u16 = 0x000D;
/// Code of `*JOBSCD`.
pub const JOBSCD: u16 = 0x0021;
/// Code of `*USRPRF`.
pub const USRPRF: u16 = 0x000E;
/// Code of `*AUTL`.
pub const AUTL: u16 = 0x000F;
/// Code of `*FILE`.
pub const FILE: u16 = 0x0013;
/// Code of `*MODULE`.
pub const MODULE: u16 = 0x0014;
/// Code of `*PGM`.
pub const PGM: u16 = 0x0015;
/// Code of `*SRVPGM`.
pub const SRVPGM: u16 = 0x0016;
/// Code of `*BNDDIR`.
pub const BNDDIR: u16 = 0x0017;
/// Code of `*MBR`: a database file member, owned by its file and listed
/// in no library.
pub const MBR: u16 = 0x00FE;
/// Code of `*SYSVAL`.
pub const SYSVAL: u16 = 0x00FF;

/// Every type, in code order.
pub static TYPES: &[TypeInfo] = &[
    t(LIB, "*LIB", true, false, true, false, Module::Library),
    t(
        DTAARA,
        "*DTAARA",
        true,
        true,
        false,
        false,
        Module::DataArea,
    ),
    t(DTAQ, "*DTAQ", true, true, false, false, Module::DataQueue),
    t(
        USRSPC,
        "*USRSPC",
        true,
        true,
        false,
        false,
        Module::UserSpace,
    ),
    t(
        USRIDX,
        "*USRIDX",
        true,
        true,
        false,
        false,
        Module::UserIndex,
    ),
    t(USRQ, "*USRQ", false, true, false, false, Module::UserQueue),
    t(MSGQ, "*MSGQ", true, true, false, false, Module::Opaque),
    t(MSGF, "*MSGF", true, true, false, false, Module::Opaque),
    t(JOBD, "*JOBD", true, true, false, false, Module::Opaque),
    t(CLS, "*CLS", true, true, false, false, Module::Opaque),
    t(SBSD, "*SBSD", true, false, false, false, Module::Opaque),
    t(JOBQ, "*JOBQ", true, false, false, false, Module::Opaque),
    t(OUTQ, "*OUTQ", true, false, false, false, Module::Opaque),
    t(USRPRF, "*USRPRF", true, false, true, true, Module::Opaque),
    t(AUTL, "*AUTL", true, false, true, true, Module::Opaque),
    t(0x0010, "*CMD", true, true, false, false, Module::Opaque),
    t(0x0011, "*MENU", true, true, false, false, Module::Opaque),
    t(0x0012, "*PNLGRP", true, true, false, false, Module::Opaque),
    t(FILE, "*FILE", true, true, false, false, Module::Opaque),
    t(MODULE, "*MODULE", true, true, false, false, Module::Opaque),
    t(PGM, "*PGM", true, true, false, false, Module::Opaque),
    t(SRVPGM, "*SRVPGM", true, true, false, false, Module::Opaque),
    t(BNDDIR, "*BNDDIR", true, true, false, false, Module::Opaque),
    t(0x0018, "*JRN", true, false, false, false, Module::Opaque),
    t(0x0019, "*JRNRCV", true, false, false, false, Module::Opaque),
    t(0x001A, "*SAVF", true, true, false, false, Module::Opaque),
    t(0x001B, "*DEVD", true, false, true, true, Module::Opaque),
    t(0x001C, "*CTLD", true, false, true, true, Module::Opaque),
    t(0x001D, "*LIND", true, false, true, true, Module::Opaque),
    t(0x001E, "*SQLPKG", true, true, false, false, Module::Opaque),
    t(0x001F, "*LOCALE", true, true, false, false, Module::Opaque),
    t(0x0020, "*IMGCLG", true, false, false, false, Module::Opaque),
    t(JOBSCD, "*JOBSCD", true, false, false, false, Module::Opaque),
    t(MBR, "*MBR", false, true, false, false, Module::Opaque),
    t(SYSVAL, "*SYSVAL", false, false, true, true, Module::SysVal),
];

/// The type with `code`.
#[must_use]
pub fn type_by_code(code: u16) -> Option<&'static TypeInfo> {
    TYPES.iter().find(|t| t.code == code)
}

/// The type called `name` (`*LIB`, case-insensitive).
#[must_use]
pub fn type_by_name(name: &str) -> Option<&'static TypeInfo> {
    TYPES.iter().find(|t| t.name.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_consistent() {
        let mut codes: Vec<u16> = TYPES.iter().map(|t| t.code).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), TYPES.len(), "codes unique");
        assert_eq!(TYPES.len(), 35);
        assert!(TYPES.iter().all(|t| t.name.starts_with('*')));
        assert_eq!(type_by_name("*dtaara").unwrap().code, DTAARA);
        assert_eq!(type_by_code(SYSVAL).unwrap().module, Module::SysVal);
        assert!(!type_by_code(SYSVAL).unwrap().user_creatable);
        assert!(type_by_code(USRPRF).unwrap().qsys_only);
        assert!(type_by_name("*NOPE").is_none());
    }
}
