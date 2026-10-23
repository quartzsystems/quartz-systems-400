// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Programming Development Manager's logic: which options each
//! list offers and the command each runs ([`option`]), the `&L &F &N
//! &O &T` substitution ([`subst`]), compiling by source type
//! ([`compile`]), the session defaults ([`defaults`]), the
//! user-defined options file ([`uopt`]), find string ([`find`]) and
//! subsetting and position-to ([`subset`]). No store and no screen:
//! the job's driver (`work/qsjob/src/devtools/pdm.rs`) does those.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod compile;
pub mod defaults;
pub mod find;
pub mod option;
pub mod screen;
pub mod subset;
pub mod subst;
pub mod uopt;

pub use defaults::Defaults;
pub use option::{ListKind, Outcome};
pub use subst::Item;

/// Why an option cannot run on an item.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PdmError {
    /// No such option on this list.
    #[error("Option {0} is not valid on this list.")]
    NoOption(String),
    /// The option does not apply to the item's type.
    #[error("Option {option} is not valid for {what} {ty}.")]
    NotForType {
        /// The option typed.
        option: String,
        /// `object type` or `member type`.
        what: &'static str,
        /// The item's type.
        ty: String,
    },
    /// The compiler for the source type is not on this system.
    #[error("The compiler for member type {0} is not installed.")]
    NotInstalled(String),
    /// A user-defined option code that cannot be used.
    #[error("Option {0} is not a valid user-defined option.")]
    BadCode(String),
    /// A default value outside its set.
    #[error("Value {value} is not valid for {field}.")]
    BadValue {
        /// The field.
        field: &'static str,
        /// What was typed.
        value: String,
    },
}
