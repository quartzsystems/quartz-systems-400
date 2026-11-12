// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The screen builders: the screen kit's (`scrkit::ui`), and SST's
//! menu layout on top of them.

use console::Screen;

pub use scrkit::ui::*;

/// SST's function key line on the primary.
pub const SST_KEYS: &str = "F3=Exit         F12=Cancel";

/// A numbered menu as SST shows it on the primary (V7R1M0): the title
/// from `title_col`, no system name, `Select one of the following:` on
/// row 3 and the options from row 5.
pub fn sst_menu(title: &str, title_col: usize, options: &[(u8, &str)], fkeys: &str) -> Screen {
    menu_at(title, title_col, options, fkeys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hidden_field_takes_a_whole_password() {
        assert_eq!(SECRET_LEN, crate::users::PASSWORD_LEN);
    }

    #[test]
    fn the_sst_menu_has_no_system_name() {
        let s = sst_menu("System Service Tools (SST)", 24, &[(1, "Start")], SST_KEYS);
        let d = s.dump();
        let lines: Vec<&str> = d.lines().collect();
        assert_eq!(&lines[0][23..49], "System Service Tools (SST)");
        assert!(lines[1].trim().is_empty());
        assert!(lines[2].starts_with(" Select one of the following:"));
        assert!(lines[4].starts_with("      1. Start"));
        assert!(lines[22].starts_with(" F3=Exit         F12=Cancel"));
    }
}
