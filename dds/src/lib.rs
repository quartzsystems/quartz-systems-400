// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! DDS for QS/400: the fixed-column source form, the keyword grammar and
//! the physical and logical file compilers. Display and printer files
//! will share the lexer and the keyword parser.
//!
//! The compiler never touches the store. References to other files
//! (`REF`, `REFFLD`, `FORMAT`, `REFACCPTH`, `PFILE`, `JFILE`) go through
//! [`RefResolver`], which the database layer implements; [`MapRefs`]
//! serves tests.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod compile;
pub mod diag;
pub mod lex;
pub mod lf;
pub mod listing;
pub mod model;
pub mod pf;
pub mod refs;
pub mod source;

pub use compile::{Compilation, CompileOpts, FileKind};
pub use diag::{Diagnostic, Severity};
pub use lex::{Keyword, NameType, Parm, Spec};
pub use lf::compile_lf;
pub use model::{
    CompOp, Condition, DataType, DateFmt, DefaultValue, DupOrder, EditCode, FieldDef, FieldSource,
    FileRef, FloatPrecision, Join, KeyField, KeyTransform, LfDefinition, LfFormat, Literal,
    NullKeys, PfDefinition, RecordFormat, RefSource, Reference, SelectOmit, TimeFmt, Usage,
    Validity, VarLen,
};
pub use pf::compile_pf;
pub use refs::{MapRefs, NoRefs, RefError, RefResolver};
pub use source::{RECORD_LEN, Source, SourceLine};
