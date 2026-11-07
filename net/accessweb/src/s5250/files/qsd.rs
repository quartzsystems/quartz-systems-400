// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `.qsd` files: a saved 5250 display session, as QS/400 Access for
//! Desktop writes them (`qsa-config/src/qsd.rs`), so one file serves both
//! clients.
//!
//! ```toml
//! [qsaccess]
//! kind = "display"
//! version = 1
//!
//! [system]
//! name = "QSHQ400"
//! host = "10.0.10.20"
//! telnet_port = 992
//! tls = true
//!
//! [session]
//! title = "QSHQ400 operator"
//! screen_size = "24x80"
//! ccsid = 37
//! device_name = "QPADEV0003"
//! colour_map = "console"
//! auto_connect = true
//! auto_sign_on = true
//! keypad_visible = true
//!
//! [keymap]
//! base = "defaults-v1"
//! [keymap.overrides]
//! "Ctrl+Enter" = "FieldExit"
//!
//! [hotspots]
//! enabled = true
//! kinds = ["function-keys", "paging"]
//!
//! [macros]
//! files = ["SIGNON-DEV.qsm"]
//! ```
//!
//! A session file never holds a password.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::doc::{self, Extra, FileError, Header};
use super::keymap::{self, KeyMap};

/// File extension, without the dot.
pub const EXTENSION: &str = "qsd";
/// Hotspot kind: function-key labels such as `F3=Exit`.
pub const HOTSPOT_FUNCTION_KEYS: &str = "function-keys";
/// Hotspot kind: `More...` and `Bottom`.
pub const HOTSPOT_PAGING: &str = "paging";
/// The CCSIDs a display session can use, and what they are.
pub const CCSIDS: &[(u16, &str)] = &[
    (37, "USA, Canada"),
    (273, "Austria, Germany"),
    (285, "United Kingdom"),
    (297, "France"),
    (500, "International"),
    (1140, "USA, Canada, euro"),
];

/// Screen size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScreenSize {
    /// 24 x 80.
    #[default]
    #[serde(rename = "24x80")]
    S24x80,
    /// 27 x 132.
    #[serde(rename = "27x132")]
    S27x132,
}

impl ScreenSize {
    /// Rows and columns.
    pub fn rows_cols(self) -> (usize, usize) {
        match self {
            ScreenSize::S24x80 => (24, 80),
            ScreenSize::S27x132 => (27, 132),
        }
    }

    /// As shown: `24 x 80`.
    pub fn label(self) -> &'static str {
        match self {
            ScreenSize::S24x80 => "24 x 80",
            ScreenSize::S27x132 => "27 x 132",
        }
    }

    /// As forms post it: `24x80`.
    pub fn key(self) -> &'static str {
        match self {
            ScreenSize::S24x80 => "24x80",
            ScreenSize::S27x132 => "27x132",
        }
    }
}

/// Colour map.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ColorMap {
    /// The QS/400 console's colours.
    #[default]
    Console,
    /// Black on white.
    BlackOnWhite,
}

impl ColorMap {
    /// As shown.
    pub fn label(self) -> &'static str {
        match self {
            ColorMap::Console => "QS/400 console",
            ColorMap::BlackOnWhite => "Black on white",
        }
    }
}

/// Check a device name: empty, or 1 to 10 of A-Z, 0-9, #, $, @ and _,
/// starting with a letter, #, $ or @. In upper case.
pub fn check_device(name: &str) -> Result<String, String> {
    let n = name.trim().to_ascii_uppercase();
    if n.is_empty() || n == "*DFT" {
        return Ok(String::new());
    }
    let ok = n.len() <= 10
        && n.chars().all(|c| c.is_ascii_alphanumeric() || "#$@_".contains(c))
        && n.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || "#$@".contains(c));
    if ok {
        Ok(n)
    } else {
        Err("Device name must be 1 to 10 letters, digits, #, $, @ or _, starting with a letter, #, $ or @.".into())
    }
}

