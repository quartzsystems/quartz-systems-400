// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The messages EDTF and DSPF show: IBM's identifiers (the `CPDB6xx`,
//! `CPFB6xx` and `CPIB6xx` of `QCPFMSG`), Quartz's texts. The same
//! texts are in `runtime/msgf/msgs/qcpfmsg.toml` (a test keeps them
//! together); the session shows them on the message line.

/// A message with its values filled in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Msg {
    /// `AAAnnnn`.
    pub id: &'static str,
    /// The text, values substituted.
    pub text: String,
}

/// Every identifier the editor uses, with its text (`&1` ... for the
/// values).
pub const TEXTS: &[(&str, &str)] = &[
    ("CPDB603", "Type a record number or a column in the field first."),
    ("CPDB605", "The position typed is not a number."),
    ("CPDB607", "CCSID &1 cannot be converted here."),
    ("CPDB608", "That line command is not one the editor knows."),
    ("CPDB612", "The file mixes line ends; the first one found is used on save."),
    ("CPDB613", "&1 changes were not saved."),
    ("CPDB617", "There is no change to repeat: use CHANGE on the control line first."),
    ("CPDB624", "That line end is not one of *CRLF, *LF, *CR, *LFCR, *NL."),
    ("CPDB627", "Name a stream file or a database file, not both."),
    ("CPDB628", "Name a stream file or a database file."),
    ("CPDB629", "A member goes with a database file, not with a stream file."),
    ("CPFB602", "File &1 could not be opened."),
    ("CPFB610", "The control line command is not one the editor knows."),
    ("CPFB612", "Give the text to find (and the new text, to change)."),
    ("CPFB615", "Mark where the lines go with A (after) or B (before)."),
    ("CPFB618", "Text in CCSID &1 cannot be shown in the job's CCSID."),
    ("CPFB619", "The file holds no data."),
    ("CPFB621", "The editor and the browser need a display: run the command from an interactive job."),
    ("CPIB605", "The file was changed. Press F12 again to leave without saving, F3 to save and leave."),
    ("CPIB606", "The browser does not change the file."),
    ("CPIB607", "&1 occurrences of &2 found."),
    ("CPIB608", "&1 occurrences changed."),
    ("CPIB611", "The file will be tagged with CCSID &1 when it is saved."),
    ("CPIB612", "The file's CCSID was changed from &1 to &2."),
];

impl Msg {
    /// Message `id` with `values` for `&1`, `&2` ...
    ///
    /// # Panics
    /// When `id` is not in [`TEXTS`] (a test keeps them in step).
    #[must_use]
    pub fn new(id: &'static str, values: &[&str]) -> Self {
        let (_, t) = TEXTS
            .iter()
            .find(|(i, _)| *i == id)
            .expect("an EDTF message identifier");
        let mut text = (*t).to_string();
        for (n, v) in values.iter().enumerate().rev() {
            text = text.replace(&format!("&{}", n + 1), v);
        }
        Self { id, text }
    }
}
