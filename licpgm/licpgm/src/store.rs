// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the integrator provides: the `*PRDDFN` and `*PRDLOD` objects,
//! the license and agreement records, and a view of other objects.
//! [`MemStore`] is the in-memory one the tests use.

use std::collections::{BTreeMap, BTreeSet};

use release::Release;
use thiserror::Error;

use crate::agree::AgreementBook;
use crate::id::ProductId;
use crate::license::repo::LicenseRepo;
use crate::model::QualName;
use crate::model::definition::ProductDefinition;
use crate::model::load::{LoadKey, ObjEntry, ProductLoad};

/// A storage operation that failed.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum StoreError {
    /// The object exists.
    #[error("object {0} already exists")]
    Exists(QualName),
    /// The object does not exist.
    #[error("object {0} not found")]
    NotFound(QualName),
    /// The library does not exist.
    #[error("library {0} not found")]
    NoLibrary(String),
    /// Anything else, in the integrator's words.
    #[error("{0}")]
    Other(String),
}

/// The product objects and records.
pub trait ProductStore {
    /// Every `*PRDDFN`.
    fn definitions(&self) -> Vec<(QualName, ProductDefinition)>;
    /// Create a `*PRDDFN`.
    fn create_definition(&mut self, at: &QualName, d: &ProductDefinition)
    -> Result<(), StoreError>;
    /// Delete a `*PRDDFN`.
    fn delete_definition(&mut self, at: &QualName) -> Result<(), StoreError>;
    /// Every `*PRDLOD`.
    fn loads(&self) -> Vec<(QualName, ProductLoad)>;
    /// Create a `*PRDLOD`.
    fn create_load(&mut self, at: &QualName, l: &ProductLoad) -> Result<(), StoreError>;
    /// Replace the payload of an existing `*PRDLOD` (state changes).
    fn update_load(&mut self, at: &QualName, l: &ProductLoad) -> Result<(), StoreError>;
    /// Delete a `*PRDLOD`.
    fn delete_load(&mut self, at: &QualName) -> Result<(), StoreError>;
    /// Whether a library exists.
    fn library_exists(&self, lib: &str) -> bool;
    /// The license repository.
    fn licenses(&self) -> LicenseRepo;
    /// Save the license repository.
    fn save_licenses(&mut self, r: &LicenseRepo) -> Result<(), StoreError>;
    /// The agreement answers.
    fn agreements(&self) -> AgreementBook;
    /// Save the agreement answers.
    fn save_agreements(&mut self, a: &AgreementBook) -> Result<(), StoreError>;
}

/// How an object or path looks to CHKPRDOPT and packaging.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjStatus {
    /// Not there.
    Missing,
    /// There and damaged.
    Damaged,
    /// There; with the product release its description records, if any.
    Present(Option<Release>),
}

/// The integrator's view of objects outside the product records.
pub trait ObjectCheck {
    /// One library object.
    fn object(&self, o: &ObjEntry) -> ObjStatus;
    /// Every object in a library (for `*ALL` entries).
    fn objects_in(&self, lib: &str) -> Vec<ObjEntry>;
    /// One IFS path.
    fn path(&self, path: &str) -> ObjStatus;
}

/// The definition of a product at a release.
#[must_use]
pub fn find_definition(
    s: &dyn ProductStore,
    p: &ProductId,
    r: Release,
) -> Option<(QualName, ProductDefinition)> {
    s.definitions()
        .into_iter()
        .find(|(_, d)| &d.product == p && d.release == r)
}

/// The load with a key.
#[must_use]
pub fn find_load(s: &dyn ProductStore, k: &LoadKey) -> Option<(QualName, ProductLoad)> {
    s.loads().into_iter().find(|(_, l)| &l.key == k)
}

