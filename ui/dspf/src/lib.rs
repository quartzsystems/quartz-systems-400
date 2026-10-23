// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Display files for QS/400: the DDS display-file compiler behind
//! `CRTDSPF`, the model it produces, the record buffers a program sees
//! and the payload the `*FILE` object keeps. See `docs/DSPF.md`.
//!
//! The crate is a library with no runtime: it never writes a 5250 data
//! stream and never touches the store. The source form, the keyword
//! grammar and the diagnostics come from the `dds` crate; references to
//! other files go through [`RefResolver`].

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod compile;
pub mod cond;
pub mod diag;
pub mod kw;
pub mod layout;
pub mod listing;
pub mod model;
pub mod payload;
pub mod spec;

pub use compile::compile_dspf;
pub use cond::{Cond, Ind};
pub use dds::{
    Compilation, CompileOpts, Diagnostic, MapRefs, NoRefs, RefResolver, Severity, Source,
};
pub use kw::{KwInfo, Level, Support};
pub use model::{
    Attr, Buffer, BufferField, Clrl, CmdKey, Constant, DisplayFile, DisplaySize, Edit, ErrMsg,
    Field, FieldType, FileAttrs, KeyName, KeywordUse, Position, Record, RecordKind, RecordLayout,
    SflEnd, SflRole, SflSize, Slno, Subfile, TypeClass, Usage, Window, WindowOrigin,
};
pub use payload::{PayloadError, decode, encode};
