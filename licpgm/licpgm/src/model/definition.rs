// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The product definition (`*PRDDFN`): one product at one release, its
//! options, language loads, CCSIDs and license terms.

use release::Release;

use crate::id::{LicTerm, LoadId, OptionNo, ProductId};
use crate::model::{Date, QualName};

/// How a product's registration ID is formed (QSZRTVPR's type).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegType {
    /// `02`, `*PHONE`: a telephone number.
    Phone,
    /// `04`: the operating system's registration value.
    OsSame,
    /// `08`, `*CUSTOMER`: country code and customer number.
    Customer,
}

impl RegType {
    /// QSZRTVPR's two-character code.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            RegType::Phone => "02",
            RegType::OsSame => "04",
            RegType::Customer => "08",
        }
    }

    /// The type with a code.
    #[must_use]
    pub fn from_code(c: &str) -> Option<Self> {
        match c {
            "02" => Some(RegType::Phone),
            "04" => Some(RegType::OsSame),
            "08" => Some(RegType::Customer),
            _ => None,
        }
    }

    /// The type a QSZCRTPD/QSZCRTPL name selects.
    #[must_use]
    pub fn from_name(n: &str) -> Option<Self> {
        match n {
            "*PHONE" => Some(RegType::Phone),
            "*CUSTOMER" => Some(RegType::Customer),
            "*OPSYS" | "*SAME" => Some(RegType::OsSame),
            _ => None,
        }
    }
}

/// A registration ID: type and value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Registration {
    /// How the value is formed.
    pub kind: RegType,
    /// Up to 14 of `A-Z 0-9`.
    pub value: String,
}

impl Registration {
    /// The operating system's registration, shared.
    #[must_use]
    pub fn os() -> Self {
        Self {
            kind: RegType::OsSame,
            value: String::new(),
        }
    }
}

/// Another product option an option needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prereq {
    /// The product.
    pub product: ProductId,
    /// Its option.
    pub option: OptionNo,
    /// The lowest release that will do; `None` is any.
    pub min_release: Option<Release>,
}

/// One option of a product.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionDef {
    /// `*BASE` or 1..99.
    pub option: OptionNo,
    /// Our description of the option (50 characters).
    pub text: String,
    /// The message describing the option in the product's message file.
    pub msg_id: String,
    /// Whether RSTLICPGM may install its libraries and folders under
    /// other names.
    pub dynamic_naming: bool,
    /// The code load ID: the license feature the option is in.
    pub feature: LoadId,
    /// The lowest release of `*BASE` the option works with; `None` is
    /// the product's own release.
    pub min_base: Option<Release>,
    /// Other options it needs installed.
    pub requires: Vec<Prereq>,
}

/// Which national languages an option has language loads for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LngSel {
    /// One language feature.
    Nlv(LoadId),
    /// `*NONE`: no language load.
    None,
    /// `*IBMLNG`: every language the operating system has (base only).
    IbmLng,
    /// `*BASEOPT`: whatever `*BASE` has (other options only).
    BaseOpt,
}

impl LngSel {
    /// The QSZCRTPD text.
    #[must_use]
    pub fn text(&self) -> &str {
        match self {
            LngSel::Nlv(n) => n.as_str(),
            LngSel::None => "*NONE",
            LngSel::IbmLng => "*IBMLNG",
            LngSel::BaseOpt => "*BASEOPT",
        }
    }

    /// Parse the QSZCRTPD text.
    pub fn parse(s: &str) -> Result<Self, crate::id::IdError> {
        Ok(match s {
            "*NONE" => LngSel::None,
            "*IBMLNG" => LngSel::IbmLng,
            "*BASEOPT" => LngSel::BaseOpt,
            n => LngSel::Nlv(LoadId::nlv(n)?),
        })
    }
}

/// One entry of the language load list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LngLoad {
    /// The option.
    pub option: OptionNo,
    /// The language.
    pub load: LngSel,
}

/// How license use is counted (WRKLICINF's usage type).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageType {
    /// `*CONCURRENT`: uses held at once, by job.
    Concurrent,
    /// `*REGISTERED`: uses held by named users.
    Registered,
    /// `*PROCESSOR`: uses held by processors.
    Processor,
}

impl UsageType {
    /// IBM's name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            UsageType::Concurrent => "*CONCURRENT",
            UsageType::Registered => "*REGISTERED",
            UsageType::Processor => "*PROCESSOR",
        }
    }

    /// Parse IBM's name.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "*CONCURRENT" => Some(UsageType::Concurrent),
            "*REGISTERED" => Some(UsageType::Registered),
            "*PROCESSOR" => Some(UsageType::Processor),
            _ => None,
        }
    }
}

/// What happens when use goes past the usage limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compliance {
    /// `*OPRACTION`: the request is refused until the operator raises
    /// the limit.
    OprAction,
    /// `*WARNING`: the request is granted and a warning sent.
    Warning,
    /// `*KEYED`: the limit comes from a license key; past it, or with
    /// no key, use continues only during the grace period.
    Keyed,
}