/// Whether `name` is a host name or an IP address.
pub fn check_name(name: &str) -> bool {
    let name = name.trim();
    if name.parse::<std::net::IpAddr>().is_ok() {
        return true;
    }
    !name.is_empty()
        && name.len() <= 253
        && name.split('.').all(|l| {
            !l.is_empty() && l.len() <= 63 && !l.starts_with('-') && !l.ends_with('-') && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

/// `[system]`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SystemRef {
    /// The system's name.
    pub name: String,
    /// Host name or address; empty: the same as `name`.
    pub host: String,
    /// Telnet port.
    pub telnet_port: u16,
    /// Telnet over TLS.
    pub tls: bool,
    /// Keys this release does not know.
    #[serde(flatten)]
    pub extra: Extra,
}

impl Default for SystemRef {
    fn default() -> SystemRef {
        SystemRef {
            name: String::new(),
            host: String::new(),
            telnet_port: 23,
            tls: false,
            extra: Extra::new(),
        }
    }
}

/// A window's place, kept for the desktop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Geometry {
    /// Left.
    pub x: i32,
    /// Top.
    pub y: i32,
    /// Width.
    pub w: i32,
    /// Height.
    pub h: i32,
}

/// `[session]`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[allow(clippy::struct_excessive_bools)]
pub struct Session {
    /// Shown in titles and lists.
    pub title: String,
    /// Screen size.
    pub screen_size: ScreenSize,
    /// CCSID.
    pub ccsid: u16,
    /// Empty: the system names the device.
    pub device_name: String,
    /// Colour map.
    pub colour_map: ColorMap,
    /// Connect when the session opens.
    pub auto_connect: bool,
    /// Sign on with a one-time QAP Telnet ticket (Telnet over TLS only).
    pub auto_sign_on: bool,
    /// The keypad shows.
    pub keypad_visible: bool,
    /// The desktop's window.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<Geometry>,
    /// Keys this release does not know.
    #[serde(flatten)]
    pub extra: Extra,
}

impl Default for Session {
    fn default() -> Session {
        Session {
            title: String::new(),
            screen_size: ScreenSize::S24x80,
            ccsid: 37,
            device_name: String::new(),
            colour_map: ColorMap::Console,
            auto_connect: true,
            auto_sign_on: false,
            keypad_visible: false,
            window: None,
            extra: Extra::new(),
        }
    }
}

/// `[keymap]`: the changes from a set of built-in defaults.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KeymapOverrides {
    /// Which defaults the overrides apply to ([`keymap::DEFAULTS_ID`]).
    pub base: String,
    /// Key to function; an empty function takes the key's default away.
    #[serde(default)]
    pub overrides: BTreeMap<String, String>,
    /// Keys this release does not know.
    #[serde(flatten)]
    pub extra: Extra,
}

impl KeymapOverrides {
    /// The changes that turn the defaults into `map`.
    pub fn from_map(map: &KeyMap) -> KeymapOverrides {
        let defaults = KeyMap::defaults();
        let mut overrides = BTreeMap::new();
        for (k, f) in &map.keys {
            if defaults.function(k) != Some(f.as_str()) {
                overrides.insert(k.clone(), f.clone());
            }
        }
        for k in defaults.keys.keys() {
            if !map.keys.contains_key(k) {
                overrides.insert(k.clone(), String::new());
            }
        }
        KeymapOverrides {
            base: keymap::DEFAULTS_ID.to_owned(),
            overrides,
            extra: Extra::new(),
        }
    }

    /// The defaults with the overrides applied.
    pub fn to_map(&self) -> Result<KeyMap, String> {
        if self.base != keymap::DEFAULTS_ID {
            return Err(format!(
                "keyboard map base \"{}\" is not known; this release knows \"{}\"",
                self.base,
                keymap::DEFAULTS_ID
            ));
        }
        let mut map = KeyMap::defaults();
        for (k, f) in &self.overrides {
            if f.is_empty() {
                map.keys.remove(k);
            } else if keymap::is_function(f) {
                map.keys.insert(k.clone(), f.clone());
            } else {
                return Err(format!("key {k} is given an unknown function \"{f}\""));
            }
        }
        Ok(map)
    }
}

/// `[hotspots]`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hotspots {
    /// Hotspots act at all.
    pub enabled: bool,
    /// The kinds switched on.
    pub kinds: Vec<String>,
    /// Keys this release does not know.
    #[serde(flatten)]
    pub extra: Extra,
}

impl Default for Hotspots {
    fn default() -> Hotspots {
        Hotspots {
            enabled: true,
            kinds: vec![HOTSPOT_FUNCTION_KEYS.into(), HOTSPOT_PAGING.into()],
            extra: Extra::new(),
        }
    }
}

impl Hotspots {
    /// Whether hotspots of `kind` act.
    pub fn on(&self, kind: &str) -> bool {
        self.enabled && self.kinds.iter().any(|k| k == kind)
    }

    /// Switch a kind on or off.
    pub fn set(&mut self, kind: &str, on: bool) {
        self.kinds.retain(|k| k != kind);
        if on {
            self.kinds.push(kind.to_owned());
            self.kinds.sort();
        }
    }
}

/// `[macros]`: the macros of the session's Macro menu, as file names.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Macros {
    /// The macro files, relative to the session file.
    pub files: Vec<String>,
    /// Keys this release does not know.
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct SessionDoc {
    qsaccess: Header,
    #[serde(default)]
    system: SystemRef,
    #[serde(default)]
    session: Session,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    keymap: Option<KeymapOverrides>,
    #[serde(default)]
    hotspots: Hotspots,
    #[serde(default)]
    macros: Macros,
    #[serde(flatten)]
    extra: Extra,
}

/// A display session file.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionFile {
    /// The header.
    pub header: Header,
    /// The system.
    pub system: SystemRef,
    /// The session.
    pub session: Session,
    /// `None`: no map of its own; the defaults apply.
    pub keymap: Option<KeymapOverrides>,
    /// Hotspots.
    pub hotspots: Hotspots,
    /// Macros.
    pub macros: Macros,
    /// Tables this release does not know.
    pub extra: Extra,
}

