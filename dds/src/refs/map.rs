// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An in-memory resolver for tests and tools.

use std::collections::BTreeMap;

use qsname::{LibSpec, Name};

use super::{RefError, RefResolver};
use crate::model::{PfDefinition, RecordFormat};

/// Files by library and name. `*LIBL` and `*CURLIB` find the first
/// library, in EBCDIC order, that holds the file.
#[derive(Clone, Debug, Default)]
pub struct MapRefs {
    files: BTreeMap<(Name, Name), Vec<RecordFormat>>,
}

impl MapRefs {
    /// Add a file with its formats.
    pub fn insert(&mut self, library: Name, file: Name, formats: Vec<RecordFormat>) {
        self.files.insert((library, file), formats);
    }

    /// Add a compiled physical file.
    pub fn insert_pf(&mut self, library: &str, file: &str, def: &PfDefinition) {
        let library = Name::parse(library).expect("library name");
        let file = Name::parse(file).expect("file name");
        self.insert(library, file, vec![def.format.clone()]);
    }
}

impl RefResolver for MapRefs {
    fn formats(
        &self,
        library: &LibSpec,
        file: &Name,
    ) -> Result<Option<Vec<RecordFormat>>, RefError> {
        let found = match library {
            LibSpec::Name(lib) => self.files.get(&(*lib, *file)),
            LibSpec::Libl | LibSpec::Curlib => self
                .files
                .iter()
                .find(|((_, f), _)| f == file)
                .map(|(_, v)| v),
        };
        Ok(found.cloned())
    }
}
