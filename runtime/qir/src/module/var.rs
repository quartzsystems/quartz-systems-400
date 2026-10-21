// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Variables and their storage classes.

use crate::types::Type;

/// Where a variable is declared.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Scope {
    /// In the module.
    Module,
    /// In a procedure.
    Proc,
}

/// A reference to a variable: its scope and index in that scope's list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VarRef {
    /// Scope.
    pub scope: Scope,
    /// Index in [`crate::Module::vars`] or [`crate::Proc::vars`].
    pub index: u32,
}

/// Storage class.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum StorageClass {
    /// In the module's static frame, one instance per activation group.
    Static,
    /// In the procedure's call frame.
    Auto,
    /// Addressed through a pointer variable.
    Based(VarRef),
    /// Static storage of another module, bound at bind time.
    Import,
    /// Read-only storage with an initializer.
    Const,
    /// The procedure's parameter with this index.
    Param(u32),
}

impl StorageClass {
    /// The keyword as written.
    #[must_use]
    pub fn keyword(&self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Auto => "auto",
            Self::Based(_) => "based",
            Self::Import => "import",
            Self::Const => "const",
            Self::Param(_) => "param",
        }
    }
}

/// A variable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Var {
    /// Name, unique in its scope.
    pub name: String,
    /// Type.
    pub ty: Type,
    /// Storage class.
    pub class: StorageClass,
    /// Exported for other modules to import; statics only.
    pub export: bool,
    /// Initial bytes, exactly the type's size; `None` leaves the
    /// type's default (blanks for text, zero for numbers, null pointers).
    pub inz: Option<Vec<u8>>,
}

impl Var {
    /// A variable without an initializer.
    #[must_use]
    pub fn new(name: &str, ty: Type, class: StorageClass) -> Self {
        Self {
            name: name.into(),
            ty,
            class,
            export: false,
            inz: None,
        }
    }
}
