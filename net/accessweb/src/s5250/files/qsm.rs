// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `.qsm` files: a macro for a 5250 display session, as QS/400 Access
//! for Desktop writes them (`qsa-config/src/qsm.rs`).
//!
//! ```toml
//! [qsaccess]
//! kind = "macro"
//! version = 1
//!
//! [macro]
//! name = "Sign on"
//! screen_size = "24x80"
//!
//! [[step]]
//! action = "wait-text"
//! row = 1
//! col = 36
//! text = "Sign On"
//! timeout_ms = 15000
//!
//! [[step]]
//! action = "type"
//! text = "QSATEST"
//!
//! [[step]]
//! action = "key"
//! key = "FieldExit"
//! ```
//!
//! `key` names a 5250 function as keyboard maps do; `row` and `col`
//! count from 1; `prompt` asks for a value when the macro runs, and
//! recording puts one in place of anything typed into a nondisplay
//! field, so a password never reaches the file. The web editor shows the
//! steps one a line ([`MacroFile::to_lines`], [`MacroFile::from_lines`]).

use serde::{Deserialize, Serialize};

use super::doc::{self, Extra, FileError, Header};
use super::keymap;
use super::qsd::ScreenSize;

/// File extension, without the dot.
pub const EXTENSION: &str = "qsm";
/// Longest wait a step may give, in milliseconds (ten minutes).
pub const MAX_WAIT_MS: u32 = 600_000;
/// The wait a new step gets, in milliseconds.
pub const DEFAULT_TIMEOUT_MS: u32 = 15_000;

/// What a step does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Type text at the cursor.
    Type {
        /// The text.
        text: String,
    },
    /// Press a key or attention key, by 5250 function name.
    Key {
        /// The function.
        key: String,
    },
    /// Wait until the keyboard unlocks.
    WaitUnlock {
        /// How long, at most.
        timeout_ms: u32,
    },
    /// Wait until `text` appears at `row`, `col` (from 1).
    WaitText {
        /// Row.
        row: u8,
        /// Column.
        col: u8,
        /// The text.
        text: String,
        /// How long, at most.
        timeout_ms: u32,
    },
    /// Wait a fixed time.
    Pause {
        /// How long.
        ms: u32,
    },
    /// Ask for a value when the macro runs, and type it. `hidden` asks
    /// without echoing (for passwords).
    Prompt {
        /// What to ask for.
        label: String,
        /// Ask without echoing.
        hidden: bool,
    },
}

impl Action {
    /// The name written in the file.
    pub fn name(&self) -> &'static str {
        match self {
            Action::Type { .. } => "type",
            Action::Key { .. } => "key",
            Action::WaitUnlock { .. } => "wait-unlock",
            Action::WaitText { .. } => "wait-text",
            Action::Pause { .. } => "pause",
            Action::Prompt { .. } => "prompt",
        }
    }

    /// A one-line description, as the Macro editor lists it.
    pub fn describe(&self) -> String {
        let secs = |ms: &u32| {
            if ms.is_multiple_of(1000) {
                format!("{} s", ms / 1000)
            } else {
                format!("{ms} ms")
            }
        };
        match self {
            Action::Type { text } => format!("Type \"{text}\""),
            Action::Key { key } => format!("Press {}", keymap::function_label(key)),
            Action::WaitUnlock { timeout_ms } => {
                format!("Wait for keyboard (up to {})", secs(timeout_ms))
            }
            Action::WaitText {
                row,
                col,
                text,
                timeout_ms,
            } => format!(
                "Wait for \"{text}\" at {row},{col} (up to {})",
                secs(timeout_ms)
            ),
            Action::Pause { ms } => format!("Pause {}", secs(ms)),
            Action::Prompt { label, hidden } => {
                if *hidden {
                    format!("Ask for {label} (hidden)")
                } else {
                    format!("Ask for {label}")
                }
            }
        }
    }
}

