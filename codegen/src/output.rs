// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What code generation produces for a module: native code, the
//! relocations the loader resolves, the procedure table and the maps
//! the debugger and the call stack display use.

use crate::abi::StaticLayout;
use crate::rt::RtFn;

/// The code generator's version. A `*PGM` records it; a different
/// release retranslates from the IR.
pub const CODEGEN_VERSION: u32 = 1;

/// What a relocation refers to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelocTarget {
    /// A runtime function by table index.
    Runtime(RtFn),
    /// A procedure another module exports, by name.
    ImportProc(String),
    /// This module's static frame offset inside the program's static
    /// area, as a plain value.
    StaticFrame,
    /// A static another module exports, by name: its offset inside the
    /// program's static area, as a plain value.
    ImportData(String),
}

/// How a relocation is applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelocKind {
    /// An 8-byte absolute value at the offset.
    Abs8,
    /// A 4-byte PC-relative displacement at the offset: `target -
    /// (offset + 4)` plus the addend.
    PcRel4,
}

/// A relocation in the module's code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reloc {
    /// Byte offset in the module's code.
    pub offset: u32,
    /// How it is applied.
    pub kind: RelocKind,
    /// What it refers to.
    pub target: RelocTarget,
    /// Added to the resolved value.
    pub addend: i64,
}

/// A statement's native range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StmtRange {
    /// Statement number from `debug.stmt`.
    pub stmt: u32,
    /// First byte of its code, from the module's code start.
    pub start: u32,
    /// One past its last byte.
    pub end: u32,
}

/// A compiled procedure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcCode {
    /// Procedure name.
    pub name: String,
    /// Index in the module.
    pub index: u32,
    /// Offset of its entry in the module's code.
    pub offset: u32,
    /// Code bytes.
    pub size: u32,
    /// Exported from the module.
    pub export: bool,
    /// The program entry procedure.
    pub entry: bool,
    /// Statement ranges, in code order.
    pub stmts: Vec<StmtRange>,
}

/// The output for one module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleCode {
    /// Module name.
    pub name: String,
    /// Code generator version.
    pub version: u32,
    /// Native code, every procedure concatenated and 16-byte aligned.
    pub code: Vec<u8>,
    /// Relocations to apply when the code is placed.
    pub relocs: Vec<Reloc>,
    /// Procedures in module order.
    pub procs: Vec<ProcCode>,
    /// The static frame.
    pub statics: StaticLayout,
    /// Initial bytes of the static frame.
    pub static_init: Vec<u8>,
    /// Names of exported statics with their frame offsets.
    pub data_exports: Vec<(String, u64)>,
}

impl ModuleCode {
    /// A procedure by name.
    #[must_use]
    pub fn proc_named(&self, name: &str) -> Option<&ProcCode> {
        self.procs.iter().find(|p| p.name == name)
    }

    /// The procedure whose code holds an offset.
    #[must_use]
    pub fn proc_at(&self, offset: u32) -> Option<&ProcCode> {
        self.procs
            .iter()
            .find(|p| p.offset <= offset && offset < p.offset + p.size)
    }

    /// The statement at a code offset, if any.
    #[must_use]
    pub fn stmt_at(&self, offset: u32) -> Option<u32> {
        let p = self.proc_at(offset)?;
        p.stmts
            .iter()
            .find(|s| s.start <= offset && offset < s.end)
            .map(|s| s.stmt)
    }
}
