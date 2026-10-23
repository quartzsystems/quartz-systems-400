// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The 5250 data stream: commands, orders, attention identifiers and
//! attributes as numbers (`cmd`, `order`, `aid`, `attr`), field words
//! (`field`), the outbound parser (`parse`), the outbound builder
//! (`build`), the inbound record (`inbound`) and structured fields
//! (`wdsf`).

pub mod build;
pub mod field;
pub mod inbound;
pub mod parse;
pub mod wdsf;

/// The escape that starts every command.
pub const ESC: u8 = 0x04;

/// Commands (the byte after `ESC`).
pub mod cmd {
    /// Clear Unit: 24×80, everything blank, format table cleared.
    pub const CLEAR_UNIT: u8 = 0x40;
    /// Clear Unit Alternate: one parameter byte selects the size.
    pub const CLEAR_UNIT_ALT: u8 = 0x20;
    /// Clear Format Table.
    pub const CLEAR_FORMAT_TABLE: u8 = 0x50;
    /// Write To Display: two control characters, then orders.
    pub const WRITE_TO_DISPLAY: u8 = 0x11;
    /// Write Error Code: text for the error line.
    pub const WRITE_ERROR_CODE: u8 = 0x21;
    /// Write Error Code To Window.
    pub const WRITE_ERROR_CODE_WINDOW: u8 = 0x22;
    /// Read Input Fields: two control characters.
    pub const READ_INPUT_FIELDS: u8 = 0x42;
    /// Read MDT Fields: two control characters.
    pub const READ_MDT_FIELDS: u8 = 0x52;
    /// Read MDT Fields Alternate: two control characters.
    pub const READ_MDT_FIELDS_ALT: u8 = 0x82;
    /// Read Screen (Immediate).
    pub const READ_SCREEN: u8 = 0x62;
    /// Read Screen with extended attributes.
    pub const READ_SCREEN_EXT: u8 = 0x64;
    /// Read Screen to print.
    pub const READ_SCREEN_PRINT: u8 = 0x66;
    /// Read Screen to print with extended attributes.
    pub const READ_SCREEN_PRINT_EXT: u8 = 0x68;
    /// Read Immediate.
    pub const READ_IMMEDIATE: u8 = 0x72;
    /// Read Modified Immediate Alternate.
    pub const READ_IMMEDIATE_ALT: u8 = 0x83;
    /// Save Screen.
    pub const SAVE_SCREEN: u8 = 0x02;
    /// Save Partial Screen.
    pub const SAVE_PARTIAL_SCREEN: u8 = 0x03;
    /// Restore Screen: what follows is what Save Screen returned.
    pub const RESTORE_SCREEN: u8 = 0x12;
    /// Restore Partial Screen.
    pub const RESTORE_PARTIAL_SCREEN: u8 = 0x13;
    /// Roll: three parameter bytes.
    pub const ROLL: u8 = 0x23;
    /// Write Structured Field.
    pub const WRITE_STRUCTURED_FIELD: u8 = 0xF3;
    /// Copy To Printer.
    pub const COPY_TO_PRINTER: u8 = 0x16;

    /// The name for traces.
    pub fn name(c: u8) -> &'static str {
        match c {
            CLEAR_UNIT => "clear-unit",
            CLEAR_UNIT_ALT => "clear-unit-alt",
            CLEAR_FORMAT_TABLE => "clear-format-table",
            WRITE_TO_DISPLAY => "wtd",
            WRITE_ERROR_CODE => "write-error-code",
            WRITE_ERROR_CODE_WINDOW => "write-error-code-window",
            READ_INPUT_FIELDS => "read-input-fields",
            READ_MDT_FIELDS => "read-mdt-fields",
            READ_MDT_FIELDS_ALT => "read-mdt-fields-alt",
            READ_SCREEN => "read-screen",
            READ_SCREEN_EXT => "read-screen-ext",
            READ_SCREEN_PRINT => "read-screen-print",
            READ_SCREEN_PRINT_EXT => "read-screen-print-ext",
            READ_IMMEDIATE => "read-immediate",
            READ_IMMEDIATE_ALT => "read-immediate-alt",
            SAVE_SCREEN => "save-screen",
            SAVE_PARTIAL_SCREEN => "save-partial-screen",
            RESTORE_SCREEN => "restore-screen",
            RESTORE_PARTIAL_SCREEN => "restore-partial-screen",
            ROLL => "roll",
            WRITE_STRUCTURED_FIELD => "wsf",
            COPY_TO_PRINTER => "copy-to-printer",
            _ => "?",
        }
    }
}

