// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The product load (`*PRDLOD`): what one option's code or one language
//! of it is made of, where it goes, and its state.

use release::Release;

use crate::id::{LoadId, LoadType, OptionNo, ProductId};
use crate::model::QualName;
use crate::model::definition::Registration;
use crate::model::state::LoadState;

/// Which load: product, release, option, type, ID.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LoadKey {
    /// The product.
    pub product: ProductId,
    /// Its release.
    pub release: Release,
    /// The option.
    pub option: OptionNo,
    /// `*CODE` or `*LNG`.
    pub load_type: LoadType,
    /// Feature or language.
    pub load_id: LoadId,
}

impl std::fmt::Display for LoadKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {} {} {} {}",
            self.product,
            self.release,
            self.option,
            self.load_type.name(),
            self.load_id
        )
    }
}

/// `*PROD` or `*TEST`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibType {
    /// A production library.
    Prod,
    /// A test library.
    Test,
}

impl LibType {
    /// IBM's name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            LibType::Prod => "*PROD",
            LibType::Test => "*TEST",
        }
    }
}

/// One library of a load: where it is developed, where it installs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibEntry {
    /// The development library (where the objects were packaged from).
    pub dev: String,
    /// The primary library, the name it installs under.
    pub primary: String,
    /// The name it was installed under; the primary until installed.
    pub installed: String,
    /// The postoperation exit program in it, if any.
    pub post_exit: Option<String>,
    /// `*PROD` or `*TEST`.
    pub lib_type: LibType,
    /// Public authority of the library.
    pub aut: String,
    /// Create authority of the library.
    pub crt_aut: String,
}

impl LibEntry {
    /// A production library developed and installed under one name.
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            dev: name.to_string(),
            primary: name.to_string(),
            installed: name.to_string(),
            post_exit: None,
            lib_type: LibType::Prod,
            aut: "*USE".into(),
            crt_aut: "*SYSVAL".into(),
        }
    }
}

/// One folder of a load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderEntry {
    /// The development folder.
    pub dev: String,
    /// The primary folder.
    pub primary: String,
    /// The installed folder.
    pub installed: String,
}

/// One IFS directory of a load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirEntry {
    /// The primary full path.
    pub primary: String,
    /// The product home directory name within it.
    pub home: String,
    /// The installed full path.
    pub installed: String,
    /// The installed home directory name.
    pub installed_home: String,
    /// Public data authority (`*RX`, `*RWX`, `*EXCLUDE`, ...).
    pub data_aut: String,
    /// Public object authorities (`*OBJEXIST`, ...).
    pub obj_auts: Vec<String>,
}

/// One object a load owns.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjEntry {
    /// The library it is in (primary name).
    pub lib: String,
    /// The object.
    pub name: String,
    /// Its type: `*PGM`, `*FILE`, ...
    pub typ: String,
}

impl ObjEntry {
    /// An entry.
    #[must_use]
    pub fn new(lib: &str, name: &str, typ: &str) -> Self {
        Self {
            lib: lib.into(),
            name: name.into(),
            typ: typ.into(),
        }
    }
}

/// A product load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductLoad {
    /// Which load.
    pub key: LoadKey,
    /// The registration ID; `None` is the definition's (`*PRDDFN`).
    pub registration: Option<Registration>,
    /// The lowest release it may be saved for; `None` is current.
    pub min_target: Option<Release>,
    /// The secondary language library (language loads).
    pub secondary_lib: Option<String>,
    /// Libraries, the principal library first.
    pub libraries: Vec<LibEntry>,
    /// Preoperation exit programs, run before RSTLICPGM, DLTLICPGM,
    /// SAVLICPGM and CHKPRDOPT touch the load.
    pub pre_exits: Vec<QualName>,
    /// Folders.
    pub folders: Vec<FolderEntry>,
    /// IFS directories.
    pub directories: Vec<DirEntry>,
    /// Library objects the load owns.
    pub objects: Vec<ObjEntry>,
    /// IFS paths the load owns, below its directories.
    pub paths: Vec<String>,
    /// Software agreement documents (IFS paths) shown before install.
    pub agreements: Vec<String>,
    /// Our description (50 characters).
    pub text: String,
    /// Public authority of the object.
    pub public_aut: String,
    /// Its state.
    pub state: LoadState,
    /// Whether the load is supported (QSZSPTPR).
    pub supported: bool,
    /// When it was installed, seconds since the epoch; 0 if not.
    pub installed_at: u64,
}

impl ProductLoad {
    /// A load in state 20 with a principal library, nothing else.
    #[must_use]
    pub fn new(key: LoadKey, principal: &str, text: &str) -> Self {
        Self {
            key,
            registration: None,
            min_target: None,
            secondary_lib: None,
            libraries: vec![LibEntry::new(principal)],
            pre_exits: Vec::new(),
            folders: Vec::new(),
            directories: Vec::new(),
            objects: Vec::new(),
            paths: Vec::new(),
            agreements: Vec::new(),
            text: text.to_string(),
            public_aut: "*USE".into(),
            state: LoadState::Created,
            supported: true,
            installed_at: 0,
        }
    }

    /// The principal library.
    #[must_use]
    pub fn principal(&self) -> Option<&LibEntry> {
        self.libraries.first()
    }

    /// The default object name: the last three characters of the
    /// product ID, the option, `C` or `L`, the load ID
    /// (`SS100C5050`). `None` when that is not a valid name.
    #[must_use]
    pub fn default_name(key: &LoadKey) -> Option<String> {
        let id = key.product.as_str();
        let n = format!(
            "{}{:02}{}{}",
            &id[4..],
            key.option.0,
            if key.load_type == crate::id::LoadType::Code {
                'C'
            } else {
                'L'
            },
            key.load_id
        );
        crate::id::is_name(&n).then_some(n)
    }
}
