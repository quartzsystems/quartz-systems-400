// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A code's family: its first two characters.

/// A code's family, named by its first two characters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// `C1`: LIC initialization.
    C1,
    /// `C6`: operating system initialization, install and PTFs.
    C6,
    /// `A6`: attention; the system goes on.
    A6,
    /// `B1`: fatal; the IPL stops.
    B1,
    /// `D1`: power down.
    D1,
}

impl Family {
    /// Every family, in table order.
    pub const ALL: [Family; 5] = [Family::C1, Family::C6, Family::A6, Family::B1, Family::D1];

    /// The family of a code, by its first two characters.
    #[must_use]
    pub fn of(code: &str) -> Option<Family> {
        Family::ALL
            .into_iter()
            .find(|f| code.as_bytes().starts_with(f.prefix().as_bytes()))
    }

    /// The two characters that start its codes.
    #[must_use]
    pub const fn prefix(self) -> &'static str {
        match self {
            Family::C1 => "C1",
            Family::C6 => "C6",
            Family::A6 => "A6",
            Family::B1 => "B1",
            Family::D1 => "D1",
        }
    }

    /// What the family's codes report.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Family::C1 => "Licensed Internal Code initialization",
            Family::C6 => "Operating system initialization, install and PTFs",
            Family::A6 => "Attention: something needs a look, the system goes on",
            Family::B1 => "Fatal: the IPL stops",
            Family::D1 => "Power down",
        }
    }
}
