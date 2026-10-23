// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Field Format Word and the Field Control Words of a Start of
//! Field order.

/// Field shift and edit types, the low three bits of the first FFW byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum Shift {
    /// Alphanumeric shift: anything.
    #[default]
    AlphaShift = 0,
    /// Alphabetic only.
    AlphaOnly = 1,
    /// Numeric shift: the numeric row without shifting.
    NumericShift = 2,
    /// Numeric only: digits, sign, decimal point, blank.
    NumericOnly = 3,
    /// Katakana shift.
    Katakana = 4,
    /// Digits only.
    DigitsOnly = 5,
    /// I/O: magnetic stripe reader data.
    Io = 6,
    /// Signed numeric: digits, the sign typed with Field+ and Field-.
    SignedNumeric = 7,
}

impl Shift {
    /// From the low three bits.
    pub fn from_bits(b: u8) -> Self {
        match b & 7 {
            1 => Self::AlphaOnly,
            2 => Self::NumericShift,
            3 => Self::NumericOnly,
            4 => Self::Katakana,
            5 => Self::DigitsOnly,
            6 => Self::Io,
            7 => Self::SignedNumeric,
            _ => Self::AlphaShift,
        }
    }
}

/// Right adjustment and mandatory fill, the low three bits of the
/// second FFW byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Adjust {
    /// None.
    #[default]
    None,
    /// Right adjust, zero fill.
    RightZero,
    /// Right adjust, blank fill.
    RightBlank,
    /// Mandatory fill: all or nothing.
    MandatoryFill,
}

/// The Field Format Word, decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[allow(clippy::struct_excessive_bools)] // the word's bits
pub struct Ffw {
    /// Bypass: the field cannot be typed into.
    pub bypass: bool,
    /// Dup and Field Mark keys allowed.
    pub dup: bool,
    /// Modified data tag.
    pub mdt: bool,
    /// Shift and edit type.
    pub shift: Shift,
    /// Auto enter when the last position is typed.
    pub auto_enter: bool,
    /// Field Exit required to leave the field.
    pub field_exit_required: bool,
    /// Lower case folded to upper.
    pub monocase: bool,
    /// Mandatory entry.
    pub mandatory_entry: bool,
    /// Adjustment.
    pub adjust: Adjust,
}

impl Ffw {
    /// Decode the two bytes.
    pub fn decode(word: u16) -> Self {
        let [b1, b2] = word.to_be_bytes();
        Self {
            bypass: b1 & 0x20 != 0,
            dup: b1 & 0x10 != 0,
            mdt: b1 & 0x08 != 0,
            shift: Shift::from_bits(b1),
            auto_enter: b2 & 0x80 != 0,
            field_exit_required: b2 & 0x40 != 0,
            monocase: b2 & 0x20 != 0,
            mandatory_entry: b2 & 0x08 != 0,
            adjust: match b2 & 7 {
                5 => Adjust::RightZero,
                6 => Adjust::RightBlank,
                7 => Adjust::MandatoryFill,
                _ => Adjust::None,
            },
        }
    }

    /// Encode to the two bytes; the identifying `01` bits set.
    pub fn encode(&self) -> u16 {
        let mut b1 = 0x40u8 | (self.shift as u8);
        if self.bypass {
            b1 |= 0x20;
        }
        if self.dup {
            b1 |= 0x10;
        }
        if self.mdt {
            b1 |= 0x08;
        }
        let mut b2 = match self.adjust {
            Adjust::None => 0,
            Adjust::RightZero => 5,
            Adjust::RightBlank => 6,
            Adjust::MandatoryFill => 7,
        };
        if self.auto_enter {
            b2 |= 0x80;
        }
        if self.field_exit_required {
            b2 |= 0x40;
        }
        if self.monocase {
            b2 |= 0x20;
        }
        if self.mandatory_entry {
            b2 |= 0x08;
        }
        u16::from_be_bytes([b1, b2])
    }

    /// Whether the two bytes are an FFW (the `01` identifier).
    pub fn is_ffw(b: u8) -> bool {
        b & 0xC0 == 0x40
    }
}

/// Field Control Word types (the first byte).
pub mod fcw {
    /// Cursor progression: the next field's number in the second byte.
    pub const CURSOR_PROGRESSION: u8 = 0x80;
    /// Magnetic stripe reader.
    pub const MAGNETIC_STRIPE: u8 = 0x81;
    /// Selector light pen.
    pub const LIGHT_PEN: u8 = 0x82;
    /// Ideographic controls.
    pub const IDEOGRAPHIC: u8 = 0x84;
    /// Transparency.
    pub const TRANSPARENCY: u8 = 0x85;
    /// Continued entry field (`01` first, `03` middle, `02` last).
    pub const CONTINUED: u8 = 0x86;
    /// Word wrap.
    pub const WORD_WRAP: u8 = 0x87;
    /// Self-check (modulus 10 and 11).
    pub const SELF_CHECK: u8 = 0xB1;
    /// Highlighted entry field attribute.
    pub const ENTRY_ATTR: u8 = 0xB4;
}

/// Whether the two bytes are a Field Control Word.
pub fn is_fcw(b: u8) -> bool {
    b & 0x80 != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffw_round_trip() {
        let f = Ffw {
            bypass: false,
            dup: true,
            mdt: true,
            shift: Shift::SignedNumeric,
            auto_enter: true,
            field_exit_required: false,
            monocase: true,
            mandatory_entry: true,
            adjust: Adjust::RightZero,
        };
        let w = f.encode();
        assert_eq!(w & 0xC000, 0x4000);
        assert_eq!(Ffw::decode(w), f);
    }
}
