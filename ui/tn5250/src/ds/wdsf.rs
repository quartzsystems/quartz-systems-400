// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Structured fields: the Query and its reply, and the workstation
//! GUI constructs (windows, selection fields, scroll bars). Only the
//! Query is answered here; the terminal keeps the others as parsed
//! records for the trace and for the window borders it draws.

/// The 5250 structured field class.
pub const CLASS_5250: u8 = 0xD9;

/// Structured field types of class `0xD9`.
pub mod kind {
    /// Define Audit Window Table.
    pub const AUDIT_WINDOW_TABLE: u8 = 0x30;
    /// Define Command Key Function.
    pub const COMMAND_KEY_FUNCTION: u8 = 0x31;
    /// Read Text Screen.
    pub const READ_TEXT_SCREEN: u8 = 0x32;
    /// Define Pending Operations.
    pub const PENDING_OPERATIONS: u8 = 0x33;
    /// Define Text Screen Format.
    pub const TEXT_SCREEN_FORMAT: u8 = 0x34;
    /// Define Scale Line.
    pub const SCALE_LINE: u8 = 0x35;
    /// Write Text Screen.
    pub const WRITE_TEXT_SCREEN: u8 = 0x36;
    /// Define Special Characters.
    pub const SPECIAL_CHARACTERS: u8 = 0x37;
    /// Pending Data.
    pub const PENDING_DATA: u8 = 0x38;
    /// Define Operator Error Messages.
    pub const OPERATOR_ERROR_MESSAGES: u8 = 0x39;
    /// Define Pitch Table.
    pub const PITCH_TABLE: u8 = 0x3A;
    /// Define Fake DP Command Key Function.
    pub const FAKE_DP_COMMAND_KEY: u8 = 0x3B;
    /// Pass-Through.
    pub const PASS_THROUGH: u8 = 0x3F;
    /// Define Selection Field.
    pub const SELECTION_FIELD: u8 = 0x50;
    /// Create Window.
    pub const CREATE_WINDOW: u8 = 0x51;
    /// Unrestricted Window Cursor Movement.
    pub const UNRESTRICTED_CURSOR: u8 = 0x52;
    /// Define Scroll Bar Field.
    pub const SCROLL_BAR: u8 = 0x53;
    /// Write Data.
    pub const WRITE_DATA: u8 = 0x54;
    /// Programmable Mouse Buttons.
    pub const MOUSE_BUTTONS: u8 = 0x55;
    /// Remove GUI Selection Field.
    pub const REMOVE_SELECTION_FIELD: u8 = 0x58;
    /// Remove GUI Window.
    pub const REMOVE_WINDOW: u8 = 0x59;
    /// Remove GUI Scroll Bar Field.
    pub const REMOVE_SCROLL_BAR: u8 = 0x5B;
    /// Remove All GUI Constructs.
    pub const REMOVE_ALL_GUI: u8 = 0x5F;
    /// Draw/Erase Grid Lines.
    pub const GRID_LINES: u8 = 0x60;
    /// Clear Grid Line Buffer.
    pub const CLEAR_GRID_LINES: u8 = 0x61;
    /// Query 5250 (its reply carries the display's capabilities).
    pub const QUERY: u8 = 0x70;
    /// Query Station State.
    pub const QUERY_STATION_STATE: u8 = 0x72;

    /// The name for traces.
    pub fn name(k: u8) -> &'static str {
        match k {
            SELECTION_FIELD => "define-selection-field",
            CREATE_WINDOW => "create-window",
            UNRESTRICTED_CURSOR => "unrestricted-cursor",
            SCROLL_BAR => "define-scroll-bar",
            WRITE_DATA => "write-data",
            MOUSE_BUTTONS => "mouse-buttons",
            REMOVE_SELECTION_FIELD => "remove-selection-field",
            REMOVE_WINDOW => "remove-window",
            REMOVE_SCROLL_BAR => "remove-scroll-bar",
            REMOVE_ALL_GUI => "remove-all-gui",
            GRID_LINES => "grid-lines",
            CLEAR_GRID_LINES => "clear-grid-lines",
            QUERY => "query",
            QUERY_STATION_STATE => "query-station-state",
            PENDING_OPERATIONS => "pending-operations",
            _ => "?",
        }
    }
}

/// A structured field, split.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sf {
    /// The class.
    pub class: u8,
    /// The type.
    pub kind: u8,
    /// The bytes after the type.
    pub body: Vec<u8>,
}

impl Sf {
    /// Split the field (length bytes included). `None` when shorter
    /// than its header.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 4 {
            return None;
        }
        Some(Self {
            class: bytes[2],
            kind: bytes[3],
            body: bytes[4..].to_vec(),
        })
    }
}

/// The printer device type and model of a TN5250E printer session's
/// terminal type (`IBM-3812-1`, `IBM-5553-B01`, `IBM-5224-1`), when it
/// is one.
pub fn printer_type_model(terminal_type: &str) -> Option<(&'static str, &'static str)> {
    match terminal_type.trim().to_ascii_uppercase().as_str() {
        "IBM-3812-1" => Some(("3812", "1")),
        "IBM-5553-B01" => Some(("5553", "B01")),
        "IBM-5224-1" => Some(("5224", "1")),
        "IBM-5225-1" => Some(("5225", "1")),
        "IBM-5256-1" => Some(("5256", "1")),
        _ => None,
    }
}

