// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Authority: who asks ([`principal`]), authority sets ([`bits`]),
//! changing an object's authority ([`grant`]), authorization lists
//! ([`autl`]), what operations need ([`required`]), the search
//! ([`search`]) and a job's cache of its answers ([`cache`]).

pub mod autl;
pub mod bits;
pub mod cache;
pub mod grant;
pub mod principal;
pub mod required;
pub mod search;
