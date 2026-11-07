// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Software: licensed programs (`GO LICPGM`), license keys, PTFs
//! (`DSPPTF`), PTF groups and the update service's fixes. Every change
//! is a CL command run as the user through [`Software::run`].

use qsweb::msg::Message;

/// A product option the product directory or an image catalog knows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Product {
    /// `QS00SS1`.
    pub id: String,
    /// `*BASE` or the option number.
    pub option: String,
    /// `V1R0M0`.
    pub release: String,
    /// The description.
    pub text: String,
    /// Installed (in any state).
    pub installed: bool,
    /// The installed status as `GO LICPGM` 10 shows it (`Compatible`,
    /// `Installed`, `Error`), blank when not installed.
    pub status: String,
    /// The usage limit of the product's license (`Unlimited`), blank for
    /// an option without a key of its own.
    pub license: String,
    /// The product library.
    pub library: String,
    /// When it was installed, seconds since 1970; 0 for not installed.
    pub installed_at: u64,
}

/// An image catalog the products or PTFs can come from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImageCatalog {
    /// `QS400V1R0`.
    pub name: String,
    /// The virtual optical device it is loaded in (`OPTVRT01`).
    pub device: String,
    /// Its text.
    pub text: String,
}

/// A license key (`DSPLICKEY`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LicenseKey {
    /// `QS00SS1`.
    pub product: String,
    /// `V1`.
    pub term: String,
    /// `5050`.
    pub feature: String,
    /// `*NOMAX` or a number of users.
    pub usage_limit: String,
    /// `*NONE` or `MM/DD/YY`.
    pub expires: String,
    /// The processor group (`*ANY`).
    pub processor_group: String,
}

/// A PTF's status.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PtfStatus {
    /// Loaded, not applied.
    #[default]
    NotApplied,
    /// Temporarily applied.
    Temporary,
    /// Permanently applied.
    Permanent,
    /// Superseded by a later PTF.
    Superseded,
}

impl PtfStatus {
    /// Every status, as the filter offers them.
    pub const ALL: [PtfStatus; 4] = [
        PtfStatus::NotApplied,
        PtfStatus::Temporary,
        PtfStatus::Permanent,
        PtfStatus::Superseded,
    ];

    /// As shown.
    pub fn label(self) -> &'static str {
        match self {
            PtfStatus::NotApplied => "Not applied",
            PtfStatus::Temporary => "Temporarily applied",
            PtfStatus::Permanent => "Permanently applied",
            PtfStatus::Superseded => "Superseded",
        }
    }
}

/// A PTF on the system (`DSPPTF`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ptf {
    /// `QS10432`.
    pub id: String,
    /// The product.
    pub product: String,
    /// `V1R0M0`.
    pub release: String,
    /// Its status.
    pub status: PtfStatus,
    /// `None`, `Yes - delayed`, `Yes - immediate`.
    pub ipl_action: String,
    /// The description.
    pub text: String,
    /// When it was loaded, seconds since 1970.
    pub loaded: u64,
}

impl Ptf {
    /// Whether it takes effect at an IPL.
    pub fn delayed(&self) -> bool {
        self.ipl_action.contains("delayed")
    }
}

/// A PTF group (`WRKPTFGRP`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PtfGroup {
    /// `QS400-CUM-26290`, `QS99100`.
    pub name: String,
    /// Its level.
    pub level: u32,
    /// `Installed`, `Not installed`, ...
    pub status: String,
    /// Its text.
    pub text: String,
}

/// A fix the update service offers (the last `QSCHKUPD`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fix {
    /// `QS10491`, or a cumulative package's name.
    pub id: String,
    /// `PTF` or `Cumulative package`.
    pub kind: String,
    /// The product.
    pub product: String,
    /// Released, seconds since 1970.
    pub released: u64,
    /// Bytes.
    pub size: u64,
    /// The description.
    pub text: String,
    /// What this system has of it (`Applied`), blank for nothing.
    pub on_system: String,
}

/// The update service's attributes (`QSCHGUPDA`, data area
/// `QSYS/QSUPDATTR`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UpdateAttrs {
    /// The service URL.
    pub url: String,
    /// `host:port`, blank for none.
    pub proxy: String,
    /// `*NONE`, `*DAILY` or `*WEEKLY`.
    pub schedule: String,
    /// `HHMMSS` of the scheduled check.
    pub time: String,
    /// The last check, seconds since 1970; 0 for never.
    pub last: u64,
}

/// What the software pages need, as the user.
pub trait Software: Send + Sync {
    /// The product directory: every product option the system knows.
    fn products(&self, user: &str) -> Vec<Product>;
    /// The image catalogs.
    fn catalogs(&self, user: &str) -> Vec<ImageCatalog>;
    /// The product options on an image catalog.
    fn catalog_products(&self, user: &str, catalog: &str) -> Result<Vec<Product>, Message>;
    /// The license keys (`DSPLICKEY`).
    fn license_keys(&self, user: &str) -> Vec<LicenseKey>;
    /// The PTFs (`DSPPTF`).
    fn ptfs(&self, user: &str) -> Vec<Ptf>;
    /// The PTF groups, the cumulative package first.
    fn ptf_groups(&self, user: &str) -> Vec<PtfGroup>;
    /// A PTF's or fix's cover letter (`DSPPTFCVR`).
    fn cover_letter(&self, user: &str, id: &str) -> Option<String>;
    /// The fixes the last check found.
    fn fixes(&self, user: &str) -> Vec<Fix>;
    /// The update service's attributes.
    fn update_attrs(&self, user: &str) -> UpdateAttrs;
    /// Run a CL command as the user: its completion or escape message.
    fn run(&self, user: &str, command: &str) -> Result<Message, Message>;
}
