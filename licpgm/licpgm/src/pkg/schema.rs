// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The QSPKGPRD manifest's TOML shape (`docs/LICPGM.md` "Manifests").

use serde::Deserialize;

/// A whole manifest: one product.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// The product.
    pub product: Product,
    /// Its options, `*BASE` first.
    #[serde(default)]
    pub option: Vec<Opt>,
    /// License terms per feature.
    #[serde(default)]
    pub license: Vec<License>,
    /// The software agreement.
    pub agreement: Option<Agreement>,
}

/// The shipped catalog: several manifests.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    /// One per product.
    pub package: Vec<Manifest>,
}

/// A number or a special value written as a string.
#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum NumOrText {
    /// `31`.
    Num(i64),
    /// `"*BASE"`, `"*NOMAX"`, `"31"`.
    Text(String),
}

impl NumOrText {
    /// As text.
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            NumOrText::Num(n) => n.to_string(),
            NumOrText::Text(s) => s.clone(),
        }
    }
}

/// `[product]`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Product {
    /// Seven-character product ID.
    pub id: String,
    /// `VxRyMz`; the system's release when absent.
    pub release: Option<String>,
    /// The IBM product ID it also answers to.
    pub ibm_alias: Option<String>,
    /// Description, up to 50 characters.
    pub text: String,
    /// The `*PRDDFN` object name; the product ID when absent.
    pub object: Option<String>,
    /// The principal library of `*BASE`, where the `*PRDDFN` goes.
    pub library: String,
    /// `LIB/NAME` of the option descriptions.
    pub msgf: Option<String>,
    /// First copyright year.
    pub first_copyright: Option<u16>,
    /// Current copyright year.
    pub current_copyright: Option<u16>,
    /// `YYYY-MM-DD`.
    pub release_date: Option<String>,
    /// Whether two releases may be installed side by side.
    #[serde(default)]
    pub allow_multiple_releases: bool,
    /// Whether options may be at another release than `*BASE`.
    #[serde(default)]
    pub allow_mixed_releases: bool,
    /// `*PHONE`, `*CUSTOMER` or `*OPSYS` (default).
    pub registration_type: Option<String>,
    /// Registration value.
    #[serde(default)]
    pub registration_value: String,
    /// CCSIDs its text may be in.
    #[serde(default)]
    pub ccsids: Vec<u32>,
    /// Languages of `*BASE`; `["2924"]` when absent.
    pub languages: Option<Vec<String>>,
    /// The vendor's license public key, 64 hex digits.
    pub license_key: Option<String>,
    /// Public authority of the objects.
    pub public_aut: Option<String>,
    /// Catalog only: `"shipped"` or `"reserved"`.
    pub status: Option<String>,
}

/// `[[option]]`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Opt {
    /// `"*BASE"` or 1..99.
    pub option: NumOrText,
    /// Description, up to 50 characters.
    pub text: String,
    /// The message ID describing it.
    #[serde(default)]
    pub msgid: String,
    /// Code load ID (license feature); 5001 when absent.
    pub feature: Option<String>,
    /// Whether libraries may be installed under other names.
    #[serde(default)]
    pub dynamic_naming: bool,
    /// The lowest `*BASE` release it works with.
    pub min_base: Option<String>,
    /// Prerequisites, `PRODUCT:OPTION[:VxRyMz]`.
    #[serde(default)]
    pub requires: Vec<String>,
    /// Languages; `*BASE`'s when absent.
    pub languages: Option<Vec<String>>,
    /// Libraries; the product library when absent.
    #[serde(default)]
    pub libraries: Vec<Lib>,
    /// Preoperation exit programs, `LIB/PGM`.
    #[serde(default)]
    pub pre_exits: Vec<String>,
    /// Objects of the code load.
    #[serde(default)]
    pub objects: Vec<Obj>,
    /// Objects of the language loads.
    #[serde(default)]
    pub lng_objects: Vec<Obj>,
    /// The secondary language library.
    pub lng_library: Option<String>,
    /// IFS directories.
    #[serde(default)]
    pub directories: Vec<Dir>,
    /// IFS paths owned below the directories.
    #[serde(default)]
    pub paths: Vec<String>,
    /// Catalog note: the QS/400 crates and services the option holds.
    #[serde(default)]
    pub components: Vec<String>,
    /// Catalog note: IBM's analogue.
    pub ibm: Option<String>,
}

/// A library entry.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lib {
    /// The development library.
    pub name: String,
    /// The primary library; the development name when absent.
    pub primary: Option<String>,
    /// The postoperation exit program in it.
    pub post_exit: Option<String>,
    /// `*PROD` or `*TEST`.
    #[serde(rename = "type")]
    pub lib_type: Option<String>,
    /// Public authority.
    pub aut: Option<String>,
    /// Create authority.
    pub crt_aut: Option<String>,
}

/// An object entry.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Obj {
    /// Its library; the option's principal library when absent.
    pub lib: Option<String>,
    /// The name, or `*ALL`.
    pub name: String,
    /// The type, or `*ALL`.
    #[serde(rename = "type")]
    pub typ: String,
}

/// A directory entry.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dir {
    /// Full path.
    pub path: String,
    /// The product home directory name within it; the last name when
    /// absent.
    pub home: Option<String>,
    /// Public data authority, `*RX` when absent.
    pub data_aut: Option<String>,
    /// Public object authorities.
    #[serde(default)]
    pub obj_auts: Vec<String>,
}

/// `[[license]]`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct License {
    /// The feature.
    pub feature: String,
    /// The term; the release's version when absent.
    pub term: Option<String>,
    /// `*CONCURRENT`, `*REGISTERED`, `*PROCESSOR`.
    pub usage_type: Option<String>,
    /// `*OPRACTION`, `*WARNING`, `*KEYED`.
    pub compliance: Option<String>,
    /// A number or `*NOMAX`.
    pub usage_limit: Option<NumOrText>,
    /// Grace period in days.
    #[serde(default)]
    pub grace_days: u16,
    /// Threshold use count.
    pub threshold: Option<u32>,
}

/// `[agreement]`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Agreement {
    /// The agreement text itself.
    pub text: Option<String>,
    /// Or the IFS path of a document holding it.
    pub document: Option<String>,
}