/// One step, and any keys in it this release does not know.
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    /// What it does.
    pub action: Action,
    /// Keys this release does not know (`comment`, ...).
    pub extra: Extra,
}

impl Step {
    /// A step doing `action`.
    pub fn new(action: Action) -> Step {
        Step {
            action,
            extra: Extra::new(),
        }
    }
}

impl From<Action> for Step {
    fn from(a: Action) -> Step {
        Step::new(a)
    }
}

/// A step as written: every field optional, checked by [`StepDoc::into_step`].
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct StepDoc {
    action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    row: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    col: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hidden: Option<bool>,
    #[serde(flatten)]
    extra: Extra,
}

impl StepDoc {
    fn from_step(s: &Step) -> StepDoc {
        let mut d = StepDoc {
            action: s.action.name().into(),
            extra: s.extra.clone(),
            ..StepDoc::default()
        };
        match &s.action {
            Action::Type { text } => d.text = Some(text.clone()),
            Action::Key { key } => d.key = Some(key.clone()),
            Action::WaitUnlock { timeout_ms } => d.timeout_ms = Some((*timeout_ms).into()),
            Action::WaitText {
                row,
                col,
                text,
                timeout_ms,
            } => {
                d.row = Some((*row).into());
                d.col = Some((*col).into());
                d.text = Some(text.clone());
                d.timeout_ms = Some((*timeout_ms).into());
            }
            Action::Pause { ms } => d.ms = Some((*ms).into()),
            Action::Prompt { label, hidden } => {
                d.label = Some(label.clone());
                d.hidden = Some(*hidden);
            }
        }
        d
    }

    fn into_step(self) -> Result<Step, String> {
        fn need<T>(v: Option<T>, what: &str) -> Result<T, String> {
            v.ok_or_else(|| format!("{what} is missing"))
        }
        fn wait(v: Option<i64>, what: &str) -> Result<u32, String> {
            let v = need(v, what)?;
            u32::try_from(v)
                .ok()
                .filter(|v| (1..=MAX_WAIT_MS).contains(v))
                .ok_or_else(|| format!("{what} {v} is not between 1 and {MAX_WAIT_MS}"))
        }
        let action = match self.action.as_str() {
            "type" => Action::Type {
                text: need(self.text, "text")?,
            },
            "key" => {
                let key = need(self.key, "key")?;
                if !keymap::is_function(&key) {
                    return Err(format!("key \"{key}\" is not a 5250 function"));
                }
                Action::Key { key }
            }
            "wait-unlock" => Action::WaitUnlock {
                timeout_ms: wait(self.timeout_ms, "timeout_ms")?,
            },
            "wait-text" => {
                let pos = |v: Option<i64>, what: &str, max: i64| -> Result<u8, String> {
                    let v = need(v, what)?;
                    if (1..=max).contains(&v) {
                        Ok(u8::try_from(v).unwrap_or(u8::MAX))
                    } else {
                        Err(format!("{what} {v} is not between 1 and {max}"))
                    }
                };
                let text = need(self.text, "text")?;
                if text.is_empty() {
                    return Err("text is empty".into());
                }
                Action::WaitText {
                    row: pos(self.row, "row", 27)?,
                    col: pos(self.col, "col", 132)?,
                    text,
                    timeout_ms: wait(self.timeout_ms, "timeout_ms")?,
                }
            }
            "pause" => Action::Pause {
                ms: wait(self.ms, "ms")?,
            },
            "prompt" => Action::Prompt {
                label: self.label.unwrap_or_else(|| "a value".into()),
                hidden: self.hidden.unwrap_or(true),
            },
            other => return Err(format!("action \"{other}\" is not known")),
        };
        Ok(Step {
            action,
            extra: self.extra,
        })
    }
}

/// `[macro]`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Info {
    /// The name.
    pub name: String,
    /// What it does.
    pub description: String,
    /// The screen size it was recorded at; a run at another size stops.
    pub screen_size: ScreenSize,
    #[serde(flatten)]
    /// Keys this release does not know.
    pub extra: Extra,
}

