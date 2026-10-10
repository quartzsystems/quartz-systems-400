// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The object layer: IDs, headers, the type registry, the object
//! directory, libraries with their directories, locks, and the generic
//! operations every type gets. See `docs/STORE_FORMAT.md` §8 and §9.

pub mod dir;
pub mod header;
pub mod id;
pub mod library;
pub mod lock;
pub mod ops;
pub mod types;

pub use dir::DirKey;
pub use header::{Authority, ObjectHeader};
pub use id::ObjectId;
pub use library::{LibAttr, LibType};
pub use lock::{JobHandle, LockState, LockTable};
pub use ops::{CreateParams, Found, NameContext};
pub use types::{Module, TypeInfo, type_by_code, type_by_name};
