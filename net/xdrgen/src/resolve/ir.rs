// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The resolved model: every name checked, every constant a number,
//! every declaration a [`Shape`] the writer turns into Rust.

/// A base type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Base {
    /// `int`: `i32`.
    Int,
    /// `unsigned int`: `u32`.
    UInt,
    /// `hyper`: `i64`.
    Hyper,
    /// `unsigned hyper`: `u64`.
    UHyper,
    /// `bool`: `bool`.
    Bool,
}

/// How a declared item is held and coded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shape {
    /// A base type.
    Base(Base),
    /// A named type.
    Named(String),
    /// `opaque x[n]`: `[u8; n]`.
    FixedOpaque(u32),
    /// `opaque x<n>` or `string x<n>`: `Vec<u8>` of at most n.
    VarOpaque(u32),
    /// `T x[n]`: `Vec<T>` of exactly n.
    FixedArray(Box<Shape>, u32),
    /// `T x<n>`: `Vec<T>` of at most n.
    VarArray(Box<Shape>, u32),
    /// `T *x`: `Option<T>`, boxed when `T` contains itself.
    Optional(Box<Shape>, bool),
    /// `S *x` where `S` is a list node: `Vec<S>`, coded as the chain.
    List(String),
}

/// A union's discriminant type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Disc {
    /// `int`.
    Int,
    /// `unsigned int`.
    UInt,
    /// `bool`.
    Bool,
    /// The named enum.
    Enum(String),
}

/// A union arm for one discriminant value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arm {
    /// The Rust variant's name.
    pub variant: String,
    /// The discriminant's value.
    pub value: i128,
    /// For an enum discriminant, the enum member with that value.
    pub member: Option<String>,
    /// The arm's item; `None` is `void`.
    pub shape: Option<Shape>,
}

/// What a named type is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// An enum: its members and values.
    Enum(Vec<(String, i32)>),
    /// A struct: its fields (a list node's link left out).
    Struct(Vec<(String, Shape)>),
    /// A union.
    Union {
        /// The discriminant's type.
        disc: Disc,
        /// The arms.
        arms: Vec<Arm>,
        /// The default arm: `Some(None)` is `default: void`.
        default: Option<Option<Shape>>,
    },
    /// `pub type Name = ...;`.
    Alias(Shape),
    /// `pub struct Name(pub ...);`.
    Newtype(Shape),
}

/// A named type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ty {
    /// The name.
    pub name: String,
    /// What it is.
    pub kind: Kind,
    /// The fewest bytes it encodes to.
    pub min: u64,
    /// The description's line.
    pub line: u32,
}

/// A procedure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RProc {
    /// The name.
    pub name: String,
    /// The number.
    pub number: u32,
    /// The argument; `None` is `void`.
    pub arg: Option<Shape>,
    /// The result; `None` is `void`.
    pub result: Option<Shape>,
}

/// A program version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RVersion {
    /// The name.
    pub name: String,
    /// The number.
    pub number: u32,
    /// The procedures, in the description's order.
    pub procs: Vec<RProc>,
}

/// A program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RProgram {
    /// The name.
    pub name: String,
    /// The number.
    pub number: u32,
    /// The versions.
    pub versions: Vec<RVersion>,
}

/// A resolved description.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Model {
    /// The `const` definitions.
    pub consts: Vec<(String, i128)>,
    /// The types, in the description's order.
    pub types: Vec<Ty>,
    /// The programs.
    pub programs: Vec<RProgram>,
}

impl Model {
    /// The type named `name`.
    pub fn ty(&self, name: &str) -> Option<&Ty> {
        self.types.iter().find(|t| t.name == name)
    }
}
