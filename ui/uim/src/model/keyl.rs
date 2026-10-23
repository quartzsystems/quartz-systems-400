// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Key lists and the action set shared by keys, menu items and list
//! actions.

/// A `:KEYL.`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyList {
    /// Name.
    pub name: String,
    /// `HELP`: the help module for the key area.
    pub help: Option<String>,
    /// The keys in order.
    pub keys: Vec<KeyItem>,
    /// Source line.
    pub line: u32,
}

/// A `:KEYI.`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyItem {
    /// The key.
    pub key: Key,
    /// `HELP`.
    pub help: Option<String>,
    /// `ACTION`.
    pub action: Action,
    /// `VARUPD`: whether input fields are read before the action.
    pub varupd: Option<super::VarUpd>,
    /// `COND`: the key shows only when this condition holds.
    pub cond: Option<String>,
    /// The text shown in the key area, `F3=Exit`.
    pub text: String,
    /// Source line.
    pub line: u32,
}

/// A key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// `ENTER`.
    Enter,
    /// `HELP`.
    Help,
    /// `PRINT`.
    Print,
    /// `HOME`.
    Home,
    /// `PAGEUP` (`ROLLDOWN`).
    PageUp,
    /// `PAGEDOWN` (`ROLLUP`).
    PageDown,
    /// `F1` to `F24`.
    F(u8),
}

impl Key {
    /// Parse `F3`, `ENTER`, ...
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let t = text.to_ascii_uppercase();
        Some(match t.as_str() {
            "ENTER" => Self::Enter,
            "HELP" => Self::Help,
            "PRINT" => Self::Print,
            "HOME" => Self::Home,
            "PAGEUP" | "ROLLDOWN" => Self::PageUp,
            "PAGEDOWN" | "ROLLUP" => Self::PageDown,
            _ => {
                let n: u8 = t.strip_prefix('F')?.parse().ok()?;
                if (1..=24).contains(&n) {
                    Self::F(n)
                } else {
                    return None;
                }
            }
        })
    }

    /// The keyword.
    #[must_use]
    pub fn text(self) -> String {
        match self {
            Self::Enter => "ENTER".into(),
            Self::Help => "HELP".into(),
            Self::Print => "PRINT".into(),
            Self::Home => "HOME".into(),
            Self::PageUp => "PAGEUP".into(),
            Self::PageDown => "PAGEDOWN".into(),
            Self::F(n) => format!("F{n}"),
        }
    }
}

/// What a key, menu option or list action does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// `RETURN n`: return the number to the program.
    Return(u32),
    /// `ENTER`: as the panel's `ENTER`.
    Enter,
    /// `HELP`: help for the cursor's location.
    Help,
    /// `EXTHELP`: the panel's extended help.
    ExtHelp,
    /// `KEYSHELP`: help on the keys.
    KeysHelp,
    /// `ABOUT`.
    About,
    /// `EXIT`.
    Exit,
    /// `CANCEL`.
    Cancel,
    /// `PRINT`.
    Print,
    /// `CMDLINE`: move the cursor to the command line.
    CmdLine,
    /// `PRVPNL`.
    PrvPnl,
    /// `NXTPNL`.
    NxtPnl,
    /// `PAGEUP`.
    PageUp,
    /// `PAGEDOWN`.
    PageDown,
    /// `PULLDOWN`.
    PullDown,
    /// `ACTIONS`: the action bar.
    Actions,
    /// `RETRIEVE`: the previous command.
    Retrieve,
    /// `MOREKEYS`: the next set of keys.
    MoreKeys,
    /// `FLDHELP`: help for the field.
    FldHelp,
    /// `TOP`.
    Top,
    /// `BOTTOM`.
    Bottom,
    /// `HOME`.
    Home,
    /// `PRVPAGE`.
    PrvPage,
    /// `NXTPAGE`.
    NxtPage,
    /// `CALL name`: call the program the variable names.
    Call(String),
    /// `PANEL name`: show another panel of the group.
    Panel(String),
    /// `MENU name`: show a menu object.
    Menu(String),
    /// `MSG name`: send the message the variable names.
    Msg(String),
    /// `HELP name`: show one help module.
    HelpModule(String),
    /// `CMD string`: run the command string, as a `*MENU` option does;
    /// a leading `?` prompts it. The string keeps its case.
    Cmd(String),
}