/// Orders inside Write To Display.
pub mod order {
    /// Start of Header: a length byte, then up to seven bytes.
    pub const SOH: u8 = 0x01;
    /// Repeat to Address: row, column, the byte.
    pub const RA: u8 = 0x02;
    /// Erase to Address: row, column, a length byte and attribute types.
    pub const EA: u8 = 0x03;
    /// Transparent Data: two length bytes, then the data.
    pub const TD: u8 = 0x10;
    /// Set Buffer Address: row, column.
    pub const SBA: u8 = 0x11;
    /// Write Extended Attribute: type, value.
    pub const WEA: u8 = 0x12;
    /// Insert Cursor: row, column.
    pub const IC: u8 = 0x13;
    /// Move Cursor: row, column.
    pub const MC: u8 = 0x14;
    /// Start of Field: the field words, the attribute, the length.
    pub const SF: u8 = 0x1D;
}

/// Control character 1 of Write To Display and the read commands: the
/// high three bits say what happens to the keyboard and the fields.
pub mod cc1 {
    /// Lock the keyboard (the default: anything but `0x00`).
    pub const LOCK: u8 = 0x20;
    /// Reset MDT of non-bypass fields.
    pub const RESET_MDT: u8 = 0x40;
    /// Reset MDT of every field.
    pub const RESET_ALL_MDT: u8 = 0x60;
    /// Null non-bypass fields with MDT set.
    pub const NULL_MDT: u8 = 0x80;
    /// Reset MDT and null non-bypass fields.
    pub const RESET_NULL: u8 = 0xA0;
    /// Reset MDT and null non-bypass fields with MDT set.
    pub const RESET_NULL_MDT: u8 = 0xC0;
    /// Reset every MDT and null non-bypass fields.
    pub const RESET_ALL_NULL: u8 = 0xE0;
}

/// Control character 2 bits.
pub mod cc2 {
    /// Cursor stops blinking.
    pub const CLEAR_BLINK: u8 = 0x40;
    /// Cursor blinks.
    pub const SET_BLINK: u8 = 0x20;
    /// Unlock the keyboard and drop a pending AID.
    pub const UNLOCK: u8 = 0x08;
    /// Sound the alarm.
    pub const ALARM: u8 = 0x04;
    /// Message light off.
    pub const MESSAGE_OFF: u8 = 0x02;
    /// Message light on.
    pub const MESSAGE_ON: u8 = 0x01;
}

/// Attention identifiers the display sends.
pub mod aid {
    /// Enter.
    pub const ENTER: u8 = 0xF1;
    /// Help.
    pub const HELP: u8 = 0xF3;
    /// Roll Down (Page Up).
    pub const ROLL_DOWN: u8 = 0xF4;
    /// Roll Up (Page Down).
    pub const ROLL_UP: u8 = 0xF5;
    /// Print.
    pub const PRINT: u8 = 0xF6;
    /// Record Backspace.
    pub const RECORD_BACKSPACE: u8 = 0xF8;
    /// Auto Enter (a field with the auto-enter bit filled).
    pub const AUTO_ENTER: u8 = 0x3F;
    /// Clear.
    pub const CLEAR: u8 = 0xBD;
    /// The reply to a Query structured field.
    pub const QUERY_REPLY: u8 = 0x88;
    /// Selection field or pointer-device AID.
    pub const SELECTION: u8 = 0x7F;

    /// `F1`..`F24`.
    pub fn function(n: u8) -> u8 {
        if n <= 12 { 0x30 + n } else { 0xB0 + (n - 12) }
    }

    /// The function key number of an AID, when it is one.
    pub fn function_of(a: u8) -> Option<u8> {
        match a {
            0x31..=0x3C => Some(a - 0x30),
            0xB1..=0xBC => Some(a - 0xB0 + 12),
            _ => None,
        }
    }

    /// The name for traces.
    pub fn name(a: u8) -> String {
        match a {
            ENTER => "Enter".into(),
            HELP => "Help".into(),
            ROLL_DOWN => "RollDown".into(),
            ROLL_UP => "RollUp".into(),
            PRINT => "Print".into(),
            RECORD_BACKSPACE => "RecordBackspace".into(),
            AUTO_ENTER => "AutoEnter".into(),
            CLEAR => "Clear".into(),
            QUERY_REPLY => "QueryReply".into(),
            SELECTION => "Selection".into(),
            _ => function_of(a).map_or_else(|| format!("{a:#04x}"), |n| format!("F{n}")),
        }
    }
}

