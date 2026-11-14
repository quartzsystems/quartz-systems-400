// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The entry store over the object store: `*DIRDB` (docs/DIRSRV.md §5).

pub mod normalize;

pub use normalize::{AsciiFold, NormError, Normalizer, Rule};