impl Action {
    /// Parse `RETURN 3`, `CALL pgm`, `EXIT`, ...
    pub fn parse(text: &str) -> Result<Self, String> {
        let raw = text.trim();
        let t = raw.to_ascii_uppercase();
        let mut parts = t.split_whitespace();
        let word = parts.next().ok_or_else(|| "empty".to_string())?;
        if word == "CMD" {
            let rest = raw[3..].trim();
            if rest.is_empty() {
                return Err("CMD needs a command string".to_string());
            }
            return Ok(Self::Cmd(rest.to_string()));
        }
        let arg = parts.next();
        if parts.next().is_some() {
            return Err("more than two words".to_string());
        }
        let need = |what: &str| -> Result<String, String> {
            arg.map(str::to_string)
                .ok_or_else(|| format!("{word} needs a {what}"))
        };
        let none = |a: Self| -> Result<Self, String> {
            match arg {
                None => Ok(a),
                Some(x) => Err(format!("{word} takes no operand; found {x}")),
            }
        };
        match word {
            "RETURN" => {
                let n = need("number")?;
                n.parse()
                    .map(Self::Return)
                    .map_err(|_| format!("{n} is not a number"))
            }
            "ENTER" => none(Self::Enter),
            "HELP" => match arg {
                None => Ok(Self::Help),
                Some(h) => Ok(Self::HelpModule(h.to_string())),
            },
            "EXTHELP" => none(Self::ExtHelp),
            "KEYSHELP" => none(Self::KeysHelp),
            "ABOUT" => none(Self::About),
            "EXIT" => none(Self::Exit),
            "CANCEL" => none(Self::Cancel),
            "PRINT" => none(Self::Print),
            "CMDLINE" => none(Self::CmdLine),
            "PRVPNL" => none(Self::PrvPnl),
            "NXTPNL" => none(Self::NxtPnl),
            "PAGEUP" => none(Self::PageUp),
            "PAGEDOWN" => none(Self::PageDown),
            "PULLDOWN" => none(Self::PullDown),
            "ACTIONS" => none(Self::Actions),
            "RETRIEVE" => none(Self::Retrieve),
            "MOREKEYS" => none(Self::MoreKeys),
            "FLDHELP" => none(Self::FldHelp),
            "TOP" => none(Self::Top),
            "BOTTOM" => none(Self::Bottom),
            "HOME" => none(Self::Home),
            "PRVPAGE" => none(Self::PrvPage),
            "NXTPAGE" => none(Self::NxtPage),
            "CALL" => need("variable").map(Self::Call),
            "PANEL" => need("panel").map(Self::Panel),
            "MENU" => need("menu").map(Self::Menu),
            "MSG" => need("variable").map(Self::Msg),
            other => Err(format!("{other} is not an action")),
        }
    }

    /// The action as written.
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            Self::Return(n) => format!("RETURN {n}"),
            Self::Enter => "ENTER".into(),
            Self::Help => "HELP".into(),
            Self::ExtHelp => "EXTHELP".into(),
            Self::KeysHelp => "KEYSHELP".into(),
            Self::About => "ABOUT".into(),
            Self::Exit => "EXIT".into(),
            Self::Cancel => "CANCEL".into(),
            Self::Print => "PRINT".into(),
            Self::CmdLine => "CMDLINE".into(),
            Self::PrvPnl => "PRVPNL".into(),
            Self::NxtPnl => "NXTPNL".into(),
            Self::PageUp => "PAGEUP".into(),
            Self::PageDown => "PAGEDOWN".into(),
            Self::PullDown => "PULLDOWN".into(),
            Self::Actions => "ACTIONS".into(),
            Self::Retrieve => "RETRIEVE".into(),
            Self::MoreKeys => "MOREKEYS".into(),
            Self::FldHelp => "FLDHELP".into(),
            Self::Top => "TOP".into(),
            Self::Bottom => "BOTTOM".into(),
            Self::Home => "HOME".into(),
            Self::PrvPage => "PRVPAGE".into(),
            Self::NxtPage => "NXTPAGE".into(),
            Self::Call(v) => format!("CALL {v}"),
            Self::Panel(p) => format!("PANEL {p}"),
            Self::Menu(m) => format!("MENU {m}"),
            Self::Msg(v) => format!("MSG {v}"),
            Self::HelpModule(h) => format!("HELP {h}"),
            Self::Cmd(s) => format!("CMD {s}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_and_keys_parse() {
        assert_eq!(Action::parse("return 3"), Ok(Action::Return(3)));
        assert_eq!(Action::parse("CALL pgm"), Ok(Action::Call("PGM".into())));
        assert_eq!(Action::parse("HELP"), Ok(Action::Help));
        assert_eq!(Action::parse("HELP x"), Ok(Action::HelpModule("X".into())));
        assert!(Action::parse("EXIT now").is_err());
        assert!(Action::parse("RETURN x").is_err());
        assert!(Action::parse("JUMP").is_err());
        assert_eq!(
            Action::parse("cmd ?AddJobScde"),
            Ok(Action::Cmd("?AddJobScde".into()))
        );
        assert!(Action::parse("CMD").is_err());
        assert_eq!(Key::parse("f12"), Some(Key::F(12)));
        assert_eq!(Key::parse("F25"), None);
        assert_eq!(Key::parse("rollup"), Some(Key::PageDown));
        assert_eq!(Action::Return(7).text(), "RETURN 7");
    }
}
