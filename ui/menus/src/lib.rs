// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Menus for QS/400: the `*MENU` object's payload ([`payload`]), the
//! state machine behind `GO` and the menu keys ([`stack`]) and the
//! generator of the command-group menus as UIM panel groups
//! ([`generate`]). The crate never touches the store and never writes
//! a screen; `docs/MENUS.md` is the reference.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod generate;
pub mod payload;
pub mod shipped;
pub mod spec;
pub mod stack;

pub use payload::{CmdLin, CurLib, MenuAttrs, MenuKind, PrdLib};
pub use stack::{MenuStack, Outcome, Target};

/// A qualified object name: `lib/name`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Qualified {
    /// The object.
    pub name: String,
    /// Its library, or `*LIBL`, `*CURLIB`.
    pub lib: String,
}

impl Qualified {
    /// A qualified name, both parts folded to upper case.
    #[must_use]
    pub fn new(lib: &str, name: &str) -> Self {
        Self {
            name: name.trim().to_ascii_uppercase(),
            lib: lib.trim().to_ascii_uppercase(),
        }
    }

    /// `QSYS/NAME`.
    #[must_use]
    pub fn qsys(name: &str) -> Self {
        Self::new("QSYS", name)
    }
}

impl std::fmt::Display for Qualified {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.lib, self.name)
    }
}

/// What can go wrong.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A payload that is not a menu or is cut short.
    #[error("{0}")]
    Corrupt(String),
    /// Attributes that do not go together.
    #[error("{id}: {text}")]
    Value {
        /// The message identifier.
        id: &'static str,
        /// What is wrong.
        text: String,
    },
    /// A generator input the code cannot read.
    #[error("{0}")]
    Input(String),
    /// A file the generator could not read or write.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Error {
    pub(crate) fn value(id: &'static str, text: impl Into<String>) -> Self {
        Self::Value {
            id,
            text: text.into(),
        }
    }
}

/// The crate's result.
pub type Result<T> = std::result::Result<T, Error>;