/// Screen attributes, `0x20` to `0x3F`.
pub mod attr {
    /// Green.
    pub const GREEN: u8 = 0x20;
    /// Green, reverse image.
    pub const GREEN_RI: u8 = 0x21;
    /// White (high intensity).
    pub const WHITE: u8 = 0x22;
    /// White, reverse image.
    pub const WHITE_RI: u8 = 0x23;
    /// Green, underline.
    pub const GREEN_UL: u8 = 0x24;
    /// Green, underline, reverse image.
    pub const GREEN_UL_RI: u8 = 0x25;
    /// White, underline.
    pub const WHITE_UL: u8 = 0x26;
    /// Nondisplay.
    pub const NONDISPLAY: u8 = 0x27;
    /// Red.
    pub const RED: u8 = 0x28;
    /// Red, reverse image.
    pub const RED_RI: u8 = 0x29;
    /// Red, blink.
    pub const RED_BL: u8 = 0x2A;
    /// Red, reverse image, blink.
    pub const RED_RI_BL: u8 = 0x2B;
    /// Red, underline.
    pub const RED_UL: u8 = 0x2C;
    /// Red, underline, reverse image.
    pub const RED_UL_RI: u8 = 0x2D;
    /// Red, underline, blink.
    pub const RED_UL_BL: u8 = 0x2E;
    /// Nondisplay.
    pub const NONDISPLAY_2: u8 = 0x2F;
    /// Turquoise, column separators.
    pub const TURQUOISE_CS: u8 = 0x30;
    /// Turquoise, column separators, reverse image.
    pub const TURQUOISE_CS_RI: u8 = 0x31;
    /// Yellow, column separators.
    pub const YELLOW_CS: u8 = 0x32;
    /// Yellow, column separators, reverse image.
    pub const YELLOW_CS_RI: u8 = 0x33;
    /// Turquoise, underline, column separators.
    pub const TURQUOISE_UL_CS: u8 = 0x34;
    /// Turquoise, underline, column separators, reverse image.
    pub const TURQUOISE_UL_CS_RI: u8 = 0x35;
    /// Yellow, underline, column separators.
    pub const YELLOW_UL_CS: u8 = 0x36;
    /// Nondisplay.
    pub const NONDISPLAY_3: u8 = 0x37;
    /// Pink.
    pub const PINK: u8 = 0x38;
    /// Pink, reverse image.
    pub const PINK_RI: u8 = 0x39;
    /// Blue.
    pub const BLUE: u8 = 0x3A;
    /// Blue, reverse image.
    pub const BLUE_RI: u8 = 0x3B;
    /// Pink, underline.
    pub const PINK_UL: u8 = 0x3C;
    /// Pink, underline, reverse image.
    pub const PINK_UL_RI: u8 = 0x3D;
    /// Blue, underline.
    pub const BLUE_UL: u8 = 0x3E;
    /// Nondisplay.
    pub const NONDISPLAY_4: u8 = 0x3F;

    /// Whether a byte is an attribute.
    pub fn is_attr(b: u8) -> bool {
        (0x20..=0x3F).contains(&b)
    }

    /// Whether the attribute shows nothing.
    pub fn is_nondisplay(a: u8) -> bool {
        a & 0x07 == 0x07
    }

    /// Whether the attribute underlines.
    pub fn is_underline(a: u8) -> bool {
        a & 0x04 != 0 && !is_nondisplay(a)
    }

    /// Whether the attribute is reverse image.
    pub fn is_reverse(a: u8) -> bool {
        a & 0x01 != 0 && !is_nondisplay(a)
    }

    /// Whether the attribute blinks.
    pub fn is_blink(a: u8) -> bool {
        a & 0x02 != 0 && (0x28..=0x2F).contains(&a) && !is_nondisplay(a)
    }

    /// Whether the attribute draws column separators.
    pub fn is_column_sep(a: u8) -> bool {
        (0x30..=0x37).contains(&a) && !is_nondisplay(a)
    }

    /// The color name.
    pub fn color(a: u8) -> &'static str {
        match a {
            0x20..=0x27 => {
                if a & 0x02 != 0 {
                    "white"
                } else {
                    "green"
                }
            }
            0x28..=0x2F => "red",
            0x30..=0x37 => {
                if a & 0x02 != 0 {
                    "yellow"
                } else {
                    "turquoise"
                }
            }
            0x38..=0x3F => {
                if a & 0x02 != 0 {
                    "blue"
                } else {
                    "pink"
                }
            }
            _ => "?",
        }
    }
}

/// The null character in a field.
pub const NULL: u8 = 0x00;
/// The dup character.
pub const DUP: u8 = 0x1C;
/// The field mark character.
pub const FIELD_MARK: u8 = 0x1E;
