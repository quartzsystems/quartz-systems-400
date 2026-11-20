// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Keys by name: what a key message carries, the keypad a page posts
//! without script, and the console's keys (`qsipart::console::Key`).

use qsipart::console::Key;

/// The keypad: the label a page posts (`k=`) and the key it names.
pub const KEYPAD: &[(&str, &str)] = &[
    ("F1", "F:1"),
    ("F2", "F:2"),
    ("F3", "F:3"),
    ("F4", "F:4"),
    ("F5", "F:5"),
    ("F6", "F:6"),
    ("F7", "F:7"),
    ("F8", "F:8"),
    ("F9", "F:9"),
    ("F10", "F:10"),
    ("F11", "F:11"),
    ("F12", "F:12"),
    ("F13", "F:13"),
    ("F14", "F:14"),
    ("F15", "F:15"),
    ("F16", "F:16"),
    ("F17", "F:17"),
    ("F18", "F:18"),
    ("F19", "F:19"),
    ("F20", "F:20"),
    ("F21", "F:21"),
    ("F22", "F:22"),
    ("F23", "F:23"),
    ("F24", "F:24"),
    ("Enter", "Enter"),
    ("Reset", "Reset"),
    ("Attn", "Attention"),
    ("SysRq", "SysReq"),
    ("PgUp", "PageUp"),
    ("PgDn", "PageDown"),
    ("FldExit", "FieldExit"),
    ("Field+", "FieldPlus"),
    ("Field-", "FieldMinus"),
    ("Help", "Help"),
    ("Print", "Print"),
    ("Clear", "Clear"),
];

/// Every key name but `F:1` to `F:24`: those the console has, then
/// those only a 5250 session takes.
pub const NAMES: &[&str] = &[
    "Enter",
    "Tab",
    "BackTab",
    "FieldExit",
    "Backspace",
    "Home",
    "Insert",
    "Delete",
    "Attention",
    "SysReq",
    "Reset",
    "Left",
    "Right",
    "Up",
    "Down",
    "PageUp",
    "PageDown",
    "FieldPlus",
    "FieldMinus",
    "Clear",
    "Help",
    "Print",
    "RecordBackspace",
    "NewLine",
    "Dup",
    "EraseInput",
    "EraseEof",
];

/// The key a posted keypad label names.
pub fn keypad(label: &str) -> Option<&'static str> {
    KEYPAD.iter().find(|(l, _)| *l == label).map(|(_, k)| *k)
}

/// The console key a name gives; `None` for a name only a 5250
/// session takes, or none at all.
pub fn console_key(name: &str) -> Option<Key> {
    if let Some(n) = name.strip_prefix("F:") {
        return n
            .parse()
            .ok()
            .filter(|n| (1..=24).contains(n))
            .map(Key::Function);
    }
    Some(match name {
        "Enter" => Key::Enter,
        "Tab" => Key::FieldAdvance,
        "BackTab" => Key::FieldBackspace,
        "FieldExit" => Key::FieldExit,
        "Backspace" => Key::Backspace,
        "Home" => Key::Home,
        "Insert" => Key::Insert,
        "Delete" => Key::Delete,
        "Attention" => Key::Attention,
        "SysReq" => Key::SysReq,
        "Reset" => Key::Reset,
        "Left" => Key::Left,
        "Right" => Key::Right,
        "Up" => Key::Up,
        "Down" => Key::Down,
        "PageUp" => Key::PageUp,
        "PageDown" => Key::PageDown,
        _ => return None,
    })
}

/// A console key's name; `None` for a character or a key a later
/// version has.
pub fn name(k: Key) -> Option<String> {
    Some(
        match k {
            Key::Function(n) => return Some(format!("F:{n}")),
            Key::Char(_) | Key::Other(_) => return None,
            Key::Enter => "Enter",
            Key::FieldAdvance => "Tab",
            Key::FieldBackspace => "BackTab",
            Key::FieldExit => "FieldExit",
            Key::Backspace => "Backspace",
            Key::Home => "Home",
            Key::Insert => "Insert",
            Key::Delete => "Delete",
            Key::Attention => "Attention",
            Key::SysReq => "SysReq",
            Key::Reset => "Reset",
            Key::Left => "Left",
            Key::Right => "Right",
            Key::Up => "Up",
            Key::Down => "Down",
            Key::PageUp => "PageUp",
            Key::PageDown => "PageDown",
        }
        .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_console_key_has_a_name_and_back() {
        let mut keys = vec![
            Key::Enter,
            Key::FieldAdvance,
            Key::FieldBackspace,
            Key::FieldExit,
            Key::Backspace,
            Key::Home,
            Key::Insert,
            Key::Delete,
            Key::Attention,
            Key::SysReq,
            Key::Reset,
            Key::Left,
            Key::Right,
            Key::Up,
            Key::Down,
            Key::PageUp,
            Key::PageDown,
        ];
        keys.extend((1..=24).map(Key::Function));
        for k in keys {
            let n = name(k).unwrap();
            assert!(n.starts_with("F:") || NAMES.contains(&n.as_str()), "{n}");
            assert_eq!(console_key(&n), Some(k), "{n}");
        }
        assert_eq!(console_key("Help"), None);
        assert_eq!(console_key("F:25"), None);
        assert_eq!(name(Key::Char('a')), None);
    }

    #[test]
    fn the_keypad_names_known_keys() {
        for (_, k) in KEYPAD {
            assert!(k.starts_with("F:") || NAMES.contains(k), "{k}");
        }
        assert_eq!(keypad("Attn"), Some("Attention"));
        assert_eq!(keypad("F25"), None);
    }
}
