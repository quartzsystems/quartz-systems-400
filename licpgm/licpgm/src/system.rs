// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the licensed-program functions need to know about the system.

use release::Release;

use crate::id::LoadId;

/// The system: release, identity, languages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemInfo {
    /// The operating system's release.
    pub release: Release,
    /// The system serial number (`QSRLNBR`).
    pub serial: String,
    /// The processor group license keys are bound to (`P05`, ...).
    pub processor_group: String,
    /// The primary language feature.
    pub primary_lng: LoadId,
    /// Every language installed with the operating system.
    pub languages: Vec<LoadId>,
    /// Releases a target-release parameter may name, ascending.
    pub known_releases: Vec<Release>,
}

impl SystemInfo {
    /// A system at the current release with English as its language.
    #[must_use]
    pub fn current(serial: &str, processor_group: &str) -> Self {
        let en = LoadId::nlv(LoadId::ENGLISH).expect("2924");
        Self {
            release: release::RELEASE,
            serial: serial.to_string(),
            processor_group: processor_group.to_string(),
            primary_lng: en.clone(),
            languages: vec![en],
            known_releases: vec![release::RELEASE],
        }
    }
}
