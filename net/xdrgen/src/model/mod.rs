// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A description as parsed (RFC 4506 section 6, RFC 5531 section 12),
//! before any name is resolved.

use crate::error::Pos;

/// A constant: a number or the name of a constant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// A literal.
    Num(i128),
    /// A named constant, enum value or program, version or procedure.
    Name(String, Pos),
}

/// A type specifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeSpec {
    /// `int`.
    Int,
    /// `unsigned int`.
    UInt,
    /// `hyper`.
    Hyper,
    /// `unsigned hyper`.
    UHyper,
    /// `bool`.
    Bool,
    /// A named type (`name`, or `struct name` and the like).
    Named(String, Pos),
}

/// What a declaration declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclKind {
    /// `void`.
    Void,
    /// `T x`.
    Plain(TypeSpec),
    /// `T x[n]`.
    FixedArray(TypeSpec, Value),
    /// `T x<n>`; no bound is `<>`.
    VarArray(TypeSpec, Option<Value>),
    /// `opaque x[n]`.
    FixedOpaque(Value),
    /// `opaque x<n>`.
    VarOpaque(Option<Value>),
    /// `string x<n>`.
    String(Option<Value>),
    /// `T *x`.
    Optional(TypeSpec),
}

/// A declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decl {
    /// The declared name (none for `void`).
    pub name: Option<String>,
    /// What it declares.
    pub kind: DeclKind,
    /// Where.
    pub pos: Pos,
}

/// A union's arm with its labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Case {
    /// The labels, each with its position.
    pub labels: Vec<(Value, Pos)>,
    /// The arm.
    pub decl: Decl,
}

/// A union body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Union {
    /// The discriminant.
    pub disc: Decl,
    /// The arms.
    pub cases: Vec<Case>,
    /// The default arm.
    pub default: Option<Decl>,
}

/// A type definition's body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeBody {
    /// `enum { A = 1, ... }`.
    Enum(Vec<(String, Value, Pos)>),
    /// `struct { ... }`.
    Struct(Vec<Decl>),
    /// `union switch (...) { ... }`.
    Union(Union),
    /// `typedef declaration`.
    Typedef(Decl),
}

/// A named type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeDef {
    /// The name.
    pub name: String,
    /// Where.
    pub pos: Pos,
    /// The body.
    pub body: TypeBody,
}

/// A procedure of an RPC program's version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proc {
    /// The name.
    pub name: String,
    /// Where.
    pub pos: Pos,
    /// The argument type; `None` is `void`.
    pub arg: Option<TypeSpec>,
    /// The result type; `None` is `void`.
    pub result: Option<TypeSpec>,
    /// The procedure number.
    pub number: Value,
}

/// A version of an RPC program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    /// The name.
    pub name: String,
    /// Where.
    pub pos: Pos,
    /// The procedures.
    pub procs: Vec<Proc>,
    /// The version number.
    pub number: Value,
}

/// An RPC program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    /// The name.
    pub name: String,
    /// Where.
    pub pos: Pos,
    /// The versions.
    pub versions: Vec<Version>,
    /// The program number.
    pub number: Value,
}

/// A definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Def {
    /// `const NAME = value;`.
    Const(String, Value, Pos),
    /// A type.
    Type(TypeDef),
    /// A program.
    Program(Program),
}
