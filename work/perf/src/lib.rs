// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Collection Services (`docs/PERF.md`): the collector's categories and
//! profiles, `CFGPFRCOL`'s configuration, one interval's data, the
//! `*MGTCOL` payload, the `QAPM*` model files and the records
//! `CRTPFRDTA` writes, and `QPFRADJ`'s adjustment. The storage service
//! samples the machine and keeps the collections (`lic/slic/src/stgsvc/
//! perfcol.rs`); this crate is the pure part.

pub mod adjust;
pub mod config;
pub mod mgtcol;
pub mod profile;
pub mod qapm;
pub mod sample;
