// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `iconv -f from -t to [-c] [file...]`, `iconv -l`: convert between
//! CCSIDs with the system's tables. Names are `IBMCCSID00037`,
//! `IBM-037`, `CP037`, `UTF-8`, `ISO8859-1` or a number.

use qsh::sys::UtilCtx;

use crate::common::args::getopt;
use crate::common::io::{Out, inputs, read_input};
use crate::common::{complain, complain_sys, usage};

/// The CCSID a name stands for.
pub fn ccsid_of(name: &str) -> Option<u16> {
    let up = name.to_ascii_uppercase();
    let fixed = match up.as_str() {
        "UTF-8" | "UTF8" => Some(1208),
        "UTF-16" | "UTF16" | "UTF-16BE" => Some(1200),
        "UCS-2" | "UCS2" => Some(13488),
        "ISO8859-1" | "ISO-8859-1" | "ISO_8859-1" | "LATIN1" | "L1" => Some(819),
        "ASCII" | "US-ASCII" | "ANSI_X3.4-1968" | "646" => Some(367),
        "WINDOWS-1252" | "CP1252" | "MS-ANSI" => Some(1252),
        "EBCDIC-US" | "EBCDICUS" => Some(37),
        "BINARY" => Some(65535),
        _ => None,
    };
    if fixed.is_some() {
        return fixed;
    }
    let digits = up
        .trim_start_matches("IBMCCSID")
        .trim_start_matches("IBM-")
        .trim_start_matches("IBM")
        .trim_start_matches("CP");
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// Run `iconv`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "iconv";
    let p = match getopt(&cx.argv[1..], "f:t:cls") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    let mut out = Out::stdout(cx);
    if p.has('l') {
        for c in ccsid::conv::supported() {
            out.text(&format!("IBMCCSID{c:05}\n"));
        }
        return out.finish(cx, name, 0);
    }
    let (Some(from), Some(to)) = (p.value('f'), p.value('t')) else {
        return usage(cx, name, "Both -f and -t are needed.");
    };
    let (Some(fc), Some(tc)) = (ccsid_of(from), ccsid_of(to)) else {
        complain(cx, name, &format!("A conversion from {from} to {to} is not available."));
        return 1;
    };
    let Ok(mut conv) = ccsid::conv::Converter::new(fc, tc) else {
        complain(cx, name, &format!("A conversion from {from} to {to} is not available."));
        return 1;
    };
    let mut status = 0;
    for f in inputs(&p.operands) {
        let data = match read_input(cx, &f) {
            Ok(d) => d,
            Err(e) => {
                complain_sys(cx, name, &f, e);
                status = 1;
                continue;
            }
        };
        let mut o = Vec::with_capacity(data.len());
        conv.feed(&data, &mut o);
        conv.finish(&mut o);
        if !out.write(&o) {
            break;
        }
    }
    out.finish(cx, name, status)
}
