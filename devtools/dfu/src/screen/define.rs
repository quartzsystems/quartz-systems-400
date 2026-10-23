// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Define General Information: the first display of a DFU program's
//! definition, its choices read back into the program.

use uim::Screen;
use uim::render::Attr;

use super::{blank, bottom_line, keys, leader, out, put, var};
use crate::program::{Format, Program};

const VAL: usize = 44;
const LEGEND: usize = 55;
const YN: &str = "Y=Yes, N=No";

fn yn(b: bool) -> &'static str {
    if b { "Y" } else { "N" }
}

fn entry(s: &mut Screen, row: usize, sub: bool, label: &str, key: &str, len: usize, value: &str) {
    let col = if sub { 6 } else { 4 };
    out(s, row, col, &leader(label, col, 40));
    var(s, row, VAL, len, key, value);
}

/// The display for a program over an indexed (keyed) file or not.
#[must_use]
pub fn general(p: &Program, indexed: bool, message: Option<&str>) -> Screen {
    let mut s = blank();
    let title = if indexed {
        "Define General Information/Indexed File"
    } else {
        "Define General Information/Nonindexed File"
    };
    put(&mut s, 1, 17, Attr::HighIntensity, title);
    put(&mut s, 3, 2, Attr::Blue, "Type choices, press Enter.");
    entry(&mut s, 5, false, "Job title", "TITLE", 39, &p.title);
    entry(&mut s, 6, false, "Display format", "DSPFMT", 1, p.format.code());
    out(&mut s, 6, LEGEND, "1=Single,  2=Multiple");
    out(&mut s, 7, LEGEND, "3=Maximum, 4=Row oriented");
    let flags = [
        (9, "Audit report", "AUDIT", p.audit),
        (10, "S/36 style", "S36", p.s36),
        (11, "Suppress errors", "SUPPRESS", p.suppress),
        (12, "Edit numerics", "EDIT", p.edit),
        (13, "Allow updates on roll", "ROLLUPD", p.roll_update),
    ];
    for (row, label, key, v) in flags {
        entry(&mut s, row, false, label, key, 1, yn(v));
        out(&mut s, row, LEGEND, YN);
    }
    if indexed {
        entry(&mut s, 14, false, "Generate keys", "GENKEYS", 1, yn(p.gen_keys));
        out(&mut s, 14, LEGEND, YN);
    } else {
        out(&mut s, 14, 4, "Record numbers:");
        entry(&mut s, 15, true, "Generate", "RECNBR", 1, yn(p.recnbr));
        out(&mut s, 15, LEGEND, YN);
        entry(&mut s, 16, true, "Store in a field", "RECNBRFLD", 1, yn(p.recnbr_field));
        out(&mut s, 16, LEGEND, YN);
        entry(&mut s, 17, true, "Heading", "RECNBRHDG", 30, &p.recnbr_heading);
        entry(&mut s, 20, false, "Processing", "PROCESS", 1, if p.sequential { "2" } else { "1" });
        out(&mut s, 20, LEGEND, "1=Direct");
        out(&mut s, 21, LEGEND, "2=Sequential");
    }
    keys(
        &mut s,
        &[(23, 2, "F3=Exit"), (23, 15, "F12=Cancel"), (23, 33, "F14=Display definition")],
    );
    bottom_line(&mut s, message, false);
    s.cursor = Some((5, VAL));
    s
}

/// The display's values into the program; the message for a value
/// that is not allowed.
///
/// # Errors
/// The field's name and why.
pub fn apply(p: &mut Program, value: &dyn Fn(&str) -> String) -> Result<(), String> {
    let flag = |key: &str, label: &str| -> Result<Option<bool>, String> {
        match value(key).trim() {
            "" => Ok(None),
            "Y" | "y" => Ok(Some(true)),
            "N" | "n" => Ok(Some(false)),
            _ => Err(format!("Type Y or N for {label}.")),
        }
    };
    let t = value("TITLE");
    if !t.trim().is_empty() {
        p.title = t.trim().to_string();
    }
    match value("DSPFMT").trim() {
        "" => {}
        c => p.format = Format::from_code(c).ok_or("Type 1, 2, 3 or 4 for the display format.")?,
    }
    for (key, label, slot) in [
        ("AUDIT", "audit report", &mut p.audit),
        ("S36", "S/36 style", &mut p.s36),
        ("SUPPRESS", "suppress errors", &mut p.suppress),
        ("EDIT", "edit numerics", &mut p.edit),
        ("ROLLUPD", "allow updates on roll", &mut p.roll_update),
        ("RECNBR", "generate", &mut p.recnbr),
        ("RECNBRFLD", "store in a field", &mut p.recnbr_field),
        ("GENKEYS", "generate keys", &mut p.gen_keys),
    ] {
        if let Some(b) = flag(key, label)? {
            *slot = b;
        }
    }
    let h = value("RECNBRHDG");
    if !h.trim().is_empty() {
        p.recnbr_heading = h.trim().to_string();
    }
    match value("PROCESS").trim() {
        "" => {}
        "1" => p.sequential = false,
        "2" => p.sequential = true,
        _ => return Err("Type 1 or 2 for processing.".into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies() {
        let mut p = Program::new("X", "*LIBL", "F");
        let v = |k: &str| match k {
            "DSPFMT" => "1".to_string(),
            "AUDIT" => "n".into(),
            "PROCESS" => "1".into(),
            _ => String::new(),
        };
        apply(&mut p, &v).unwrap();
        assert_eq!(p.format, Format::Single);
        assert!(!p.audit && !p.sequential);
        let bad = |k: &str| if k == "S36" { "X".into() } else { String::new() };
        assert!(apply(&mut p, &bad).is_err());
    }
}
