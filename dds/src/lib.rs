// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! DDS for QS/400: the fixed-column source form, the keyword grammar and
//! the physical file compiler. Logical, display and printer files share
//! the lexer and the keyword parser.
//!
//! The compiler never touches the store. References to other files
//! (`REF`, `REFFLD`, `FORMAT`, `REFACCPTH`) go through [`RefResolver`],
//! which the database layer implements; [`MapRefs`] serves tests.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod compile;
pub mod diag;
pub mod lex;
pub mod listing;
pub mod model;
pub mod pf;
pub mod refs;
pub mod source;

pub use compile::{Compilation, CompileOpts, FileKind};
pub use diag::{Diagnostic, Severity};
pub use lex::{Keyword, NameType, Parm, Spec};
pub use model::{
    CompOp, DataType, DateFmt, DefaultValue, DupOrder, EditCode, FieldDef, FileRef, FloatPrecision,
    KeyField, KeyTransform, Literal, NullKeys, PfDefinition, RecordFormat, RefSource, Reference,
    TimeFmt, Usage, Validity, VarLen,
};
pub use pf::compile_pf;
pub use refs::{MapRefs, NoRefs, RefError, RefResolver};
pub use source::{RECORD_LEN, Source, SourceLine};