/// Everything in memory.
#[derive(Clone, Debug, Default)]
pub struct MemStore {
    /// `*PRDDFN`s.
    pub defs: BTreeMap<QualName, ProductDefinition>,
    /// `*PRDLOD`s.
    pub lods: BTreeMap<QualName, ProductLoad>,
    /// Libraries.
    pub libs: BTreeSet<String>,
    /// License records.
    pub lic: LicenseRepo,
    /// Agreement answers.
    pub agr: AgreementBook,
    /// Other objects and their states.
    pub objects: BTreeMap<ObjEntry, ObjStatus>,
    /// IFS paths and their states.
    pub paths: BTreeMap<String, ObjStatus>,
}

impl MemStore {
    /// A store with the given libraries.
    #[must_use]
    pub fn with_libs(libs: &[&str]) -> Self {
        Self {
            libs: libs.iter().map(|s| (*s).to_string()).collect(),
            ..Self::default()
        }
    }

    /// Add an object in a good state.
    pub fn add_object(&mut self, lib: &str, name: &str, typ: &str) {
        self.libs.insert(lib.to_string());
        self.objects
            .insert(ObjEntry::new(lib, name, typ), ObjStatus::Present(None));
    }
}

impl ProductStore for MemStore {
    fn definitions(&self) -> Vec<(QualName, ProductDefinition)> {
        self.defs
            .iter()
            .map(|(q, d)| (q.clone(), d.clone()))
            .collect()
    }

    fn create_definition(
        &mut self,
        at: &QualName,
        d: &ProductDefinition,
    ) -> Result<(), StoreError> {
        if !self.libs.contains(&at.lib) {
            return Err(StoreError::NoLibrary(at.lib.clone()));
        }
        if self.defs.contains_key(at) {
            return Err(StoreError::Exists(at.clone()));
        }
        self.defs.insert(at.clone(), d.clone());
        Ok(())
    }

    fn delete_definition(&mut self, at: &QualName) -> Result<(), StoreError> {
        self.defs
            .remove(at)
            .map(|_| ())
            .ok_or_else(|| StoreError::NotFound(at.clone()))
    }

    fn loads(&self) -> Vec<(QualName, ProductLoad)> {
        self.lods
            .iter()
            .map(|(q, l)| (q.clone(), l.clone()))
            .collect()
    }

    fn create_load(&mut self, at: &QualName, l: &ProductLoad) -> Result<(), StoreError> {
        if !self.libs.contains(&at.lib) {
            return Err(StoreError::NoLibrary(at.lib.clone()));
        }
        if self.lods.contains_key(at) {
            return Err(StoreError::Exists(at.clone()));
        }
        self.lods.insert(at.clone(), l.clone());
        Ok(())
    }

    fn update_load(&mut self, at: &QualName, l: &ProductLoad) -> Result<(), StoreError> {
        match self.lods.get_mut(at) {
            Some(x) => {
                *x = l.clone();
                Ok(())
            }
            None => Err(StoreError::NotFound(at.clone())),
        }
    }

    fn delete_load(&mut self, at: &QualName) -> Result<(), StoreError> {
        self.lods
            .remove(at)
            .map(|_| ())
            .ok_or_else(|| StoreError::NotFound(at.clone()))
    }

    fn library_exists(&self, lib: &str) -> bool {
        self.libs.contains(lib)
    }

    fn licenses(&self) -> LicenseRepo {
        self.lic.clone()
    }

    fn save_licenses(&mut self, r: &LicenseRepo) -> Result<(), StoreError> {
        self.lic = r.clone();
        Ok(())
    }

    fn agreements(&self) -> AgreementBook {
        self.agr.clone()
    }

    fn save_agreements(&mut self, a: &AgreementBook) -> Result<(), StoreError> {
        self.agr = a.clone();
        Ok(())
    }
}

impl ObjectCheck for MemStore {
    fn object(&self, o: &ObjEntry) -> ObjStatus {
        self.objects.get(o).copied().unwrap_or(ObjStatus::Missing)
    }

    fn objects_in(&self, lib: &str) -> Vec<ObjEntry> {
        self.objects
            .keys()
            .filter(|o| o.lib == lib)
            .cloned()
            .collect()
    }

    fn path(&self, path: &str) -> ObjStatus {
        self.paths.get(path).copied().unwrap_or(ObjStatus::Missing)
    }
}
