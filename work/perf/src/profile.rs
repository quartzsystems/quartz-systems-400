// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Collector categories and the profiles that name sets of them.

/// A category of data the collector gathers each interval.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    /// The machine as a whole: CPU, storage, job counts.
    SysLvl,
    /// Each job's machine figures: CPU, I/O, transitions.
    JobMi,
    /// Each job's operating system figures: subsystem, transactions.
    JobOs,
    /// Each disk unit.
    Disk,
    /// Each communications line (Ethernet).
    CmnBase,
    /// Each storage pool.
    Pool,
    /// The system bus. QS/400 has none to measure: accepted, records
    /// nothing.
    SysBus,
}

impl Category {
    /// Every category, in name order.
    pub const ALL: [Self; 7] = [
        Self::CmnBase,
        Self::Disk,
        Self::JobMi,
        Self::JobOs,
        Self::Pool,
        Self::SysBus,
        Self::SysLvl,
    ];

    /// The name commands use: `*SYSLVL`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::SysLvl => "*SYSLVL",
            Self::JobMi => "*JOBMI",
            Self::JobOs => "*JOBOS",
            Self::Disk => "*DISK",
            Self::CmnBase => "*CMNBASE",
            Self::Pool => "*POOL",
            Self::SysBus => "*SYSBUS",
        }
    }

    /// The category called `name`.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|c| c.name().eq_ignore_ascii_case(name.trim()))
    }

    /// The database files `CRTPFRDTA` makes from it.
    #[must_use]
    pub fn files(self) -> &'static [&'static str] {
        match self {
            Self::SysLvl => &["QAPMSYSTEM"],
            Self::JobMi => &["QAPMJOBMI", "QAPMJSUM"],
            Self::JobOs => &["QAPMJOBOS"],
            Self::Disk => &["QAPMDISK"],
            Self::CmnBase => &["QAPMETH"],
            Self::Pool => &["QAPMPOOL"],
            Self::SysBus => &[],
        }
    }

    /// A bit for a set of categories.
    #[must_use]
    pub fn bit(self) -> u32 {
        1 << (self as u32)
    }
}

/// A set of categories as bits.
#[must_use]
pub fn mask(cats: &[Category]) -> u32 {
    cats.iter().fold(0, |m, c| m | c.bit())
}

/// The categories in `mask`, in name order.
#[must_use]
pub fn of_mask(mask: u32) -> Vec<Category> {
    Category::ALL
        .into_iter()
        .filter(|c| mask & c.bit() != 0)
        .collect()
}

/// The profiles `STRPFRCOL COLPRF` and `CFGPFRCOL DFTCOLPRF` name.
pub const PROFILES: [&str; 5] = [
    "*MINIMUM",
    "*STANDARD",
    "*STANDARDP",
    "*ENHCPCPLN",
    "*CUSTOM",
];

/// The categories of `profile`; `custom` for `*CUSTOM`. `None` for a
/// name that is not a profile.
#[must_use]
pub fn categories(profile: &str, custom: &[Category]) -> Option<Vec<Category>> {
    use Category::{CmnBase, Disk, JobMi, JobOs, Pool, SysBus, SysLvl};
    Some(match profile.trim().to_ascii_uppercase().as_str() {
        "*MINIMUM" => vec![Pool, SysBus, SysLvl],
        // The protocol and capacity planning categories of the larger
        // profiles have nothing more to measure here.
        "*STANDARD" | "*STANDARDP" | "*ENHCPCPLN" => {
            vec![CmnBase, Disk, JobMi, JobOs, Pool, SysBus, SysLvl]
        }
        "*CUSTOM" => custom.to_vec(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_profiles() {
        for c in Category::ALL {
            assert_eq!(Category::parse(c.name()), Some(c));
        }
        assert_eq!(Category::parse("*jobmi"), Some(Category::JobMi));
        assert_eq!(categories("*MINIMUM", &[]).unwrap().len(), 3);
        let std = categories("*STANDARDP", &[]).unwrap();
        assert_eq!(of_mask(mask(&std)), std);
        assert!(categories("*NONE", &[]).is_none());
        assert!(Category::SysBus.files().is_empty());
    }
}
