// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A `*MODULE` IR unit: its attributes, types, variables, file
//! references, prototypes and procedures.

mod proc;
mod var;

use std::fmt;

pub use proc::{Block, Label, Param, Proc, ProcRef, Reg, RegInfo};
pub use var::{Scope, StorageClass, Var, VarRef};

use crate::types::Type;

/// The IR format version this crate writes. Within V1R it only grows.
pub const FORMAT_VERSION: u16 = 1;

/// A list index as the 32-bit form the IR uses.
#[must_use]
pub fn index32(i: usize) -> u32 {
    u32::try_from(i).expect("fewer than 2^32 items")
}

/// The source language a module was compiled from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Lang {
    /// Hand-written IR.
    #[default]
    Qir,
    /// ILE RPG.
    Rpgle,
    /// ILE CL.
    Clle,
}

impl Lang {
    /// The name as written in the text form.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Qir => "QIR",
            Self::Rpgle => "RPGLE",
            Self::Clle => "CLLE",
        }
    }

    /// From the text form.
    #[must_use]
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s {
            "QIR" => Self::Qir,
            "RPGLE" => Self::Rpgle,
            "CLLE" => Self::Clle,
            _ => return None,
        })
    }
}

/// The activation group the source asked for.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum ActGrp {
    /// `*NEW`: one per call.
    New,
    /// `*CALLER`: the caller's.
    #[default]
    Caller,
    /// A named activation group.
    Named(String),
}

impl fmt::Display for ActGrp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::New => f.write_str("*NEW"),
            Self::Caller => f.write_str("*CALLER"),
            Self::Named(n) => f.write_str(n),
        }
    }
}

/// Debug view level requested at compile time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DbgView {
    /// `*NONE`.
    #[default]
    None,
    /// `*STMT`.
    Stmt,
    /// `*SOURCE`.
    Source,
    /// `*LIST`.
    List,
    /// `*ALL`.
    All,
}

impl DbgView {
    /// The name as written.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "*NONE",
            Self::Stmt => "*STMT",
            Self::Source => "*SOURCE",
            Self::List => "*LIST",
            Self::All => "*ALL",
        }
    }

    /// From the text form.
    #[must_use]
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s {
            "*NONE" => Self::None,
            "*STMT" => Self::Stmt,
            "*SOURCE" => Self::Source,
            "*LIST" => Self::List,
            "*ALL" => Self::All,
            _ => return None,
        })
    }
}

/// Module options the source set.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Options {
    /// `*SRCSTMT`: statement numbers are source statement numbers.
    pub srcstmt: bool,
    /// `*NODEBUGIO`: no statement stops on I/O.
    pub nodebugio: bool,
    /// Debug view.
    pub dbgview: DbgView,
}

/// A named type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeDef {
    /// The name.
    pub name: String,
    /// The type.
    pub ty: Type,
}

/// How a file is opened.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FileUsage {
    /// `*INPUT`.
    #[default]
    Input,
    /// `*OUTPUT`.
    Output,
    /// `*UPDATE`: input and update.
    Update,
    /// `*ALL`: input, output, update and delete.
    All,
}

impl FileUsage {
    /// The name as written.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Input => "*INPUT",
            Self::Output => "*OUTPUT",
            Self::Update => "*UPDATE",
            Self::All => "*ALL",
        }
    }

    /// From the text form.
    #[must_use]
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s {
            "*INPUT" => Self::Input,
            "*OUTPUT" => Self::Output,
            "*UPDATE" => Self::Update,
            "*ALL" => Self::All,
            _ => return None,
        })
    }
}

/// A record format the module expects, with the level identifier it
/// was compiled against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormatRef {
    /// Format name.
    pub name: String,
    /// 13 hexadecimal digits, or none when the compiler did not check.
    pub level: Option<String>,
}

