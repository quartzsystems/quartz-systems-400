// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The WRKRDBDIRE displays as 24 rows of 80 columns: the list (position
//! to, the options, entry, remote location and text columns, twelve
//! entries a page) and option 5's entry detail. Geometry, titles and
//! labels follow the pub400 goldens (`golden/phase10/misc/wrkrdbdire*`).
//! Row 24 is left to the caller (its message or copyright line).

use super::rdbdir::RdbEntry;

/// Rows on a display.
pub const ROWS: usize = 24;
/// Columns on a display.
pub const COLS: usize = 80;
/// Entries a page of the list shows.
pub const PAGE: usize = 11;

struct Grid(Vec<Vec<char>>);

impl Grid {
    fn new() -> Self {
        Self(vec![vec![' '; COLS]; ROWS])
    }

    /// Text at a one-based row and column, cut at the right edge.
    fn put(&mut self, row: usize, col: usize, text: &str) {
        let r = &mut self.0[row - 1];
        for (i, c) in text.chars().enumerate() {
            if col - 1 + i >= COLS {
                break;
            }
            r[col - 1 + i] = c;
        }
    }

    fn center(&mut self, row: usize, text: &str) {
        let col = 1 + (COLS - text.chars().count()) / 2;
        self.put(row, col, text);
    }

    fn rows(self) -> Vec<String> {
        self.0.into_iter().map(|r| r.into_iter().collect()).collect()
    }
}

fn cut(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// The list display: `entries` from the first one shown, `more` when a
/// page follows.
pub fn wrkrdbdire(entries: &[&RdbEntry], more: bool) -> Vec<String> {
    let mut g = Grid::new();
    g.center(1, "Work with Relational Database Directory Entries");
    g.put(3, 2, "Position to  . . . . . .");
    g.put(5, 2, "Type options, press Enter.");
    g.put(6, 4, "1=Add   2=Change   4=Remove   5=Display details   6=Print details");
    g.put(8, 30, "Remote");
    g.put(9, 2, "Option  Entry               Location                  Text");
    for (i, e) in entries.iter().take(PAGE).enumerate() {
        let row = 11 + i;
        g.put(row, 10, &cut(&e.rdb, 18));
        g.put(row, 30, &cut(&e.rmtloc, 24));
        g.put(row, 56, &cut(&e.text, 24));
    }
    g.put(22, 74, if more || entries.len() > PAGE { "More..." } else { "Bottom" });
    g.put(23, 2, "F3=Exit   F5=Refresh   F6=Print list   F12=Cancel   F22=Display entire field");
    g.rows()
}

/// Option 5: one entry's detail.
pub fn display_entry(e: &RdbEntry) -> Vec<String> {
    let mut g = Grid::new();
    g.center(1, "Display Relational Database Entry Detail");
    let v = 39;
    g.put(3, 2, "Relational database  . . . . . . :");
    g.put(3, v, &e.rdb);
    let mut row = 4;
    if e.alias != "*NONE" && !e.alias.is_empty() {
        g.put(row, 2, "Relational database alias  . . . :");
        g.put(row, v, &e.alias);
        row += 1;
    }
    g.put(row, 2, "Remote location:");
    g.put(row + 1, 4, "Remote location  . . . . . . . :");
    g.put(row + 1, v, &e.rmtloc);
    g.put(row + 2, 6, "Type . . . . . . . . . . . . :");
    g.put(row + 2, v, &e.loc_type);
    row += 3;
    if e.loc_type == "*IP" {
        g.put(row, 4, "Port number or service name  . :");
        g.put(row, v, &e.port);
        g.put(row + 1, 4, "Remote authentication method:");
        g.put(row + 2, 6, "Preferred method . . . . . . :");
        g.put(row + 2, v, &e.auth);
        g.put(row + 3, 6, "Allow lower authentication . :");
        g.put(row + 3, v, &e.allow_lower);
        g.put(row + 4, 4, "Secure connection  . . . . . . :");
        g.put(row + 4, v, &e.seccnn);
        g.put(row + 5, 4, "Encryption algorithm . . . . . :");
        g.put(row + 5, v, &e.encalg);
        row += 6;
    } else {
        for (label, val) in [
            ("Device . . . . . . . . . . . . :", &e.dev),
            ("Local location . . . . . . . . :", &e.lcllocname),
            ("Remote network identifier  . . :", &e.rmtnetid),
            ("Mode . . . . . . . . . . . . . :", &e.mode),
            ("Transaction program  . . . . . :", &e.tnspgm),
        ] {
            g.put(row, 4, label);
            g.put(row, v, val);
            row += 1;
        }
    }
    g.put(row, 2, "Text . . . . . . . . . . . . . . :");
    g.put(row, v, &cut(&e.text, 42));
    g.put(16, 2, "Relational database type . . . . :");
    g.put(16, v, if e.is_local() { "*LOCAL" } else { "*REMOTE" });
    g.put(20, 74, "Bottom");
    g.put(21, 2, "Press Enter to continue.");
    g.put(23, 2, "F3=Exit   F12=Cancel");
    g.rows()
}
