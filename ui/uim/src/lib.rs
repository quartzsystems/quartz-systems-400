// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! UIM for QS/400: the panel-group tag language as `CRTPNLGRP` and
//! `CRTMNU TYPE(*UIM)` read it, the model a renderer walks, the
//! checks, the listing, the `*PNLGRP` payload and the panel renderer
//! ([`render`]).
//!
//! The crate never renders and never touches the store. Imbedded
//! members come through [`Imbed`] and the names other groups export
//! through [`Imports`]; [`MapImbeds`] and [`MapImports`] serve tests.
//! `docs/UIM.md` is the reference.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod compile;
pub mod diag;
pub mod expr;
pub mod lex;
pub mod listing;
pub mod model;
pub mod payload;
pub mod render;
pub mod source;
pub mod spec;
pub mod tags;
pub mod tree;

pub use compile::{Compilation, CompileOpts, Imports, MapImports, NoImports, compile};
pub use diag::{Diagnostic, Diagnostics, Severity};
pub use model::PanelGroup;
pub use payload::{Payload, PayloadError};
pub use render::{Dialog, ListState, MapDialog, Options, RenderError, Screen, render, render_help};
pub use source::{Imbed, MapImbeds, NoImbeds, Source, SourceLine};
