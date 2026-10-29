// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `sleep seconds`.

use std::time::Duration;

use qsh::sys::UtilCtx;

use crate::common::usage;

/// Run `sleep`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "sleep";
    let [secs] = &cx.argv[1..] else {
        return usage(cx, name, "A number of seconds is needed.");
    };
    let Some(n) = crate::common::number(secs) else {
        return usage(cx, name, &format!("{secs} is not a valid number."));
    };
    cx.sys.sleep(Duration::from_secs(n));
    0
}
