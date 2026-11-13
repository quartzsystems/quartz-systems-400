// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The types of NFS versions 4.0 and 4.1 and the callback program (RFC 5662, RFC 7531), generated from `x/nfs4.x`.

// The names are the description's, as the RFCs write them.
#![allow(
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    clippy::upper_case_acronyms,
    clippy::large_enum_variant,
    clippy::too_many_lines,
    clippy::unreadable_literal,
    clippy::match_same_arms,
    clippy::struct_field_names,
    clippy::struct_excessive_bools,
    clippy::enum_variant_names,
    clippy::match_bool,
    clippy::match_wildcard_for_single_variants
)]

include!(concat!(env!("OUT_DIR"), "/nfs4.rs"));