impl SessionFile {
    /// A new session for system `name`.
    pub fn new(name: &str) -> SessionFile {
        SessionFile {
            header: Header::new(doc::DISPLAY),
            system: SystemRef {
                name: name.into(),
                ..SystemRef::default()
            },
            session: Session::default(),
            keymap: None,
            hotspots: Hotspots::default(),
            macros: Macros::default(),
            extra: Extra::new(),
        }
    }

    /// The keyboard map the session uses.
    pub fn keymap(&self) -> KeyMap {
        self.keymap.as_ref().and_then(|k| k.to_map().ok()).unwrap_or_default()
    }

    /// Keep `map` as the session's keyboard map.
    pub fn set_keymap(&mut self, map: &KeyMap) {
        let extra = self.keymap.take().map(|k| k.extra).unwrap_or_default();
        self.keymap = Some(KeymapOverrides {
            extra,
            ..KeymapOverrides::from_map(map)
        });
    }

    /// The title to show: the session's, else the system's name.
    pub fn display_title(&self) -> String {
        let t = self.session.title.trim();
        if t.is_empty() { self.system.name.clone() } else { t.to_owned() }
    }

    /// Check every value a session needs.
    pub fn validate(&self) -> Result<(), FileError> {
        let bad = |s: String| Err(FileError::Invalid(s));
        if self.system.name.trim().is_empty() {
            return bad("[system] has no name".into());
        }
        if !check_name(&self.system.name) {
            return bad(format!("system name {} is not a host name or address", self.system.name));
        }
        if self.system.telnet_port == 0 {
            return bad("Telnet port 0 is not valid".into());
        }
        if !CCSIDS.iter().any(|c| c.0 == self.session.ccsid) {
            return bad(format!("CCSID {} is not supported", self.session.ccsid));
        }
        if let Err(e) = check_device(&self.session.device_name) {
            return bad(e.trim_end_matches('.').to_owned());
        }
        if let Some(k) = &self.keymap {
            k.to_map().map_err(FileError::Invalid)?;
        }
        Ok(())
    }

    /// Parse `.qsd` text.
    pub fn parse(text: &str) -> Result<SessionFile, FileError> {
        let d: SessionDoc = doc::parse(text, doc::DISPLAY)?;
        let f = SessionFile {
            header: d.qsaccess,
            system: d.system,
            session: d.session,
            keymap: d.keymap,
            hotspots: d.hotspots,
            macros: d.macros,
            extra: d.extra,
        };
        f.validate()?;
        Ok(f)
    }

    /// The `.qsd` text.
    pub fn to_text(&self) -> String {
        let mut header = self.header.clone();
        header.kind = doc::DISPLAY.into();
        header.version = doc::VERSION;
        doc::to_text(&SessionDoc {
            qsaccess: header,
            system: self.system.clone(),
            session: self.session.clone(),
            keymap: self.keymap.clone(),
            hotspots: self.hotspots.clone(),
            macros: self.macros.clone(),
            extra: self.extra.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut f = SessionFile::new("QSHQ400");
        f.session.title = "Payroll".into();
        f.session.screen_size = ScreenSize::S27x132;
        f.session.colour_map = ColorMap::BlackOnWhite;
        let mut map = KeyMap::defaults();
        map.assign("Ctrl+Enter", "FieldExit");
        map.clear_function("Clear");
        f.set_keymap(&map);
        f.hotspots.set(HOTSPOT_PAGING, false);
        f.macros.files.push("signon.qsm".into());
        let text = f.to_text();
        assert!(text.starts_with("[qsaccess]\nkind = \"display\"\nversion = 1\n"), "{text}");
        assert!(text.contains("colour_map = \"black-on-white\"") && text.contains("Pause = \"\""), "{text}");
        let back = SessionFile::parse(&text).unwrap();
        assert_eq!(back, f);
        assert_eq!(back.keymap().function("Pause"), None);
    }

    #[test]
    fn unknown_keys_are_kept_and_bad_values_refused() {
        let text = "future = 1\n[qsaccess]\nkind = \"display\"\nversion = 1\nbuild = \"x\"\n[system]\nname = \"sys1\"\nsite = \"Leeds\"\n[session]\nzoom = 2\n[printer]\nname = \"LPT1\"\n";
        let f = SessionFile::parse(text).unwrap();
        let out = f.to_text();
        for want in ["future = 1", "build = \"x\"", "site = \"Leeds\"", "zoom = 2", "[printer]"] {
            assert!(out.contains(want), "{want}: {out}");
        }
        let with = |b: &str| SessionFile::parse(&format!("[qsaccess]\nkind = \"display\"\nversion = 1\n{b}"));
        assert!(matches!(with(""), Err(FileError::Invalid(_))));
        assert!(matches!(with("[system]\nname = \"a\"\n[session]\nccsid = 9\n"), Err(FileError::Invalid(s)) if s.contains("CCSID 9")));
        assert!(matches!(with("[system]\nname = \"a\"\n[session]\nscreen_size = \"25x80\"\n"), Err(FileError::Parse(_))));
        assert!(matches!(
            SessionFile::parse("[qsaccess]\nkind = \"display\"\nversion = 2\n[system]\nname = 5\n"),
            Err(FileError::Newer(2))
        ));
    }
}
