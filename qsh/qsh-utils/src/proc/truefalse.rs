// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `true` and `false` as programs.

use qsh::sys::UtilCtx;

/// Run `true`.
pub fn run_true(_cx: &mut UtilCtx<'_>) -> i32 {
    0
}

/// Run `false`.
pub fn run_false(_cx: &mut UtilCtx<'_>) -> i32 {
    1
}
