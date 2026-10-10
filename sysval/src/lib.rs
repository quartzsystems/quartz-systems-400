// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! System values: the IBM i names as the compatibility interface, Quartz
//! defaults and text, validation, time zones with coded DST rules, and
//! storage of changed values in the LIC partition.
//!
//! Storage is the `*SYSVAL` object; the LIC record keeps the IPL subset.

#![forbid(unsafe_code)]

mod catalog;
mod clock;
mod error;
mod store;
pub mod tz;
mod value;

pub use catalog::{Catalog, Entry, Group, Kind};
pub use clock::{LocalTime, civil_from_days, civil_to_unix, days_from_civil};
pub use error::SysValError;
pub use store::{Effect, HostInfo, IPL_NAMES, SysVals, TAG_IPL_SYSVALS, TAG_SYSVALS};
pub use value::Value;

/// The catalog source, compiled in.
pub const CATALOG_TOML: &str = include_str!("../catalog.toml");
