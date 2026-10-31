// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! RSTLICPGM, DLTLICPGM and SAVLICPGM: what each touches and in what
//! order. Media, the object restore, deletion and exit programs are
//! the integrator's, behind the traits here.

pub mod delete;
pub mod restore;
pub mod save;

use thiserror::Error;

use crate::agree::AgreementError;
use crate::id::{LoadId, OptionNo, ProductId};
use crate::model::QualName;
use crate::model::definition::{Prereq, ProductDefinition};
use crate::model::load::{LoadKey, ObjEntry, ProductLoad};
use crate::model::state::LoadState;
use crate::store::StoreError;

/// Libraries the operating system owns: never deleted or saved whole
/// for one product; their product objects are handled one by one.
pub const SYSTEM_LIBS: &[&str] = &[
    "QSYS", "QSYS2", "QUSRSYS", "QGPL", "QTEMP", "QHLPSYS", "QSPL",
];

/// Directories never deleted for one product.
pub const SYSTEM_DIRS: &[&str] = &[
    "/",
    "/QOpenSys",
    "/QIBM",
    "/QIBM/ProdData",
    "/QIBM/UserData",
    "/QSYS.LIB",
    "/home",
    "/tmp",
];

/// A load on the media with its product's definition.
#[derive(Clone, Debug, PartialEq)]
pub struct MediaLoad {
    /// Where the `*PRDDFN` goes.
    pub definition_name: QualName,
    /// The definition.
    pub definition: ProductDefinition,
    /// Where the `*PRDLOD` goes.
    pub load_name: QualName,
    /// The load as packaged.
    pub load: ProductLoad,
}

/// One part of a load's save data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Part {
    /// A library, by its development name.
    Library(String),
    /// An IFS directory, by its primary path.
    Directory(String),
}

/// A medium went wrong.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
#[error("{0}")]
pub struct MediaError(pub String);

/// Installation media (an optical image, a save file set, a directory
/// of loads).
pub trait Media {
    /// Every load on it.
    fn loads(&self) -> Vec<MediaLoad>;
    /// The save stream of one part of a load.
    fn save_data(&self, key: &LoadKey, part: &Part) -> Result<Vec<u8>, MediaError>;
}

/// The restore did not work.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
#[error("{0}")]
pub struct RestoreError(pub String);

/// The save engine's restore.
pub trait Restorer {
    /// Restore a library's save stream into a library; returns the
    /// objects restored.
    fn restore(&mut self, stream: &[u8], library: &str) -> Result<Vec<ObjEntry>, RestoreError>;
    /// Restore a directory's save stream under a path; returns the
    /// paths restored.
    fn restore_directory(&mut self, stream: &[u8], path: &str)
    -> Result<Vec<String>, RestoreError>;
}

/// Why an exit program is called.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitOp {
    /// RSTLICPGM.
    Restore,
    /// DLTLICPGM.
    Delete,
    /// SAVLICPGM.
    Save,
    /// CHKPRDOPT.
    Check,
}

/// Calls exit programs.
pub trait ExitRunner {
    /// Call one; `Err` carries what went wrong in the integrator's words.
    fn call(&mut self, pgm: &QualName, op: ExitOp, key: &LoadKey) -> Result<(), String>;
}

/// An exit runner that calls nothing.
pub struct NoExits;

impl ExitRunner for NoExits {
    fn call(&mut self, _: &QualName, _: ExitOp, _: &LoadKey) -> Result<(), String> {
        Ok(())
    }
}

/// `RSTOBJ` / `OBJTYPE`: which loads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Objects {
    /// `*ALL`: code and language.
    All,
    /// `*PGM`: code only.
    Pgm,
    /// `*LNG`: language only.
    Lng,
}

/// A plan that cannot be made or carried out.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum PlanError {
    /// The media has no load for it.
    #[error("{product} option {option} {what} is not on the media")]
    NotOnMedia {
        /// Product.
        product: ProductId,
        /// Option.
        option: OptionNo,
        /// `*CODE` or the language.
        what: String,
    },
    /// The media has several releases; RLS must say which.
    #[error("the media has more than one release of {0}; name the release")]
    SeveralReleases(ProductId),
    /// `*BASE` is not installed and not in the plan.
    #[error("{0} *BASE must be installed before its options")]
    BaseMissing(ProductId),
    /// The code load is not installed for a language-only restore.
    #[error("{0} option {1} must have its code installed before its language objects")]
    CodeMissing(ProductId, OptionNo),
    /// A prerequisite is missing.
    #[error("{for_product} option {for_option} needs {} option {}", .needs.product, .needs.option)]
    Prereq {
        /// The product needing it.
        for_product: ProductId,
        /// Its option.
        for_option: OptionNo,
        /// What it needs.
        needs: Prereq,
    },
    /// The prerequisites go round in a circle.
    #[error("the prerequisites of the products in the plan form a cycle")]
    Cycle,
    /// Nothing installed matches.
    #[error("{0} is not installed")]
    NotInstalled(ProductId),
    /// Installed options of other products need what would go.
    #[error("other installed options need what would be deleted: {0:?}")]
    Dependents(Vec<(ProductId, OptionNo)>),
    /// A load is not in a state SAVLICPGM can save.
    #[error("{0} is in state {1:?} and cannot be saved")]
    NotSavable(LoadKey, LoadState),
    /// An agreement was not accepted.
    #[error(transparent)]
    Agreement(#[from] AgreementError),
    /// Storage failed.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// The media failed.
    #[error(transparent)]
    Media(#[from] MediaError),
    /// The restore failed for a load.
    #[error("restoring {0} failed: {1}")]
    Restore(LoadKey, String),
    /// An exit program failed.
    #[error("exit program {0} failed for {1}: {2}")]
    Exit(QualName, LoadKey, String),
    /// A language that does not exist.
    #[error("{0} is not a language feature")]
    Language(LoadId),
}
