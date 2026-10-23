// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The test facility's displays around the record under test: Set Test
//! Output Data (the option indicators and output values to write it
//! with) and Display Test Input Data (what came back: the key, the
//! response indicators and the input fields).

use uim::Screen;
use uim::render::{Attr, Target};

use super::{blank, input, keys, leader, message, more, put, title_centered, var};

/// A field on the test displays: name, value, length shown.
pub type TestField = (String, String, usize);

/// The list's name in the screen's targets.
pub const LIST: &str = "TESTFLD";
/// Fields on a page.
pub const PAGE: usize = 10;

fn head(s: &mut Screen, title_text: &str, file: &str, record: &str) {
    title_centered(s, title_text);
    put(s, 3, 2, Attr::Normal, "Display file . . :");
    put(s, 3, 21, Attr::HighIntensity, file);
    put(s, 4, 2, Attr::Normal, "Record . . . . . :");
    put(s, 4, 21, Attr::HighIntensity, record);
}

/// Set Test Output Data: `INDS` (the indicators to set on, as numbers)
/// and a value per output field.
#[must_use]
pub fn set_output(file: &str, record: &str, inds: &str, fields: &[TestField], top: usize, msg: Option<&str>) -> Screen {
    let mut s = blank();
    head(&mut s, "Set Test Output Data", file, record);
    put(&mut s, 6, 2, Attr::Blue, "Type indicators and values, press Enter to show the record.");
    put(&mut s, 8, 4, Attr::Normal, &leader("Indicators on", 4, 26));
    var(&mut s, 8, 30, 48, "INDS", inds);
    put(&mut s, 10, 2, Attr::HighIntensity, "Field");
    put(&mut s, 10, 14, Attr::HighIntensity, "Value");
    if fields.is_empty() {
        put(&mut s, 11, 4, Attr::Normal, "(No output fields)");
    }
    for (i, (name, value, len)) in fields.iter().enumerate().skip(top).take(PAGE) {
        let row = 11 + i - top;
        put(&mut s, row, 2, Attr::Normal, name);
        input(
            &mut s,
            row,
            14,
            (*len).clamp(1, 65),
            Target::ListCell {
                list: LIST.into(),
                row_index: i,
                var: "VALUE".into(),
            },
            value,
        );
    }
    more(&mut s, 21, top + PAGE >= fields.len());
    keys(&mut s, &[(23, 2, "F3=Exit"), (23, 12, "F12=Cancel")]);
    message(&mut s, msg);
    s.cursor = Some((8, 30));
    s
}

/// Display Test Input Data: the key that ended the read, the response
/// indicators that came back on and the input fields' values.
#[must_use]
pub fn show_input(
    file: &str,
    record: &str,
    key: &str,
    inds: &str,
    fields: &[TestField],
    top: usize,
    msg: Option<&str>,
) -> Screen {
    let mut s = blank();
    head(&mut s, "Display Test Input Data", file, record);
    put(&mut s, 6, 2, Attr::Normal, &leader("Key pressed", 2, 24));
    put(&mut s, 6, 28, Attr::HighIntensity, key);
    put(&mut s, 7, 2, Attr::Normal, &leader("Indicators on", 2, 24));
    put(&mut s, 7, 28, Attr::HighIntensity, if inds.is_empty() { "(none)" } else { inds });
    put(&mut s, 9, 2, Attr::Blue, "Press Enter to show the record again, F3 to end the test.");
    put(&mut s, 10, 2, Attr::HighIntensity, "Field");
    put(&mut s, 10, 14, Attr::HighIntensity, "Value");
    if fields.is_empty() {
        put(&mut s, 11, 4, Attr::Normal, "(No input fields)");
    }
    for (i, (name, value, _)) in fields.iter().enumerate().skip(top).take(PAGE) {
        let row = 11 + i - top;
        put(&mut s, row, 2, Attr::Normal, name);
        put(&mut s, row, 14, Attr::Normal, value);
    }
    more(&mut s, 21, top + PAGE >= fields.len());
    keys(&mut s, &[(23, 2, "F3=Exit"), (23, 12, "F12=Cancel")]);
    message(&mut s, msg);
    s
}
