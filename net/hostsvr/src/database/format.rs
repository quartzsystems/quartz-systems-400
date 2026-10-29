// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The super extended data format (`0x3812` for result columns,
//! `0x3813` for parameter markers): a 16-byte header, a 48-byte record
//! per field, then each field's variable information (its name as
//! `0x3840`: CCSID u16 and the name).

use crate::Error;
use crate::cp::db_reply;
use crate::frame::param;
use crate::template::{Template as _, template};

template! {
    base 0;
    /// The format header.
    pub struct FormatHeader {
        /// Consistency token.
        consistency: u32,
        /// Number of fields.
        field_count: u32,
        /// Date format, time format, date separator, time separator (or reserved).
        date_time: [u8; 4],
        /// Record size: the bytes of one row.
        record_size: u32,
    }
}

template! {
    base 0;
    /// One field's fixed description (offsets within the record).
    pub struct FieldRecord {
        /// Length of this record (48).
        ll: u16,
        /// SQL type (448 VARCHAR, 452 CHAR, 484 DECIMAL, 496 INTEGER, ...;
        /// odd when nullable).
        sql_type: u16,
        /// Length in bytes.
        length: u32,
        /// Scale (or character count for graphic types).
        scale: u16,
        /// Precision.
        precision: u16,
        /// CCSID.
        ccsid: u16,
        /// Parameter type for markers (EBCDIC `0` input, `1` output, `2` both).
        parameter_type: u8,
        /// Join reference position.
        join_ref: u16,
        /// LOB locator.
        lob_locator: u32,
        /// Flags (array, XML).
        flags: u8,
        /// Array maximum cardinality.
        max_cardinality: u32,
        /// LOB maximum size.
        lob_max: u32,
        /// Alignment.
        reserved: u16,
        /// Offset of the variable information from this record's start.
        var_offset: u32,
        /// Length of the variable information.
        var_length: u32,
        /// Reserved.
        reserved2: [u8; 8],
    }
}

/// A decoded format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuperFormat {
    /// The header.
    pub header: FormatHeader,
    /// One record per field.
    pub fields: Vec<FieldRecord>,
    /// Everything after the records (the variable information), as sent.
    pub variable: Vec<u8>,
}

impl SuperFormat {
    /// Decode a `0x3812`/`0x3813` parameter's data.
    pub fn decode(b: &[u8]) -> Result<Self, Error> {
        if b.is_empty() {
            return Ok(Self {
                header: FormatHeader::default(),
                fields: Vec::new(),
                variable: Vec::new(),
            });
        }
        let h = FormatHeader::decode(b.get(..FormatHeader::SIZE).ok_or(Error::Short {
            what: "data format",
            need: FormatHeader::SIZE,
            have: b.len(),
        })?)
        .ok_or_else(|| Error::Invalid("data format header".into()))?;
        let n = h.field_count as usize;
        let end = FormatHeader::SIZE + n * FieldRecord::SIZE;
        if end > b.len() {
            return Err(Error::Short {
                what: "data format fields",
                need: end,
                have: b.len(),
            });
        }
        let fields = (0..n)
            .map(|i| {
                let at = FormatHeader::SIZE + i * FieldRecord::SIZE;
                FieldRecord::decode(&b[at..at + FieldRecord::SIZE]).unwrap_or_default()
            })
            .collect();
        Ok(Self {
            header: h,
            fields,
            variable: b[end..].to_vec(),
        })
    }

    /// The data (an empty format encodes as no bytes).
    pub fn encode(&self) -> Vec<u8> {
        if self.fields.is_empty()
            && self.variable.is_empty()
            && self.header == FormatHeader::default()
        {
            return Vec::new();
        }
        let mut v = self.header.encode();
        for f in &self.fields {
            v.extend(f.encode());
        }
        v.extend_from_slice(&self.variable);
        v
    }

    /// A field's name: its CCSID and bytes.
    pub fn name(&self, i: usize) -> Option<(u16, Vec<u8>)> {
        let f = self.fields.get(i)?;
        let start = FormatHeader::SIZE + i * FieldRecord::SIZE + f.var_offset as usize;
        let records_end = FormatHeader::SIZE + self.fields.len() * FieldRecord::SIZE;
        let at = start.checked_sub(records_end)?;
        let area = self.variable.get(at..at + f.var_length as usize)?;
        let (params, _) = param::split(area);
        let p = params.into_iter().find(|p| p.cp == db_reply::FIELD_NAME)?;
        let ccsid = u16::from_be_bytes([*p.data.first()?, *p.data.get(1)?]);
        Some((ccsid, p.data[2..].to_vec()))
    }
}
