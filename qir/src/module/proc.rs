// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Procedures: parameters, registers, blocks.

use super::{Var, index32};
use crate::inst::Inst;
use crate::types::Type;

/// Index of a procedure in [`crate::Module::procs`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProcRef(pub u32);

/// A virtual register, indexed into [`Proc::regs`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Reg(pub u32);

/// A block label, indexed into [`Proc::blocks`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Label(pub u32);

/// A register's name and type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegInfo {
    /// Name without the `%`.
    pub name: String,
    /// Type; always a register type.
    pub ty: Type,
}

/// A parameter of a procedure or prototype.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Param {
    /// Name without the `$`.
    pub name: String,
    /// Type.
    pub ty: Type,
    /// Passed by value; by reference otherwise.
    pub by_value: bool,
    /// `*NOPASS`: the caller may leave it out.
    pub nopass: bool,
    /// `*OMIT`: the caller may pass `*OMIT`.
    pub omit: bool,
    /// `*VARSIZE`: the caller may pass something shorter.
    pub varsize: bool,
    /// `CONST`: by reference, not changed by the callee.
    pub const_: bool,
}

impl Param {
    /// A by-reference parameter with no options.
    #[must_use]
    pub fn by_ref(name: &str, ty: Type) -> Self {
        Self {
            name: name.into(),
            ty,
            by_value: false,
            nopass: false,
            omit: false,
            varsize: false,
            const_: false,
        }
    }

    /// A by-value parameter with no options.
    #[must_use]
    pub fn by_value(name: &str, ty: Type) -> Self {
        Self {
            by_value: true,
            ..Self::by_ref(name, ty)
        }
    }
}

/// A basic block: a label and instructions ending in a terminator.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    /// Label name.
    pub label: String,
    /// Instructions.
    pub insts: Vec<Inst>,
}

/// A procedure.
#[derive(Clone, Debug, PartialEq)]
pub struct Proc {
    /// Name.
    pub name: String,
    /// Parameters; each also appears in `vars` as `StorageClass::Param`.
    pub params: Vec<Param>,
    /// Return type.
    pub ret: Option<Type>,
    /// Exported from the module.
    pub export: bool,
    /// The program entry procedure.
    pub entry: bool,
    /// Expects operational descriptors.
    pub opdesc: bool,
    /// Procedure-scope variables: parameters, automatics, statics, based.
    pub vars: Vec<Var>,
    /// Registers.
    pub regs: Vec<RegInfo>,
    /// Blocks; the first is the entry block.
    pub blocks: Vec<Block>,
}

impl Proc {
    /// An empty procedure.
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            params: Vec::new(),
            ret: None,
            export: false,
            entry: false,
            opdesc: false,
            vars: Vec::new(),
            regs: Vec::new(),
            blocks: Vec::new(),
        }
    }

    /// A block by label name.
    #[must_use]
    pub fn label_named(&self, name: &str) -> Option<Label> {
        self.blocks
            .iter()
            .position(|b| b.label == name)
            .map(|i| Label(index32(i)))
    }

    /// A register by name.
    #[must_use]
    pub fn reg_named(&self, name: &str) -> Option<Reg> {
        self.regs
            .iter()
            .position(|r| r.name == name)
            .map(|i| Reg(index32(i)))
    }

    /// The type of a register.
    #[must_use]
    pub fn reg_type(&self, r: Reg) -> Option<&Type> {
        self.regs.get(r.0 as usize).map(|r| &r.ty)
    }

    /// The name of a register.
    #[must_use]
    pub fn reg_name(&self, r: Reg) -> &str {
        self.regs.get(r.0 as usize).map_or("?", |r| r.name.as_str())
    }

    /// The name of a label.
    #[must_use]
    pub fn label_name(&self, l: Label) -> &str {
        self.blocks
            .get(l.0 as usize)
            .map_or("?", |b| b.label.as_str())
    }
}
