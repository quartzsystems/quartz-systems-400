// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The two IBM inventories: `ui/menus/inventory/ibm-menu-list.txt` and
//! `tools/conformance/oracle/ibm-cmd-list.txt`. Both are fixed-column
//! text: library (11), name (11), attribute (7), releases (7), text.

/// A `*MENU` of IBM's list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuEntry {
    /// The menu.
    pub name: String,
    /// `UIM`, `DSPF` or `PGM`.
    pub attr: String,
    /// The releases that list it: 71, 75.
    pub releases: Vec<u8>,
    /// The object text.
    pub text: String,
}

/// A `*CMD` of IBM's list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CmdEntry {
    /// Its library.
    pub lib: String,
    /// The command.
    pub name: String,
    /// A proxy command (`PRX`).
    pub proxy: bool,
    /// The releases that list it.
    pub releases: Vec<u8>,
    /// The object text, the command's title.
    pub text: String,
}

/// The five columns of a row, or `None` for a comment or a blank line.
fn columns(line: &str) -> Option<[String; 5]> {
    let line = line.trim_end_matches(['\r', '\n']);
    if line.trim().is_empty() || line.starts_with('#') {
        return None;
    }
    let at = |a: usize, b: usize| -> String {
        let end = b.min(line.len());
        line.get(a..end).unwrap_or("").trim().to_string()
    };
    Some([
        at(0, 11),
        at(11, 22),
        at(22, 29),
        at(29, 36),
        at(36, line.len()),
    ])
}

fn releases(s: &str) -> Vec<u8> {
    s.split(',').filter_map(|r| r.trim().parse().ok()).collect()
}

/// Parse the menu list.
#[must_use]
pub fn parse_menu_list(text: &str) -> Vec<MenuEntry> {
    text.lines()
        .filter_map(columns)
        .map(|[_, name, attr, rel, text]| MenuEntry {
            name,
            attr,
            releases: releases(&rel),
            text,
        })
        .collect()
}

/// Parse the command list.
#[must_use]
pub fn parse_cmd_list(text: &str) -> Vec<CmdEntry> {
    text.lines()
        .filter_map(columns)
        .map(|[lib, name, attr, rel, text]| CmdEntry {
            lib,
            name,
            proxy: attr == "PRX",
            releases: releases(&rel),
            text,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_parse() {
        let menus = parse_menu_list(
            "# a comment\n\nQSYS       CMDJOB     UIM    71,75  Job Commands\nQSYS       CMDAMT     UIM    75     AMT Commands\n",
        );
        assert_eq!(menus.len(), 2);
        assert_eq!(menus[0].name, "CMDJOB");
        assert_eq!(menus[0].releases, vec![71, 75]);
        assert_eq!(menus[0].text, "Job Commands");
        assert_eq!(menus[1].releases, vec![75]);
        let cmds = parse_cmd_list(
            "QSYS       ADDASPCPYD PRX    75     \nQTCP       NETSTAT           71,75  Work with TCP/IP Network Sts",
        );
        assert!(cmds[0].proxy);
        assert_eq!(cmds[0].text, "");
        assert_eq!(cmds[1].lib, "QTCP");
        assert_eq!(cmds[1].text, "Work with TCP/IP Network Sts");
    }
}
