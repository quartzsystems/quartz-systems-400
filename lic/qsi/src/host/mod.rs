// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The families a virtual server host answers beside the base and
//! pairing ones (`docs/QSI_FAMILIES.md`): the host itself, its
//! partitions read and changed, their profiles, control panels,
//! reference codes, dumps and consoles, and its virtual resources.
//! Each is version 1 with its capability bit ([`crate::caps`]).

pub mod console;
pub mod dumps;
#[allow(clippy::module_inception)] // the host family, in the host's modules
pub mod host;
pub mod model;
pub mod panel;
pub mod partition;
pub mod partitions;
pub mod profiles;
pub mod refcodes;
pub mod vres;
