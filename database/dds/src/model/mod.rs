// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a compile produces: fields, keys, record formats and file
//! definitions. These are the compiler's output; the database layer
//! turns them into `*FILE` payloads.

pub mod field;
pub mod format;
pub mod key;
pub mod lf;
pub mod pf;

use std::fmt;

use qsname::{LibSpec, Name};

pub use field::{
    CompOp, DataType, DateFmt, DefaultValue, EditCode, FieldDef, FloatPrecision, IdentityDef,
    Literal, RefSource, Reference, TimeFmt, Usage, Validity, VarLen,
};
pub use format::RecordFormat;
pub use key::{KeyField, KeyTransform};
pub use lf::{Condition, FieldSource, Join, LfDefinition, LfFormat, SelectOmit};
pub use pf::{DupOrder, NullKeys, PfDefinition};

/// A file named in a keyword: `LIB/FILE` or `FILE` (`*LIBL`), with an
/// optional record format name where the keyword takes one.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FileRef {
    /// The library, `*LIBL` when not given.
    pub library: LibSpec,
    /// The file.
    pub file: Name,
    /// A record format of the file, when the keyword named one.
    pub format: Option<Name>,
}

impl FileRef {
    /// Parse `LIB/FILE` or `FILE`.
    pub fn parse(word: &str) -> Result<Self, String> {
        let (library, file) = match word.split_once('/') {
            Some((lib, file)) => (lib.parse::<LibSpec>().map_err(|e| e.to_string())?, file),
            None => (LibSpec::Libl, word),
        };
        let file = Name::parse(file).map_err(|e| e.to_string())?;
        Ok(Self {
            library,
            file,
            format: None,
        })
    }

    /// The same reference naming a record format.
    #[must_use]
    pub fn with_format(mut self, format: Name) -> Self {
        self.format = Some(format);
        self
    }
}

impl fmt::Display for FileRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.library, self.file)?;
        if let Some(fmt) = &self.format {
            write!(f, " {fmt}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_refs() {
        let r = FileRef::parse("qgpl/refs").unwrap();
        assert_eq!(r.to_string(), "QGPL/REFS");
        let r = FileRef::parse("REFS").unwrap();
        assert_eq!(r.library, LibSpec::Libl);
        assert_eq!(r.to_string(), "*LIBL/REFS");
        assert!(FileRef::parse("A/B/C").is_err());
        assert!(FileRef::parse("*ALL/X").is_err());
        assert!(FileRef::parse("").is_err());
    }
}
