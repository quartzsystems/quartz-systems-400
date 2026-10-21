// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! User profiles: the `*USRPRF` payload, special authorities and user
//! classes, the profile objects in `QSYS`, their object indexes, and
//! the shipped profiles.

pub mod index;
pub mod objects;
pub mod payload;
pub mod shipped;
pub mod spcaut;

pub use payload::{LmtCpb, PwdForm, PwdHash, Tri, UsrPrf};
pub use spcaut::{SpecialAuth, UserClass};
