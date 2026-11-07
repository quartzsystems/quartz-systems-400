// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A user's preferences: every setting's key and default; the store
//! keeps only what differs. Other pages read them through [`Prefs`]:
//! rows per page, the date and time formats, the default schema, the
//! row cap, the 5250 defaults, the home page's layout.

use std::collections::BTreeMap;

use crate::env::Env;

/// Rows per page.
pub const ROWS: &str = "rpp";
/// Date format.
pub const DATE: &str = "df";
/// Time format.
pub const TIME: &str = "tf";
/// Time zone.
pub const ZONE: &str = "tz";
/// Language.
pub const LANG: &str = "lang";
/// Where help opens.
pub const HELP: &str = "h";
/// Confirm before deleting.
pub const CONFIRM_DELETE: &str = "c1";
/// Confirm before ending jobs.
pub const CONFIRM_END: &str = "c2";
/// Confirm before holding and releasing.
pub const CONFIRM_HOLD: &str = "c3";
/// Default schema.
pub const SCHEMA: &str = "ds";
/// Naming convention.
pub const NAMING: &str = "nm";
/// Default output.
pub const OUTPUT: &str = "do";
/// Maximum rows returned.
pub const MAX_ROWS: &str = "mr";
/// Show system column names.
pub const SYSTEM_NAMES: &str = "scn";
/// Default screen size.
pub const SCREEN: &str = "ss";
/// Colours.
pub const COLOURS: &str = "cl";
/// Show keypad.
pub const KEYPAD: &str = "kp";
/// Font size.
pub const FONT: &str = "fs";
/// Cursor.
pub const CURSOR: &str = "cu";
/// An existing file on upload.
pub const EXISTING: &str = "fx";
/// Transfer mode.
pub const TRANSFER: &str = "fm";
/// Line endings of text downloads.
pub const ENDINGS: &str = "fe";
/// How printer output opens.
pub const PRINT_OPEN: &str = "po";
/// Home page layout.
pub const LAYOUT: &str = "hl";
/// Show the counts at sign-on.
pub const COUNTS: &str = "sh";
/// Where the last sign-on came from (kept, not shown).
pub const SIGNON_FROM: &str = "_from";

/// A checkbox that is on.
pub const ON: &str = "on";

/// The rows-per-page choices.
pub const ROW_CHOICES: &[&str] = &["10", "25", "50", "100"];
/// The date formats.
pub const DATE_FORMATS: &[&str] = &["MM/DD/YY", "DD/MM/YY", "YYYY-MM-DD", "DD.MM.YYYY"];
/// The time formats.
pub const TIME_FORMATS: &[&str] = &["HH:MM:SS (24 hour)", "hh:mm:ss AM/PM"];
/// The languages.
pub const LANGUAGES: &[&str] = &["English (US)", "English (UK)", "German", "French"];
/// The output formats.
pub const OUTPUTS: &[&str] = &["HTML", "CSV", "PDF", "Excel-compatible"];
/// The 5250 font sizes.
pub const FONTS: &[&str] = &["9pt", "10pt", "11pt", "12pt"];
/// The 5250 cursors.
pub const CURSORS: &[&str] = &["Underline", "Block"];
/// What an upload does to an existing file.
pub const EXISTING_CHOICES: &[&str] = &["Ask", "Replace", "Skip"];
/// Transfer modes.
pub const TRANSFERS: &[&str] = &["Text", "Binary"];
/// Line endings.
pub const ENDING_CHOICES: &[&str] = &["CRLF (Windows)", "LF (Unix)"];
/// How printer output opens.
pub const PRINT_CHOICES: &[&str] = &["Text", "PDF"];
/// The time zone that is the system's.
pub const ZONE_SYSTEM: &str = "System";
/// UTC.
pub const ZONE_UTC: &str = "UTC";

/// Every setting and its default (a checkbox's default is [`ON`] or
/// empty). The default schema's default is the first one offered.
pub const DEFAULTS: &[(&str, &str)] = &[
    (ROWS, "25"),
    (DATE, "MM/DD/YY"),
    (TIME, "HH:MM:SS (24 hour)"),
    (ZONE, ZONE_SYSTEM),
    (LANG, "English (US)"),
    (HELP, "win"),
    (CONFIRM_DELETE, ON),
    (CONFIRM_END, ON),
    (CONFIRM_HOLD, ""),
    (SCHEMA, ""),
    (NAMING, "sql"),
    (OUTPUT, "HTML"),
    (MAX_ROWS, "500"),
    (SYSTEM_NAMES, ""),
    (SCREEN, "24"),
    (COLOURS, "g"),
    (KEYPAD, ON),
    (FONT, "10pt"),
    (CURSOR, "Underline"),
    (EXISTING, "Ask"),
    (TRANSFER, "Text"),
    (ENDINGS, "CRLF (Windows)"),
    (PRINT_OPEN, "Text"),
    (LAYOUT, "list"),
    (COUNTS, ON),
];

/// The default of `key`.
pub fn default(key: &str) -> &'static str {
    DEFAULTS.iter().find(|d| d.0 == key).map_or("", |d| d.1)
}

/// A user's preferences, the defaults filled in.
#[derive(Clone, Debug, Default)]
pub struct Prefs {
    /// Stored values.
    pub stored: BTreeMap<String, String>,
    /// The schemas offered; the first is the default schema's default.
    pub schemas: Vec<String>,
}

impl Prefs {
    /// `user`'s.
    pub fn load(env: &Env, user: &str) -> Self {
        Self {
            stored: env.prefs.get(user).into_iter().collect(),
            schemas: env.prefs.schemas(user),
        }
    }

    /// The value of `key`.
    pub fn get(&self, key: &str) -> String {
        if let Some(v) = self.stored.get(key) {
            return v.clone();
        }
        if key == SCHEMA {
            return self.schemas.first().cloned().unwrap_or_default();
        }
        default(key).to_string()
    }

    /// Whether checkbox `key` is on.
    pub fn on(&self, key: &str) -> bool {
        self.get(key) == ON
    }

    /// Rows per page.
    pub fn rows(&self) -> usize {
        self.get(ROWS).parse().unwrap_or(25)
    }

    /// The row cap of queries.
    pub fn max_rows(&self) -> usize {
        self.get(MAX_ROWS).parse().unwrap_or(500)
    }

    /// The values to store: those that differ from the defaults.
    pub fn to_store(&self) -> Vec<(String, String)> {
        self.stored
            .iter()
            .filter(|(k, v)| {
                let d = if k.as_str() == SCHEMA {
                    self.schemas.first().map_or("", String::as_str)
                } else {
                    default(k)
                };
                k.starts_with('_') || v.as_str() != d
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// `secs` as `user` reads dates and times, `offset` the system
    /// zone's at `secs`.
    pub fn when(&self, secs: u64, offset: i64) -> (String, String) {
        let offset = if self.get(ZONE) == ZONE_UTC {
            0
        } else {
            offset
        };
        crate::util::stamp(
            secs,
            offset,
            &self.get(DATE),
            self.get(TIME) != TIME_FORMATS[0],
        )
    }
}
