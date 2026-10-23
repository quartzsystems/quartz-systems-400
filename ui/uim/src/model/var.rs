// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Dialog variables, variable records and list definitions.

/// A `:VAR.`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variable {
    /// Name.
    pub name: String,
    /// The class.
    pub class: String,
    /// `VALUE`: the initial value.
    pub value: Option<String>,
    /// `SIZE`: a size override for pointer and character variables.
    pub size: Option<u32>,
    /// Source line.
    pub line: u32,
}

/// A `:VARRCD.`: the variables a program gets and puts as one record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VarRecord {
    /// Name.
    pub name: String,
    /// `VARS`.
    pub vars: Vec<String>,
    /// `NOGET`: variables the program never reads.
    pub noget: Vec<String>,
    /// `NOPUT`: variables the program never writes.
    pub noput: Vec<String>,
    /// `EXTVARS`: external names, one per variable.
    pub extvars: Vec<String>,
    /// Source line.
    pub line: u32,
}

/// A `:LISTDEF.`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListDef {
    /// Name.
    pub name: String,
    /// `VARS`: the variables of one entry.
    pub vars: Vec<String>,
    /// `MSGID`: the message shown when the list is empty.
    pub msgid: Option<String>,
    /// Source line.
    pub line: u32,
}
