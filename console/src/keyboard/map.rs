// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Linux key codes to 5250 keys. One table, US layout (QKBDTYPE USB).

use crate::keyboard::KeyEvent;

/// A 5250 key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// A printable character.
    Char(char),
    /// Enter.
    Enter,
    /// Field Advance (Tab).
    FieldAdvance,
    /// Field Backspace (Shift+Tab).
    FieldBackspace,
    /// Field Exit (keypad plus, right Shift).
    FieldExit,
    /// Backspace within a field.
    Backspace,
    /// Home.
    Home,
    /// Insert toggle.
    Insert,
    /// Delete.
    Delete,
    /// F1 to F24.
    Function(u8),
    /// Attention (Esc). No-op in Phase 1.
    Attention,
    /// Reset (Ctrl+Left).
    Reset,
    /// Cursor left.
    Left,
    /// Cursor right.
    Right,
    /// Cursor up.
    Up,
    /// Cursor down.
    Down,
}

/// Modifier state tracked from press and release events.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    /// Either Shift held. Right Shift is also Field Exit on press.
    pub shift: bool,
    /// Either Ctrl held.
    pub ctrl: bool,
    /// Caps Lock on.
    pub caps: bool,
}

const KEY_ESC: u16 = 1;
const KEY_BACKSPACE: u16 = 14;
const KEY_TAB: u16 = 15;
const KEY_ENTER: u16 = 28;
const KEY_LEFTCTRL: u16 = 29;
const KEY_LEFTSHIFT: u16 = 42;
const KEY_RIGHTSHIFT: u16 = 54;
const KEY_CAPSLOCK: u16 = 58;
const KEY_F1: u16 = 59;
const KEY_F10: u16 = 68;
const KEY_KPPLUS: u16 = 78;
const KEY_F11: u16 = 87;
const KEY_F12: u16 = 88;
const KEY_KPENTER: u16 = 96;
const KEY_RIGHTCTRL: u16 = 97;
const KEY_HOME: u16 = 102;
const KEY_UP: u16 = 103;
const KEY_LEFT: u16 = 105;
const KEY_RIGHT: u16 = 106;
const KEY_DOWN: u16 = 108;
const KEY_INSERT: u16 = 110;
const KEY_DELETE: u16 = 111;

/// Unshifted and shifted characters for codes 2 (`1`) through 53 (`/`).
const CHARS: [(char, char); 52] = [
    ('1', '!'),
    ('2', '@'),
    ('3', '#'),
    ('4', '$'),
    ('5', '%'),
    ('6', '^'),
    ('7', '&'),
    ('8', '*'),
    ('9', '('),
    ('0', ')'),
    ('-', '_'),
    ('=', '+'),
    ('\0', '\0'),
    ('\0', '\0'),
    ('q', 'Q'),
    ('w', 'W'),
    ('e', 'E'),
    ('r', 'R'),
    ('t', 'T'),
    ('y', 'Y'),
    ('u', 'U'),
    ('i', 'I'),
    ('o', 'O'),
    ('p', 'P'),
    ('[', '{'),
    (']', '}'),
    ('\0', '\0'),
    ('\0', '\0'),
    ('a', 'A'),
    ('s', 'S'),
    ('d', 'D'),
    ('f', 'F'),
    ('g', 'G'),
    ('h', 'H'),
    ('j', 'J'),
    ('k', 'K'),
    ('l', 'L'),
    (';', ':'),
    ('\'', '"'),
    ('`', '~'),
    ('\0', '\0'),
    ('\\', '|'),
    ('z', 'Z'),
    ('x', 'X'),
    ('c', 'C'),
    ('v', 'V'),
    ('b', 'B'),
    ('n', 'N'),
    ('m', 'M'),
    (',', '<'),
    ('.', '>'),
    ('/', '?'),
];
const KEY_SPACE: u16 = 57;
const KEY_KPASTERISK: u16 = 55;
const KEY_KPMINUS: u16 = 74;
const KEY_KPSLASH: u16 = 98;
const KEY_KPDOT: u16 = 83;

/// Update modifiers from an event. Returns true when the event was only a
/// modifier change (nothing further to do).
pub fn update_modifiers(ev: KeyEvent, mods: &mut Modifiers) -> bool {
    match ev.code {
        KEY_LEFTSHIFT => {
            mods.shift = ev.pressed;
            true
        }
        KEY_RIGHTSHIFT => {
            // Right Shift is Field Exit; it still shifts while held.
            mods.shift = ev.pressed;
            !ev.pressed
        }
        KEY_LEFTCTRL => {
            mods.ctrl = ev.pressed;
            true
        }
        KEY_RIGHTCTRL => !ev.pressed, // Right Ctrl is Enter on press
        KEY_CAPSLOCK => {
            if ev.pressed {
                mods.caps = !mods.caps;
            }
            true
        }
        _ => false,
    }
}

