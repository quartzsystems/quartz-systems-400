// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Evaluate Expression (the debug expressions so far and their values,
//! with the debug command line) and Select View (F15). Neither shows
//! without a running program or a source view on pub400; their rows
//! follow Display Module Source's.

use uim::Screen;
use uim::render::{Attr, Target};

use super::source::CMD;
use super::{blank, input, keys, message, more, put, title};

/// Rows of history a page shows.
pub const PAGE: usize = 14;

/// Evaluate Expression: `lines` are the history (each command then its
/// value lines), the page starting at `first` (rows 5 to 18).
#[must_use]
pub fn evaluate(lines: &[String], first: usize, cmdline: &str, msg: Option<&str>) -> Screen {
    let mut s = blank();
    title(&mut s, "Evaluate Expression");
    put(&mut s, 3, 2, Attr::Blue, "Previous debug expressions");
    for (i, l) in lines.iter().skip(first).take(PAGE).enumerate() {
        let (attr, text) = match l.strip_prefix("> ") {
            Some(c) => (Attr::HighIntensity, format!("> {c}")),
            None => (Attr::Normal, format!("  {l}")),
        };
        put(&mut s, 5 + i, 2, attr, &text);
    }
    more(&mut s, 19, first + PAGE >= lines.len());
    put(&mut s, CMD.0, 2, Attr::Normal, "Debug . . .");
    input(&mut s, CMD.0, CMD.1, CMD.2, Target::CmdLine, cmdline);
    keys(
        &mut s,
        &[
            (22, 2, "F3=Exit"),
            (22, 12, "F9=Retrieve"),
            (22, 26, "F12=Cancel"),
            (22, 39, "F16=Repeat find"),
            (22, 57, "F19=Left"),
            (22, 68, "F20=Right"),
            (23, 2, "F21=Command entry"),
            (23, 26, "F23=Display output"),
        ],
    );
    message(&mut s, msg);
    s.cursor = Some((CMD.0, CMD.1));
    s
}

/// The history of one command: the command (its first word in upper
/// case), then its answer's lines.
#[must_use]
pub fn history_entry(command: &str, value: &str) -> Vec<String> {
    let command = command.trim();
    let (w, rest) = command.split_once(' ').unwrap_or((command, ""));
    let shown = format!("{} {}", w.to_ascii_uppercase(), rest.trim_start());
    let mut v = vec![format!("> {}", shown.trim_end())];
    v.extend(value.lines().filter(|l| !l.is_empty()).map(str::to_string));
    v
}

/// The first history line of the last page.
#[must_use]
pub fn last_page(lines: usize) -> usize {
    lines.saturating_sub(PAGE)
}

/// Select View: the current view and the views the module has, with
/// `1=Select`.
#[must_use]
pub fn select_view(current: &str, views: &[String], msg: Option<&str>) -> Screen {
    let mut s = blank();
    title(&mut s, "Select View");
    put(&mut s, 3, 2, Attr::Normal, "Current View . . . :");
    put(&mut s, 3, 25, Attr::Normal, current);
    put(&mut s, 5, 2, Attr::Blue, "Type option, press Enter.");
    put(&mut s, 6, 4, Attr::Blue, "1=Select");
    put(&mut s, 8, 2, Attr::HighIntensity, "Opt");
    put(&mut s, 8, 8, Attr::HighIntensity, "View");
    for (i, v) in views.iter().enumerate().take(9) {
        input(
            &mut s,
            9 + i,
            3,
            1,
            Target::ListOption {
                list: "VIEWS".into(),
                row_index: i,
            },
            "",
        );
        put(&mut s, 9 + i, 8, Attr::Normal, v);
    }
    more(&mut s, 19, true);
    keys(&mut s, &[(22, 2, "F12=Cancel")]);
    message(&mut s, msg);
    s.cursor = Some((9, 3));
    s
}

/// The names Select View gives a module's views, by `DBGVIEW`.
#[must_use]
pub fn view_names(dbgview: &str, language: &str) -> Vec<String> {
    let lang = match language {
        "CLLE" => "ILE CL",
        "RPGLE" | "SQLRPGLE" => "ILE RPG",
        "QIRC" => "QIR",
        other => other,
    };
    match dbgview {
        "*SOURCE" => vec![format!("{lang} Source View")],
        "*LIST" => vec![format!("{lang} Listing View")],
        "*ALL" => vec![
            format!("{lang} Source View"),
            format!("{lang} Listing View"),
            format!("{lang} Statement View"),
        ],
        _ => vec![format!("{lang} Statement View")],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluate_history() {
        let mut h = history_entry("EVAL &X", "&X = 5");
        h.extend(history_entry("EVAL &N : X", "0000  C1C2  *AB*"));
        let t = evaluate(&h, 0, "", None).text_rows();
        assert!(t[0].contains("Evaluate Expression"));
        assert!(t[4].contains("> EVAL &X"));
        assert!(t[5].contains("&X = 5"));
        assert!(t[18].contains("Bottom"));
    }

    #[test]
    fn views() {
        assert_eq!(view_names("*ALL", "CLLE").len(), 3);
        let t = select_view("ILE CL Source View", &view_names("*SOURCE", "CLLE"), None).text_rows();
        assert!(t[8].contains("ILE CL Source View"));
    }
}
