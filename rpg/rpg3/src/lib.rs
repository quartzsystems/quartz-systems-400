// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! RPG III (RPG/400): `CVTRPGSRC` and `CRTRPGPGM`. The converter turns
//! an RPG III member into the ILE RPG fixed form IBM's converter writes
//! (`cvt`), with its report (`report`); the compiler compiles that
//! conversion with the ILE RPG compiler and reports in RPG/400's terms
//! (`crt`): `QRG` diagnostics on the RPG III statements, the RPG/400
//! listing. `docs/RPG3.md` describes both.

#![deny(missing_docs)]

pub mod crt;
pub mod cvt;
pub mod report;

pub use crt::{Compiled, Options, compile};
pub use cvt::{Conversion, convert};