/// The display models the server and the terminal know.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Model {
    /// `IBM-3179-2`: 24×80, color.
    #[default]
    M3179,
    /// `IBM-3477-FC`: 27×132, color.
    M3477Fc,
    /// `IBM-5555-C01`: 24×80 DBCS, accepted as a 3179-2.
    M5555C01,
    /// `IBM-5292-2`: 24×80 color, accepted as a 3179-2.
    M5292,
}

impl Model {
    /// From a negotiated terminal type.
    pub fn from_terminal_type(t: &str) -> Option<Self> {
        match t.trim().to_ascii_uppercase().as_str() {
            "IBM-3179-2" | "IBM-3180-2" | "IBM-3196-A1" | "IBM-5251-11" | "IBM-5291-1" => {
                Some(Self::M3179)
            }
            "IBM-3477-FC" | "IBM-3477-FG" => Some(Self::M3477Fc),
            "IBM-5555-C01" | "IBM-5555-B01" => Some(Self::M5555C01),
            "IBM-5292-2" => Some(Self::M5292),
            _ => None,
        }
    }

    /// The terminal type string.
    pub fn terminal_type(self) -> &'static str {
        match self {
            Self::M3179 => "IBM-3179-2",
            Self::M3477Fc => "IBM-3477-FC",
            Self::M5555C01 => "IBM-5555-C01",
            Self::M5292 => "IBM-5292-2",
        }
    }

    /// Device type and model as the device description shows them.
    pub fn device_type_model(self) -> (&'static str, &'static str) {
        match self {
            Self::M3179 => ("3179", "2"),
            Self::M3477Fc => ("3477", "FC"),
            Self::M5555C01 => ("5555", "C01"),
            Self::M5292 => ("5292", "2"),
        }
    }

    /// Whether 27×132 is available.
    pub fn wide(self) -> bool {
        matches!(self, Self::M3477Fc)
    }

    /// Machine type and model digits for the query reply.
    fn query_type(self) -> ([u8; 4], [u8; 3]) {
        let e = |s: &str| {
            let mut out = [0x40u8; 4];
            for (i, c) in s.bytes().enumerate() {
                out[i] = 0xF0 + (c - b'0');
            }
            out
        };
        let (t, m) = match self {
            Self::M3179 => ("3179", "002"),
            Self::M3477Fc => ("3477", "0FC"),
            Self::M5555C01 => ("5555", "C01"),
            Self::M5292 => ("5292", "002"),
        };
        let mut model = [0x40u8; 3];
        for (i, c) in m.bytes().enumerate() {
            model[i] = if c.is_ascii_digit() {
                0xF0 + (c - b'0')
            } else {
                0xC1 + (c - b'A')
            };
        }
        (e(t), model)
    }
}

/// What the terminal claims in its query reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capabilities {
    /// The model.
    pub model: Model,
    /// Windows, selection fields and scroll bars as GUI constructs.
    pub gui: bool,
    /// Color.
    pub color: bool,
}

/// The Query reply's data, to be sent with AID `0x88`: cursor at row
/// and column zero, the AID, and the structured field.
pub fn query_reply(caps: Capabilities) -> Vec<u8> {
    let (kind, model) = caps.model.query_type();
    let mut d = vec![0x00, 0x00, super::aid::QUERY_REPLY];
    d.extend([0x00, 0x3A, CLASS_5250, kind::QUERY]);
    d.push(0x80); // flags: the reply
    d.extend([0x06, 0x00]); // controller hardware class: 5494
    d.extend([0x01, 0x01, 0x00]); // controller code level
    d.extend([0u8; 16]); // reserved
    d.push(0x01); // workstation type: display
    d.extend(kind);
    d.extend(model);
    d.push(0x02); // keyboard id: standard
    d.push(0x00); // extended keyboard id
    d.push(0x00); // reserved
    d.extend([0x00, 0x61, 0x50, 0x00]); // display serial number
    d.extend([0x01, 0x00]); // 256 input fields
    d.extend([0x00, 0x00, 0x00]); // reserved
    // Controller and display capabilities.
    let mut c1 = 0x20u8; // row 1, column 1 support
    if caps.model.wide() {
        c1 |= 0x01; // 27x132
    }
    if caps.color {
        c1 |= 0x08; // wide and color: extended attributes
    }
    d.push(c1);
    let mut c2 = 0x00u8;
    if caps.gui {
        c2 |= 0x80; // GUI-like characters, windows, selection fields
    }
    d.push(c2);
    d.push(0x00);
    d.push(0x00);
    d.push(0x00);
    d.push(0x00);
    while d.len() < 3 + 0x3A {
        d.push(0x00);
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply_has_the_declared_length() {
        let r = query_reply(Capabilities {
            model: Model::M3477Fc,
            gui: true,
            color: true,
        });
        assert_eq!(r.len(), 3 + 0x3A);
        assert_eq!(&r[3..7], &[0x00, 0x3A, 0xD9, 0x70]);
        assert_eq!(&r[30..34], &[0xF3, 0xF4, 0xF7, 0xF7]);
    }
}