/// A file the module uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRef {
    /// The name instructions use.
    pub name: String,
    /// Object name.
    pub file: String,
    /// Library, `*LIBL` by default.
    pub lib: String,
    /// Usage.
    pub usage: FileUsage,
    /// Opened for keyed access.
    pub keyed: bool,
    /// Level check at open.
    pub lvlchk: bool,
    /// The formats and their level identifiers.
    pub formats: Vec<FormatRef>,
}

/// How a prototype binds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Linkage {
    /// A bound procedure imported at bind time.
    Import,
    /// A program called dynamically by name.
    Pgm {
        /// Program name.
        name: String,
        /// Library, `*LIBL` by default.
        lib: String,
    },
    /// A system API in `QSYS`.
    SysApi,
    /// A signature only, for calls through procedure pointers.
    Signature,
}

/// A prototype: an imported procedure, a program, a system API or a
/// bare signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proto {
    /// Name.
    pub name: String,
    /// Parameters.
    pub params: Vec<Param>,
    /// Return type.
    pub ret: Option<Type>,
    /// Operational descriptors are passed.
    pub opdesc: bool,
    /// Linkage.
    pub linkage: Linkage,
}

/// Index of a prototype in [`Module::protos`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProtoRef(pub u32);

/// A `*MODULE` IR unit.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Module {
    /// Module name.
    pub name: String,
    /// Source language.
    pub lang: Lang,
    /// CCSID of character data.
    pub ccsid: u16,
    /// Requested activation group.
    pub actgrp: ActGrp,
    /// `DFTACTGRP(*YES)`.
    pub dftactgrp: bool,
    /// `NOMAIN`: no program entry procedure.
    pub nomain: bool,
    /// Options.
    pub options: Options,
    /// Named types, in declaration order.
    pub types: Vec<TypeDef>,
    /// Module-scope variables.
    pub vars: Vec<Var>,
    /// Files.
    pub files: Vec<FileRef>,
    /// Prototypes.
    pub protos: Vec<Proto>,
    /// Procedures.
    pub procs: Vec<Proc>,
}

impl Module {
    /// An empty module with the given name and default attributes.
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            ccsid: 37,
            ..Self::default()
        }
    }

    /// The variable a reference names.
    #[must_use]
    pub fn var<'a>(&'a self, r: VarRef, proc: Option<&'a Proc>) -> Option<&'a Var> {
        match r.scope {
            Scope::Module => self.vars.get(r.index as usize),
            Scope::Proc => proc.and_then(|p| p.vars.get(r.index as usize)),
        }
    }

    /// A procedure by name.
    #[must_use]
    pub fn proc_named(&self, name: &str) -> Option<(ProcRef, &Proc)> {
        self.procs
            .iter()
            .enumerate()
            .find(|(_, p)| p.name == name)
            .map(|(i, p)| (ProcRef(index32(i)), p))
    }

    /// A prototype by name.
    #[must_use]
    pub fn proto_named(&self, name: &str) -> Option<(ProtoRef, &Proto)> {
        self.protos
            .iter()
            .enumerate()
            .find(|(_, p)| p.name == name)
            .map(|(i, p)| (ProtoRef(index32(i)), p))
    }

    /// A named type.
    #[must_use]
    pub fn type_named(&self, name: &str) -> Option<&Type> {
        self.types.iter().find(|t| t.name == name).map(|t| &t.ty)
    }

    /// A file reference by name.
    #[must_use]
    pub fn file_named(&self, name: &str) -> Option<(u32, &FileRef)> {
        self.files
            .iter()
            .enumerate()
            .find(|(_, f)| f.name == name)
            .map(|(i, f)| (index32(i), f))
    }

    /// The program entry procedure, if any.
    #[must_use]
    pub fn entry(&self) -> Option<(ProcRef, &Proc)> {
        self.procs
            .iter()
            .enumerate()
            .find(|(_, p)| p.entry)
            .map(|(i, p)| (ProcRef(index32(i)), p))
    }
}
