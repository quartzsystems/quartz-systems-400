// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QS/400 security: user profiles ([`profile`]), passwords
//! ([`password`]), and the authority every object operation is checked
//! against. `docs/SECURITY.md` describes the model and the layouts.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod error;
pub mod password;
pub mod profile;
pub mod rec;
pub mod spec;

pub use error::{Result, SecError};