impl Default for Info {
    fn default() -> Info {
        Info {
            name: String::new(),
            description: String::new(),
            screen_size: ScreenSize::S24x80,
            extra: Extra::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct MacroDoc {
    qsaccess: Header,
    #[serde(default, rename = "macro")]
    info: Info,
    #[serde(default, rename = "step")]
    steps: Vec<StepDoc>,
    #[serde(flatten)]
    extra: Extra,
}

/// A macro file.
#[derive(Clone, Debug, PartialEq)]
pub struct MacroFile {
    /// The header.
    pub header: Header,
    /// `[macro]`.
    pub info: Info,
    /// The steps.
    pub steps: Vec<Step>,
    /// Tables this release does not know.
    pub extra: Extra,
}

impl MacroFile {
    /// An empty macro.
    pub fn new(name: &str, screen_size: ScreenSize) -> MacroFile {
        MacroFile {
            header: Header::new(doc::MACRO),
            info: Info {
                name: name.to_owned(),
                screen_size,
                ..Info::default()
            },
            steps: Vec::new(),
            extra: Extra::new(),
        }
    }

    /// The name to show: `[macro] name`, else the file's name.
    pub fn display_name(&self) -> String {
        let n = self.info.name.trim();
        if n.is_empty() {
            "Macro".into()
        } else {
            n.to_owned()
        }
    }

    /// Checks a step as the editor would save it.
    pub fn check_step(step: &Step, size: ScreenSize) -> Result<(), String> {
        let s = StepDoc::from_step(step).into_step()?;
        if let Action::WaitText { row, col, .. } = s.action {
            let (rows, cols) = size.rows_cols();
            if usize::from(row) > rows || usize::from(col) > cols {
                return Err(format!(
                    "position {row},{col} is outside a {rows} x {cols} screen"
                ));
            }
        }
        Ok(())
    }

    /// Parses `.qsm` text.
    pub fn parse(text: &str) -> Result<MacroFile, FileError> {
        let d: MacroDoc = doc::parse(text, doc::MACRO)?;
        let mut steps = Vec::with_capacity(d.steps.len());
        for (i, s) in d.steps.into_iter().enumerate() {
            let step = s
                .into_step()
                .map_err(|e| FileError::Invalid(format!("step {}: {e}", i + 1)))?;
            MacroFile::check_step(&step, d.info.screen_size)
                .map_err(|e| FileError::Invalid(format!("step {}: {e}", i + 1)))?;
            steps.push(step);
        }
        Ok(MacroFile {
            header: d.qsaccess,
            info: d.info,
            steps,
            extra: d.extra,
        })
    }

    /// The `.qsm` text.
    pub fn to_text(&self) -> String {
        let mut header = self.header.clone();
        header.kind = doc::MACRO.into();
        header.version = doc::VERSION;
        doc::to_text(&MacroDoc {
            qsaccess: header,
            info: self.info.clone(),
            steps: self.steps.iter().map(StepDoc::from_step).collect(),
            extra: self.extra.clone(),
        })
    }

    /// What the macro waits for, as the Macros list says it.
    pub fn waits(&self) -> String {
        let mut out = Vec::new();
        let mut texts = 0;
        for s in &self.steps {
            match &s.action {
                Action::WaitText { row, col, text, .. } if text.trim() == "===>" => {
                    out.push(format!("Command line at {row}/{col}"))
                }
                Action::WaitText { row, col, text, .. } => {
                    texts += 1;
                    let lead = if texts == 1 { "Screen text " } else { "" };
                    out.push(format!("{lead}'{text}' at {row}/{col}"));
                }
                Action::WaitUnlock { .. } => out.push("Keyboard".into()),
                _ => {}
            }
        }
        if out.is_empty() {
            "Nothing".into()
        } else {
            out.join("; ")
        }
    }

    /// The steps one a line, as the web editor shows them.
    pub fn to_lines(&self) -> String {
        let mut out = String::new();
        for s in &self.steps {
            let q = |t: &str| format!("\"{}\"", t.replace('\\', "\\\\").replace('"', "\\\""));
            let within = |ms: u32| {
                if ms == DEFAULT_TIMEOUT_MS {
                    String::new()
                } else {
                    format!(" for {ms} ms")
                }
            };
            let mut line = match &s.action {
                Action::Type { text } => format!("type  {}", q(text)),
                Action::Key { key } => format!("key   {key}"),
                Action::WaitUnlock { timeout_ms } => format!("wait  unlock{}", within(*timeout_ms)),
                Action::WaitText {
                    row,
                    col,
                    text,
                    timeout_ms,
                } => format!(
                    "wait  screen {} at {row},{col}{}",
                    q(text),
                    within(*timeout_ms)
                ),
                Action::Pause { ms } => format!("pause {ms}"),
                Action::Prompt {
                    label,
                    hidden: true,
                } => format!("prompt {}", q(label)),
                Action::Prompt {
                    label,
                    hidden: false,
                } => format!("prompt shown {}", q(label)),
            };
            if let Some(c) = s.extra.get("comment").and_then(toml::Value::as_str) {
                line.push_str("   ; ");
                line.push_str(c);
            }
            out.push_str(&line);
            out.push('\n');
        }
        out
    }

    /// Steps from the editor's lines; a `;` starts a comment, kept with
    /// its step. The error names the line.
    pub fn from_lines(text: &str, size: ScreenSize) -> Result<Vec<Step>, String> {
        let mut steps = Vec::new();
        for (n, raw) in text.lines().enumerate() {
            let at = |e: String| format!("line {}: {e}", n + 1);
            let (body, comment) = split_comment(raw);
            let body = body.trim();
            if body.is_empty() {
                continue;
            }
            let (verb, rest) = body.split_once(char::is_whitespace).unwrap_or((body, ""));
            let rest = rest.trim();
            let action = match verb {
                "type" => Action::Type {
                    text: quoted(rest).map_err(at)?.0,
                },
                "key" => Action::Key {
                    key: rest.to_string(),
                },
                "pause" => Action::Pause {
                    ms: rest
                        .parse()
                        .map_err(|_| at(format!("pause needs milliseconds, not \"{rest}\"")))?,
                },
                "prompt" => {
                    let (hidden, rest) = match rest.strip_prefix("shown") {
                        Some(r) => (false, r.trim()),
                        None => (true, rest),
                    };
                    Action::Prompt {
                        label: quoted(rest).map_err(at)?.0,
                        hidden,
                    }
                }
                "wait" => {
                    let (what, rest) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
                    let (timeout_ms, rest) = within(rest.trim()).map_err(at)?;
                    match what {
                        "unlock" if rest.is_empty() => Action::WaitUnlock { timeout_ms },
                        "screen" => {
                            let (text, after) = quoted(rest).map_err(at)?;
                            let pos = after
                                .trim()
                                .strip_prefix("at")
                                .map(str::trim)
                                .ok_or_else(|| at("wait screen needs \"at ROW,COL\"".into()))?;
                            let (r, c) = pos
                                .split_once(',')
                                .ok_or_else(|| at(format!("\"{pos}\" is not ROW,COL")))?;
                            let num = |v: &str| {
                                v.trim()
                                    .parse::<u8>()
                                    .map_err(|_| at(format!("\"{pos}\" is not ROW,COL")))
                            };
                            Action::WaitText {
                                row: num(r)?,
                                col: num(c)?,
                                text,
                                timeout_ms,
                            }
                        }
                        _ => return Err(at(format!("wait \"{what}\" is not unlock or screen"))),
                    }
                }
                other => {
                    return Err(at(format!(
                        "\"{other}\" is not type, key, wait, pause or prompt"
                    )));
                }
            };
            let mut step = Step::new(action);
            if let Some(c) = comment {
                step.extra
                    .insert("comment".into(), toml::Value::String(c.to_string()));
            }
            MacroFile::check_step(&step, size).map_err(at)?;
            steps.push(step);
        }
        Ok(steps)
    }
}

/// A line's body and its comment: a `;` outside quotes.
fn split_comment(line: &str) -> (&str, Option<&str>) {
    let mut quoted = false;
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        match c {
            '\\' if quoted && !escaped => {
                escaped = true;
                continue;
            }
            '"' if !escaped => quoted = !quoted,
            ';' if !quoted => return (&line[..i], Some(line[i + 1..].trim())),
            _ => {}
        }
        escaped = false;
    }
    (line, None)
}

/// A `"..."` at the start of `s`, and what follows it.
fn quoted(s: &str) -> Result<(String, &str), String> {
    let body = s
        .strip_prefix('"')
        .ok_or_else(|| format!("{s} is not in quotes"))?;
    let mut out = String::new();
    let mut chars = body.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => {
                if let Some((_, n)) = chars.next() {
                    out.push(n);
                }
            }
            '"' => return Ok((out, &body[i + 1..])),
            _ => out.push(c),
        }
    }
    Err(format!("{s} has no closing quote"))
}

