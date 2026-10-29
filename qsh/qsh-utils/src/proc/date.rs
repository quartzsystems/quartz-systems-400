// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `date [-u] [+format]`: the time in the `TZ` zone (UTC without one),
//! formatted as `strftime` does. Setting the date is refused.

use qsh::sys::UtilCtx;

use crate::common::args::getopt;
use crate::common::io::Out;
use crate::common::time::{Zone, strftime};
use crate::common::{complain, usage};

/// Run `date`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "date";
    let p = match getopt(&cx.argv[1..], "u") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    let mut format = "%a %b %e %H:%M:%S %Z %Y".to_string();
    match p.operands.as_slice() {
        [] => {}
        [f] if f.starts_with('+') => format = f[1..].to_string(),
        [_] => {
            complain(cx, name, "Setting the date is not permitted.");
            return 1;
        }
        _ => return usage(cx, name, "Too many operands."),
    }
    let zone = if p.has('u') { Zone::utc() } else { Zone::of(cx) };
    let now = i64::try_from(cx.sys.now().as_secs()).unwrap_or(0);
    let tm = zone.local(now);
    let mut out = Out::stdout(cx);
    out.text(&strftime(&format, &tm));
    out.text("\n");
    out.finish(cx, name, 0)
}
