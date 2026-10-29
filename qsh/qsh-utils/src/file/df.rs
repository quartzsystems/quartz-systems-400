// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `df [-kPt] [file...]`: free space of the file systems holding the
//! operands (or every mounted one), in 512-byte blocks or kilobytes.

use qsh::sys::UtilCtx;

use crate::common::args::getopt;
use crate::common::io::Out;
use crate::common::{complain_sys, usage};

/// Run `df`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "df";
    let p = match getopt(&cx.argv[1..], "kPt") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    let unit: u64 = if p.has('k') { 1024 } else { 512 };
    let operands = if p.operands.is_empty() {
        vec!["/".to_string()]
    } else {
        p.operands.clone()
    };
    let mut out = Out::stdout(cx);
    let mut status = 0;
    out.text(&format!(
        "{:<20} {:>10} {:>10} {:>10} {:>8} {}\n",
        "Filesystem",
        format!("{unit}-blocks"),
        "Used",
        "Available",
        "Capacity",
        "Mounted on"
    ));
    let mut shown: Vec<String> = Vec::new();
    for op in &operands {
        let info = match cx.sys.statfs(&cx.path(op)) {
            Ok(i) => i,
            Err(e) => {
                complain_sys(cx, name, op, e);
                status = 1;
                continue;
            }
        };
        if shown.contains(&info.mounted_on) {
            continue;
        }
        shown.push(info.mounted_on.clone());
        let to_units = |b: u64| b.saturating_mul(info.block_size) / unit;
        let total = to_units(info.blocks);
        let avail = to_units(info.avail);
        let used = to_units(info.blocks.saturating_sub(info.free));
        let denom = used + avail;
        let pct = if denom == 0 {
            0
        } else {
            (used * 100).div_ceil(denom)
        };
        out.text(&format!(
            "{:<20} {:>10} {:>10} {:>10} {:>7}% {}\n",
            info.name, total, used, avail, pct, info.mounted_on
        ));
    }
    out.finish(cx, name, status)
}
