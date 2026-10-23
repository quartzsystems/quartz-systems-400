// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A builder for the outbound stream: the server's screen model and
//! the terminal's Save Screen answer both write through it.

use super::{ESC, cmd, order};

/// Accumulates commands and orders.
#[derive(Clone, Debug, Default)]
pub struct Builder {
    out: Vec<u8>,
}

impl Builder {
    /// Empty.
    pub fn new() -> Self {
        Self::default()
    }

    /// Clear Unit.
    pub fn clear_unit(&mut self) -> &mut Self {
        self.out.extend([ESC, cmd::CLEAR_UNIT]);
        self
    }

    /// Clear Unit Alternate: `0x00` selects 27×132.
    pub fn clear_unit_alt(&mut self, param: u8) -> &mut Self {
        self.out.extend([ESC, cmd::CLEAR_UNIT_ALT, param]);
        self
    }

    /// Clear Format Table.
    pub fn clear_format_table(&mut self) -> &mut Self {
        self.out.extend([ESC, cmd::CLEAR_FORMAT_TABLE]);
        self
    }

    /// Start a Write To Display.
    pub fn wtd(&mut self, cc1: u8, cc2: u8) -> &mut Self {
        self.out.extend([ESC, cmd::WRITE_TO_DISPLAY, cc1, cc2]);
        self
    }

    /// Start a Write Error Code.
    pub fn write_error_code(&mut self) -> &mut Self {
        self.out.extend([ESC, cmd::WRITE_ERROR_CODE]);
        self
    }

    /// Set Buffer Address.
    pub fn sba(&mut self, row: u8, col: u8) -> &mut Self {
        self.out.extend([order::SBA, row, col]);
        self
    }

    /// Insert Cursor.
    pub fn ic(&mut self, row: u8, col: u8) -> &mut Self {
        self.out.extend([order::IC, row, col]);
        self
    }

    /// Move Cursor.
    pub fn mc(&mut self, row: u8, col: u8) -> &mut Self {
        self.out.extend([order::MC, row, col]);
        self
    }

    /// Repeat to Address.
    pub fn ra(&mut self, row: u8, col: u8, byte: u8) -> &mut Self {
        self.out.extend([order::RA, row, col, byte]);
        self
    }

    /// Erase to Address (every attribute type).
    pub fn ea(&mut self, row: u8, col: u8) -> &mut Self {
        self.out.extend([order::EA, row, col, 1]);
        self
    }

    /// Start of Header with its bytes.
    pub fn soh(&mut self, bytes: &[u8]) -> &mut Self {
        self.out.push(order::SOH);
        self.out.push(bytes.len() as u8);
        self.out.extend(bytes);
        self
    }

    /// Start of Field.
    pub fn sf(&mut self, ffw: u16, fcws: &[u16], attr: u8, len: u16) -> &mut Self {
        self.out.push(order::SF);
        self.out.extend(ffw.to_be_bytes());
        for w in fcws {
            self.out.extend(w.to_be_bytes());
        }
        self.out.push(attr);
        self.out.extend(len.to_be_bytes());
        self
    }

    /// Raw bytes (characters and attributes).
    pub fn data(&mut self, bytes: &[u8]) -> &mut Self {
        self.out.extend(bytes);
        self
    }

    /// One attribute byte.
    pub fn attr(&mut self, a: u8) -> &mut Self {
        self.out.push(a);
        self
    }

    /// Text in EBCDIC; characters outside CP037 become `?`.
    pub fn text(&mut self, s: &str) -> &mut Self {
        for ch in s.chars() {
            self.out.push(ccsid::byte_of(ch).unwrap_or(0x6F));
        }
        self
    }

    /// Transparent Data.
    pub fn td(&mut self, bytes: &[u8]) -> &mut Self {
        self.out.push(order::TD);
        self.out.extend((bytes.len() as u16).to_be_bytes());
        self.out.extend(bytes);
        self
    }

    /// Read MDT Fields.
    pub fn read_mdt_fields(&mut self, cc1: u8, cc2: u8) -> &mut Self {
        self.out.extend([ESC, cmd::READ_MDT_FIELDS, cc1, cc2]);
        self
    }

    /// Read Input Fields.
    pub fn read_input_fields(&mut self, cc1: u8, cc2: u8) -> &mut Self {
        self.out.extend([ESC, cmd::READ_INPUT_FIELDS, cc1, cc2]);
        self
    }

    /// Read Immediate.
    pub fn read_immediate(&mut self) -> &mut Self {
        self.out.extend([ESC, cmd::READ_IMMEDIATE]);
        self
    }

    /// Read Screen (Immediate).
    pub fn read_screen(&mut self) -> &mut Self {
        self.out.extend([ESC, cmd::READ_SCREEN]);
        self
    }

    /// Save Screen.
    pub fn save_screen(&mut self) -> &mut Self {
        self.out.extend([ESC, cmd::SAVE_SCREEN]);
        self
    }

    /// Restore Screen.
    pub fn restore_screen(&mut self) -> &mut Self {
        self.out.extend([ESC, cmd::RESTORE_SCREEN]);
        self
    }

    /// Roll.
    pub fn roll(&mut self, up: bool, lines: u8, top: u8, bottom: u8) -> &mut Self {
        let d = (lines & 0x1F) | if up { 0x80 } else { 0 };
        self.out.extend([ESC, cmd::ROLL, d, top, bottom]);
        self
    }

    /// Write Structured Field: the class, type and body; the length is
    /// computed.
    pub fn wsf(&mut self, class: u8, kind: u8, body: &[u8]) -> &mut Self {
        self.out.extend([ESC, cmd::WRITE_STRUCTURED_FIELD]);
        self.out.extend(((body.len() + 4) as u16).to_be_bytes());
        self.out.extend([class, kind]);
        self.out.extend(body);
        self
    }

    /// Whether nothing was written.
    pub fn is_empty(&self) -> bool {
        self.out.is_empty()
    }

    /// The bytes.
    pub fn into_vec(self) -> Vec<u8> {
        self.out
    }

    /// The bytes so far.
    pub fn as_bytes(&self) -> &[u8] {
        &self.out
    }
}
