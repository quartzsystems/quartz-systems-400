// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! How the compiler sees other files. `REF`, `REFFLD`, `FORMAT` and
//! `REFACCPTH` need the record formats of an existing file; the
//! database layer answers from the store, tests from a map.

pub mod map;

pub use map::MapRefs;

use qsname::{LibSpec, Name};

use crate::model::RecordFormat;

/// The resolver could not answer (store I/O, damaged object).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct RefError(pub String);

/// Looks up the record formats of a file.
pub trait RefResolver {
    /// Every record format of `file` in `library`, keys included, or
    /// `None` when there is no such file.
    fn formats(
        &self,
        library: &LibSpec,
        file: &Name,
    ) -> Result<Option<Vec<RecordFormat>>, RefError>;
}

/// A resolver that knows no files.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoRefs;

impl RefResolver for NoRefs {
    fn formats(&self, _: &LibSpec, _: &Name) -> Result<Option<Vec<RecordFormat>>, RefError> {
        Ok(None)
    }
}
