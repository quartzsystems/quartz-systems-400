// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QS/400's fuzzer, in Rust: libFuzzer (what `cargo fuzz` runs) is C++
//! and the tree builds no C. Coverage comes from LLVM's `SanitizerCoverage`
//! counters ([`cov`]), inputs from [`mutate`], the loop from [`engine`];
//! [`targets`] lists every parser that takes outside bytes ([`sql`] the SQL ones). `build fuzz`
//! runs them all; corpora live in `tools/fuzz/corpus/<target>/`.

pub mod cov;
pub mod engine;
pub mod mutate;
pub mod sql;
pub mod targets;
