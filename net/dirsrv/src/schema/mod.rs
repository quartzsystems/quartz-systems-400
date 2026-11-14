// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The schema model, syntaxes and matching rules (docs/DIRSRV.md §6):
//!
//! - [`syntax`], [`rule`], [`prep`], [`time`]: syntaxes with their
//!   validators, matching rules with their normalizers, RFC 4518 string
//!   preparation, Generalized and UTC Time.
//! - [`registry`], [`model`]: attribute types and object classes by name
//!   and OID, resolved (inherited syntax and rules, superclass closure,
//!   MUST and MAY), with IBM's access classes.
//! - [`check`]: entries checked under `V3` and `V3_lenient`.
//! - [`dn`]: DN normalization (distinguishedNameMatch).
//! - [`shipped`]: QS/400's schema files, embedded.
//! - [`modify`]: changes through `cn=schema` and `V3.modifiedschema`.
//! - [`normalizer`]: the entry store's keys.

pub mod check;
pub mod dn;
pub mod model;
pub mod modify;
pub mod normalizer;
pub mod prep;
pub mod registry;
pub mod rule;
pub mod shipped;
pub mod syntax;
pub mod time;

pub use check::{CheckError, Checked, Level};
pub use model::{AccessClass, AttributeType, ObjectClass};
pub use registry::{Schema, SchemaError};
pub use rule::{Kind, Rule};