/// The 5250 key for a press, if any.
pub fn translate(code: u16, mods: Modifiers) -> Option<Key> {
    Some(match code {
        KEY_ENTER | KEY_KPENTER | KEY_RIGHTCTRL => Key::Enter,
        KEY_TAB if mods.shift => Key::FieldBackspace,
        KEY_TAB => Key::FieldAdvance,
        KEY_KPPLUS | KEY_RIGHTSHIFT => Key::FieldExit,
        KEY_BACKSPACE => Key::Backspace,
        KEY_HOME => Key::Home,
        KEY_INSERT => Key::Insert,
        KEY_DELETE => Key::Delete,
        KEY_ESC => Key::Attention,
        KEY_LEFT if mods.ctrl => Key::Reset,
        KEY_LEFT => Key::Left,
        KEY_RIGHT => Key::Right,
        KEY_UP => Key::Up,
        KEY_DOWN => Key::Down,
        KEY_F1..=KEY_F10 => function(code - KEY_F1 + 1, mods),
        KEY_F11 => function(11, mods),
        KEY_F12 => function(12, mods),
        KEY_SPACE => Key::Char(' '),
        KEY_KPASTERISK => Key::Char('*'),
        KEY_KPMINUS => Key::Char('-'),
        KEY_KPSLASH => Key::Char('/'),
        KEY_KPDOT => Key::Char('.'),
        71..=73 | 75..=77 | 79..=82 => Key::Char(keypad_digit(code)),
        2..=53 => {
            let (plain, shifted) = CHARS[usize::from(code - 2)];
            let c = if mods.shift { shifted } else { plain };
            if c == '\0' {
                return None;
            }
            let c = if mods.caps && c.is_ascii_alphabetic() {
                if mods.shift {
                    c.to_ascii_lowercase()
                } else {
                    c.to_ascii_uppercase()
                }
            } else {
                c
            };
            Key::Char(c)
        }
        _ => return None,
    })
}

fn function(n: u16, mods: Modifiers) -> Key {
    let n = u8::try_from(n).unwrap_or(1);
    Key::Function(if mods.shift { n + 12 } else { n })
}

fn keypad_digit(code: u16) -> char {
    match code {
        71 => '7',
        72 => '8',
        73 => '9',
        75 => '4',
        76 => '5',
        77 => '6',
        79 => '1',
        80 => '2',
        81 => '3',
        _ => '0',
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_digits_and_shift() {
        let plain = Modifiers::default();
        let shift = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        let caps = Modifiers {
            caps: true,
            ..Modifiers::default()
        };
        assert_eq!(translate(30, plain), Some(Key::Char('a')));
        assert_eq!(translate(30, shift), Some(Key::Char('A')));
        assert_eq!(translate(30, caps), Some(Key::Char('A')));
        assert_eq!(translate(2, plain), Some(Key::Char('1')));
        assert_eq!(translate(2, shift), Some(Key::Char('!')));
        assert_eq!(translate(57, plain), Some(Key::Char(' ')));
        assert_eq!(translate(79, plain), Some(Key::Char('1')));
    }

    #[test]
    fn special_keys() {
        let plain = Modifiers::default();
        let shift = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::default()
        };
        assert_eq!(translate(KEY_ENTER, plain), Some(Key::Enter));
        assert_eq!(translate(KEY_RIGHTCTRL, plain), Some(Key::Enter));
        assert_eq!(translate(KEY_TAB, plain), Some(Key::FieldAdvance));
        assert_eq!(translate(KEY_TAB, shift), Some(Key::FieldBackspace));
        assert_eq!(translate(KEY_KPPLUS, plain), Some(Key::FieldExit));
        assert_eq!(translate(KEY_RIGHTSHIFT, plain), Some(Key::FieldExit));
        assert_eq!(translate(KEY_F1, plain), Some(Key::Function(1)));
        assert_eq!(translate(KEY_F1, shift), Some(Key::Function(13)));
        assert_eq!(translate(KEY_F12, shift), Some(Key::Function(24)));
        assert_eq!(translate(KEY_ESC, plain), Some(Key::Attention));
        assert_eq!(translate(KEY_LEFT, ctrl), Some(Key::Reset));
        assert_eq!(translate(KEY_LEFT, plain), Some(Key::Left));
        assert_eq!(translate(200, plain), None);
    }

    #[test]
    fn modifier_tracking() {
        let mut m = Modifiers::default();
        assert!(update_modifiers(
            KeyEvent {
                code: KEY_LEFTSHIFT,
                pressed: true
            },
            &mut m
        ));
        assert!(m.shift);
        assert!(update_modifiers(
            KeyEvent {
                code: KEY_LEFTSHIFT,
                pressed: false
            },
            &mut m
        ));
        assert!(!m.shift);
        // Right Shift press is not swallowed: it is Field Exit.
        assert!(!update_modifiers(
            KeyEvent {
                code: KEY_RIGHTSHIFT,
                pressed: true
            },
            &mut m
        ));
        assert!(update_modifiers(
            KeyEvent {
                code: KEY_CAPSLOCK,
                pressed: true
            },
            &mut m
        ));
        assert!(m.caps);
        assert!(!update_modifiers(
            KeyEvent {
                code: 30,
                pressed: true
            },
            &mut m
        ));
    }
}
