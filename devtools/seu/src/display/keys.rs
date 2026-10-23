// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The function key lines of SEU's displays: each legend at the
//! column IBM's display puts it.

use crate::session::Mode;

/// One key line: legends at their screen columns.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Legend(pub Vec<(usize, &'static str)>);

impl Legend {
    /// The line as the text of a field starting at screen column `from`.
    #[must_use]
    pub fn text(&self, from: usize) -> String {
        let mut out: Vec<char> = Vec::new();
        for (col, t) in &self.0 {
            let at = col - from;
            if out.len() < at {
                out.resize(at, ' ');
            }
            out.truncate(at);
            out.extend(t.chars());
        }
        out.into_iter().collect()
    }
}

fn l(v: &[(usize, &'static str)]) -> Legend {
    Legend(v.to_vec())
}

/// The edit or browse display's keys: the set shown, or the prompt's.
#[must_use]
pub fn session(mode: Mode, set: u8, prompt: bool) -> (Legend, Legend) {
    let defaults = (
        l(&[(2, "F13=Change session defaults"), (32, "F14=Find/Change options")]),
        l(&[(2, "F15=Browse/Copy options"), (32, "F24=More keys")]),
    );
    if prompt {
        return match set {
            0 => (
                l(&[(2, "F3=Exit"), (12, "F4=Prompt"), (24, "F5=Refresh"), (44, "F11=Previous record")]),
                l(&[(2, "F12=Cancel"), (24, "F23=Select prompt"), (44, "F24=More keys")]),
            ),
            _ => defaults,
        };
    }
    match (mode, set) {
        (Mode::Edit, 0) => (
            l(&[
                (2, "F3=Exit"),
                (12, "F4=Prompt"),
                (24, "F5=Refresh"),
                (37, "F9=Retrieve"),
                (51, "F10=Cursor"),
                (64, "F11=Toggle"),
            ]),
            l(&[(2, "F16=Repeat find"), (24, "F17=Repeat change"), (51, "F24=More keys")]),
        ),
        (Mode::Edit, 1) => defaults,
        (Mode::Edit, _) => (
            l(&[(2, "F19=Left"), (13, "F20=Right"), (25, "F21=System command")]),
            l(&[(2, "F23=Select prompt"), (25, "F24=More keys")]),
        ),
        (Mode::Browse, 0) => (
            l(&[
                (2, "F3=Exit"),
                (12, "F5=Refresh"),
                (25, "F9=Retrieve"),
                (39, "F10=Cursor"),
                (52, "F11=Toggle"),
                (65, "F12=Cancel"),
            ]),
            l(&[(2, "F16=Repeat find"), (25, "F24=More keys")]),
        ),
        (Mode::Browse, 1) => (
            l(&[(2, "F13=Change session defaults"), (32, "F14=Find options")]),
            l(&[(2, "F15=Browse options"), (32, "F24=More keys")]),
        ),
        (Mode::Browse, _) => (
            l(&[(2, "F19=Left"), (13, "F20=Right"), (25, "F21=System command")]),
            l(&[(2, "F24=More keys")]),
        ),
    }
}

/// The session defaults display's keys.
#[must_use]
pub fn defaults(mode: Mode) -> (Legend, Legend) {
    match mode {
        Mode::Edit => (
            l(&[(2, "F3=Exit"), (15, "F5=Refresh"), (28, "F12=Cancel")]),
            l(&[(2, "F14=Find/Change options"), (28, "F15=Browse/Copy options")]),
        ),
        Mode::Browse => (
            l(&[(2, "F3=Exit"), (12, "F5=Refresh"), (25, "F12=Cancel")]),
            l(&[(2, "F14=Find options"), (25, "F15=Browse options")]),
        ),
    }
}

/// The find options display's keys.
#[must_use]
pub fn find(mode: Mode) -> (Legend, Legend) {
    match mode {
        Mode::Edit => (
            l(&[(2, "F3=Exit"), (12, "F5=Refresh"), (29, "F12=Cancel"), (42, "F13=Change session defaults")]),
            l(&[(2, "F15=Browse/Copy options"), (29, "F16=Find"), (42, "F17=Change")]),
        ),
        Mode::Browse => (
            l(&[(2, "F3=Exit"), (12, "F5=Refresh"), (25, "F12=Cancel"), (38, "F13=Change session defaults")]),
            l(&[(2, "F15=Browse options"), (25, "F16=Find")]),
        ),
    }
}

/// The browse options display's keys.
#[must_use]
pub fn browse(mode: Mode) -> (Legend, Legend) {
    let f14 = match mode {
        Mode::Edit => "F14=Find/Change options",
        Mode::Browse => "F14=Find options",
    };
    (
        l(&[(2, "F3=Exit"), (16, "F4=Prompt"), (32, "F5=Refresh"), (49, "F12=Cancel")]),
        l(&[(2, "F13=Change session defaults"), (32, f14)]),
    )
}

/// The Exit display's keys.
#[must_use]
pub fn exit() -> (Legend, Legend) {
    (
        Legend::default(),
        l(&[(2, "F3=Exit"), (12, "F4=Prompt"), (24, "F5=Refresh"), (37, "F12=Cancel")]),
    )
}

/// The system command window's keys, at the window's columns.
#[must_use]
pub fn window() -> Legend {
    l(&[(5, "F4=Prompt"), (17, "F9=Retrieve"), (31, "F12=Cancel")])
}

/// Select Prompt's keys.
#[must_use]
pub fn select() -> (Legend, Legend) {
    (Legend::default(), l(&[(2, "F12=Cancel"), (15, "F23=Select user prompt")]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legends_land_on_their_columns() {
        let (a, _) = session(Mode::Edit, 0, false);
        let t = a.text(2);
        assert_eq!(t.find("F4=Prompt"), Some(10));
        assert_eq!(t.find("F11=Toggle"), Some(62));
    }
}