impl Compliance {
    /// IBM's name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Compliance::OprAction => "*OPRACTION",
            Compliance::Warning => "*WARNING",
            Compliance::Keyed => "*KEYED",
        }
    }

    /// Parse IBM's name.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "*OPRACTION" => Some(Compliance::OprAction),
            "*WARNING" => Some(Compliance::Warning),
            "*KEYED" => Some(Compliance::Keyed),
            _ => None,
        }
    }
}

/// The license terms of one feature (what IBM's QLZADDLI adds).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LicenseDef {
    /// The feature (code load ID).
    pub feature: LoadId,
    /// The license term.
    pub term: LicTerm,
    /// How use is counted.
    pub usage_type: UsageType,
    /// What happens past the limit.
    pub compliance: Compliance,
    /// The shipped usage limit; `None` is `*NOMAX`.
    pub usage_limit: Option<u32>,
    /// Days of use allowed past the limit or without a key.
    pub grace_days: u16,
    /// Use count that triggers a notice; `None` is no threshold.
    pub threshold: Option<u32>,
}

/// A product definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProductDefinition {
    /// The product.
    pub product: ProductId,
    /// Its release.
    pub release: Release,
    /// The IBM product ID it answers to as well (`5770SS1`), so
    /// software checking for IBM's product finds it.
    pub ibm_alias: Option<ProductId>,
    /// Our description (50 characters).
    pub text: String,
    /// The message file of its option descriptions.
    pub msgf: Option<QualName>,
    /// First copyright year.
    pub first_copyright: Option<u16>,
    /// Current copyright year.
    pub current_copyright: Option<u16>,
    /// The release date.
    pub release_date: Option<Date>,
    /// Whether two releases may be installed side by side.
    pub allow_multiple_releases: bool,
    /// Whether options may be at a release other than `*BASE`'s.
    pub allow_mixed_releases: bool,
    /// The registration ID.
    pub registration: Registration,
    /// Public authority of the object.
    pub public_aut: String,
    /// Options, `*BASE` first.
    pub options: Vec<OptionDef>,
    /// The language load list.
    pub languages: Vec<LngLoad>,
    /// CCSIDs the product's text may be in; empty is any.
    pub ccsids: Vec<u32>,
    /// Whether use is counted against license terms at all.
    pub licensed: bool,
    /// The ed25519 public key license keys must be signed with; `None`
    /// is the system's Quartz key.
    pub license_key: Option<[u8; 32]>,
    /// License terms per feature.
    pub licenses: Vec<LicenseDef>,
}

impl ProductDefinition {
    /// A bare definition with a base option, for building on.
    #[must_use]
    pub fn new(product: ProductId, release: Release, text: &str) -> Self {
        Self {
            product,
            release,
            ibm_alias: None,
            text: text.to_string(),
            msgf: None,
            first_copyright: None,
            current_copyright: None,
            release_date: None,
            allow_multiple_releases: false,
            allow_mixed_releases: false,
            registration: Registration::os(),
            public_aut: "*USE".into(),
            options: vec![OptionDef {
                option: OptionNo::BASE,
                text: text.to_string(),
                msg_id: String::new(),
                dynamic_naming: false,
                feature: LoadId::feature(LoadId::CODE_DEFAULT).expect("5001"),
                min_base: None,
                requires: Vec::new(),
            }],
            languages: vec![LngLoad {
                option: OptionNo::BASE,
                load: LngSel::Nlv(LoadId::nlv(LoadId::ENGLISH).expect("2924")),
            }],
            ccsids: Vec::new(),
            licensed: false,
            license_key: None,
            licenses: Vec::new(),
        }
    }

    /// The option with a number.
    #[must_use]
    pub fn option(&self, n: OptionNo) -> Option<&OptionDef> {
        self.options.iter().find(|o| o.option == n)
    }

    /// The language features an option has loads for, `*BASEOPT` and
    /// `*IBMLNG` resolved against the system's languages.
    #[must_use]
    pub fn languages_of(&self, n: OptionNo, system_lngs: &[LoadId]) -> Vec<LoadId> {
        let mut out = Vec::new();
        for l in self.languages.iter().filter(|l| l.option == n) {
            match &l.load {
                LngSel::Nlv(id) => out.push(id.clone()),
                LngSel::None => {}
                LngSel::IbmLng => out.extend(system_lngs.iter().cloned()),
                LngSel::BaseOpt if !n.is_base() => {
                    out.extend(self.languages_of(OptionNo::BASE, system_lngs));
                }
                LngSel::BaseOpt => {}
            }
        }
        out.sort();
        out.dedup();
        out
    }

    /// The license terms of a feature covering a release.
    #[must_use]
    pub fn license_for(&self, feature: &LoadId, rel: Release) -> Option<&LicenseDef> {
        self.licenses
            .iter()
            .find(|l| &l.feature == feature && l.term.covers(rel))
    }

    /// Whether `id` names this product, by its own ID or IBM's.
    #[must_use]
    pub fn answers_to(&self, id: &str) -> bool {
        self.product.as_str() == id || self.ibm_alias.as_ref().is_some_and(|a| a.as_str() == id)
    }
}
