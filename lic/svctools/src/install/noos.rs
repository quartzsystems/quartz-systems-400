// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `No Operating System Installed`: option 1 on installation media when
//! no disk holds an ASP 1 partition. It names the last SRC and the disks
//! seen, and offers the install and the service tools instead.

use console::{Attr, Screen};

use crate::env::NonConfigured;
use crate::ui;

/// The choices.
pub const OPTIONS: [(u8, &str); 2] = [
    (2, "Install the operating system"),
    (3, "Use Dedicated Service Tools (DST)"),
];

/// The function key line.
pub const KEYS: &str = "F3=Return to IPL menu";

/// Disk rows that fit above the choices.
const DISK_ROWS: usize = 6;

/// The panel: the last code reached, the disks detected, the choices.
#[must_use]
pub fn screen(system: &str, src: (&str, &str), disks: &[NonConfigured]) -> Screen {
    let mut s = Screen::new();
    ui::header(&mut s, "No Operating System Installed", system);
    let col = ui::BODY_COL;
    let mut row = ui::BODY_ROW;
    s.put(
        row,
        col,
        "No disk unit holds an ASP 1 partition, so there is nothing to IPL.",
        Attr::GREEN,
    );
    row += 2;
    s.put(row, col, &format!("Last SRC  . . . . . :   {}  {}", src.0, src.1), Attr::GREEN);
    row += 2;
    s.put(row, col, "Disk units detected:", Attr::GREEN);
    row += 1;
    s.put(row, col + 2, "Resource  Type Model          Size (MiB)", Attr::GREEN);
    row += 1;
    if disks.is_empty() {
        s.put(row, col + 2, "(No disk units detected)", Attr::GREEN);
    }
    for d in disks.iter().take(DISK_ROWS) {
        s.put(
            row,
            col + 2,
            &format!(
                "{:<9} {:<19} {:>10}",
                d.resource,
                clip(&d.type_model, 19),
                d.size_mib
            ),
            Attr::GREEN,
        );
        row += 1;
    }
    if disks.len() > DISK_ROWS {
        s.put(
            row,
            col + 2,
            &format!("and {} more", disks.len() - DISK_ROWS),
            Attr::GREEN,
        );
    }
    let first = ui::SELECTION_ROW - 1 - OPTIONS.len();
    s.put(first - 1, col, "Select one of the following:", Attr::GREEN);
    for (i, (n, text)) in OPTIONS.iter().enumerate() {
        s.put(first + i, col + 4, &format!("{n:>2}. {text}"), Attr::GREEN);
    }
    s.put(ui::SELECTION_ROW, col, "Selection", Attr::GREEN);
    s.field(ui::SELECTION_ROW + 1, col + 2, 2, false);
    ui::keys(&mut s, KEYS);
    s.cursor = (ui::SELECTION_ROW, col + 1);
    s
}

fn clip(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_the_code_and_the_disks_and_has_one_field() {
        let d = NonConfigured {
            device: "/dev/sda".into(),
            resource: "DD001".into(),
            type_model: "ATA Samsung SSD 870".into(),
            size_mib: 953_869,
            ..NonConfigured::default()
        };
        let s = screen("QS400", ("C1001040", "Clock set from RTC"), &[d]);
        let dump = s.dump();
        assert!(dump.contains("C1001040  Clock set from RTC"));
        assert!(dump.contains("DD001     ATA Samsung SSD 870     953869"));
        assert!(dump.contains("2. Install the operating system"));
        assert!(dump.contains(KEYS));
        assert_eq!(s.fields.len(), 1);
        let none = screen("QS400", ("C1001040", "x"), &[]).dump();
        assert!(none.contains("(No disk units detected)"));
    }
}
