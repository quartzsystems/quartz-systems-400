// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The inbound record a display sends for an attention key: cursor
//! row and column, the AID, and the fields, each behind a Set Buffer
//! Address order.

use super::order::SBA;

/// One field's data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldData {
    /// Row, 1-based.
    pub row: u8,
    /// Column, 1-based.
    pub col: u8,
    /// The bytes as sent (trailing nulls removed by the display).
    pub data: Vec<u8>,
}

/// An inbound record's data.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Inbound {
    /// Cursor row, 1-based.
    pub row: u8,
    /// Cursor column, 1-based.
    pub col: u8,
    /// The attention identifier.
    pub aid: u8,
    /// The fields.
    pub fields: Vec<FieldData>,
}

impl Inbound {
    /// The data bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![self.row, self.col, self.aid];
        for f in &self.fields {
            out.push(SBA);
            out.push(f.row);
            out.push(f.col);
            out.extend(&f.data);
        }
        out
    }

    /// From a record's data. `None` when the three leading bytes are
    /// missing.
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < 3 {
            return None;
        }
        let mut r = Self {
            row: data[0],
            col: data[1],
            aid: data[2],
            fields: Vec::new(),
        };
        let mut i = 3;
        while i < data.len() {
            if data[i] == SBA && i + 2 < data.len() {
                r.fields.push(FieldData {
                    row: data[i + 1],
                    col: data[i + 2],
                    data: Vec::new(),
                });
                i += 3;
            } else {
                if r.fields.is_empty() {
                    r.fields.push(FieldData {
                        row: 0,
                        col: 0,
                        data: Vec::new(),
                    });
                }
                r.fields.last_mut().expect("pushed").data.push(data[i]);
                i += 1;
            }
        }
        Some(r)
    }

    /// The field at a row and column, as text with trailing blanks
    /// kept and nulls shown as blanks.
    pub fn field_text(&self, row: u8, col: u8) -> Option<String> {
        self.fields
            .iter()
            .find(|f| f.row == row && f.col == col)
            .map(|f| {
                f.data
                    .iter()
                    .map(|&b| if b == 0 { ' ' } else { ccsid::char_of(b) })
                    .collect()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let r = Inbound {
            row: 6,
            col: 53,
            aid: 0xF1,
            fields: vec![
                FieldData {
                    row: 6,
                    col: 53,
                    data: vec![0xD8, 0xE2],
                },
                FieldData {
                    row: 7,
                    col: 53,
                    data: vec![],
                },
            ],
        };
        assert_eq!(Inbound::decode(&r.encode()).unwrap(), r);
        assert_eq!(r.field_text(6, 53).unwrap(), "QS");
    }
}
