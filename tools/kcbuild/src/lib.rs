// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Knowledge Center for QS/400: a static site, in the plain style of
//! `docs/WEB_STYLE.md`, built from what the system ships: every command
//! definition with its parameters and help, every message in every
//! shipped message file, the documents under `docs/`, and the
//! conformance matrix (where each command runs). `build kc` writes it;
//! the install media carries it and `QS00AFW` serves it from
//! `/QQS/ProdData/KnowledgeCenter`.

pub mod md;
mod page;
mod site;

pub use site::{Summary, build};