/// A trailing `for N ms` (or `for N s`), else the default wait.
fn within(s: &str) -> Result<(u32, &str), String> {
    let Some(i) = s
        .rfind(" for ")
        .or_else(|| s.starts_with("for ").then_some(0))
    else {
        return Ok((DEFAULT_TIMEOUT_MS, s));
    };
    let tail = s[i..].trim().trim_start_matches("for").trim();
    let (n, unit) = tail.split_once(' ').unwrap_or((tail, "ms"));
    let n: u32 = n.parse().map_err(|_| format!("\"{tail}\" is not a time"))?;
    let ms = match unit.trim() {
        "ms" => n,
        "s" => n.saturating_mul(1000),
        u => return Err(format!("\"{u}\" is not ms or s")),
    };
    Ok((ms, s[..i].trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_desktops_files_read_and_round_trip() {
        for text in [
            include_str!("testdata/qs400-signon-24x80.qsm"),
            include_str!("testdata/sign-on-wrkactjob-f3.qsm"),
        ] {
            let m = MacroFile::parse(text).unwrap();
            assert!(!m.steps.is_empty());
            assert_eq!(MacroFile::parse(&m.to_text()).unwrap(), m);
            let steps = MacroFile::from_lines(&m.to_lines(), m.info.screen_size).unwrap();
            assert_eq!(steps, m.steps, "{}", m.to_lines());
        }
    }

    #[test]
    fn editor_lines() {
        let text = "wait  screen \"Sign On\" at 1,35\ntype  \"CWELLMAN\"\nkey   Tab\nkey   Enter\nwait  screen \"Main Menu\" at 1,36\nkey   F:4   ; pick a screen position with the mouse in the editor\n";
        let steps = MacroFile::from_lines(text, ScreenSize::S24x80).unwrap();
        let mut m = MacroFile::new("SIGNON-DEV", ScreenSize::S24x80);
        m.steps = steps;
        assert_eq!(m.to_lines(), text);
        assert_eq!(
            m.waits(),
            "Screen text 'Sign On' at 1/35; 'Main Menu' at 1/36"
        );
        assert_eq!(MacroFile::parse(&m.to_text()).unwrap(), m);
        let bad = MacroFile::from_lines("key   Warp\n", ScreenSize::S24x80).unwrap_err();
        assert!(bad.starts_with("line 1:") && bad.contains("Warp"), "{bad}");
        assert!(
            MacroFile::from_lines("wait  screen \"x\" at 30,1\n", ScreenSize::S24x80)
                .unwrap_err()
                .contains("row 30")
        );
        let s = MacroFile::from_lines(
            "wait  unlock for 5 s\npause 200\nprompt shown \"Library\"\n",
            ScreenSize::S24x80,
        )
        .unwrap();
        assert_eq!(s[0].action, Action::WaitUnlock { timeout_ms: 5000 });
    }
}
