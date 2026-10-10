// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Names of objects, libraries and members: validation, normalization
//! and the fixed 10-byte CP037 form the store keeps.
//!
//! Unquoted names: first character `A`-`Z`, `$`, `#` or `@`; the rest
//! also `0`-`9` and `_`; at most 10; lowercase is folded to uppercase.
//! Quoted names (`"..."`): any CP037 character but `"`, at most 8
//! between the quotes, case kept; the quotes are part of the stored
//! name. Whether a `Q` name is reserved is the type table's business,
//! not this crate's.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod error;
mod name;
mod special;

pub use error::NameError;
pub use name::{NAME_LEN, Name};
pub use special::{LibSpec, MemberSpec, ObjectSpec};
