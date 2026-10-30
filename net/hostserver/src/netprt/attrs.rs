// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Network print attribute lists: a count, the entry size (12), then
//! per attribute its ID, type, length and offset (from the start of
//! the parameter, LL included), then the values. Strings are EBCDIC
//! with a null; numbers are binary (type 3) or packed 15,5 (type 7).
//! An ID list is a count, the entry size (2) and the IDs.

/// Attribute types.
pub mod kind {
    /// A big-endian i32.
    pub const BIN4: u16 = 3;
    /// EBCDIC text ending in a null.
    pub const STRING: u16 = 5;
    /// Packed decimal, 15 digits, 5 decimals.
    pub const PACKED: u16 = 7;
}

/// Attribute IDs the server answers.
#[allow(missing_docs)]
pub mod ids {
    pub const DATE: u16 = 0x0022;
    pub const COPIES: u16 = 0x001C;
    pub const JOBNAME: u16 = 0x003B;
    pub const JOBNUMBER: u16 = 0x003C;
    pub const JOBUSER: u16 = 0x003E;
    pub const LPI: u16 = 0x0040;
    pub const OUTPTY: u16 = 0x0048;
    pub const OUTQUELIB: u16 = 0x0049;
    pub const OUTQUE: u16 = 0x004A;
    pub const PAGELEN: u16 = 0x004E;
    pub const PAGEWIDTH: u16 = 0x0051;
    pub const PRTDEVTYPE: u16 = 0x005A;
    pub const PRTFLIB: u16 = 0x005B;
    pub const PRTFILE: u16 = 0x005C;
    pub const SPOOLFILE: u16 = 0x0068;
    pub const SPLFNUM: u16 = 0x0069;
    pub const SPLFSTATUS: u16 = 0x006A;
    pub const TIME: u16 = 0x006E;
    pub const PAGES: u16 = 0x006F;
    pub const USERDATA: u16 = 0x0072;
    pub const FORMTYPE: u16 = 0x0031;
    pub const NPSCCSID: u16 = 0x008A;
    pub const NPSLEVEL: u16 = 0x008D;
    pub const NLV_ID: u16 = 0x00B4;
    pub const JOBSYSTEM: u16 = 0x00FB;
    pub const WRITER: u16 = 0x0076;
    pub const PRINTER: u16 = 0x005D;
}

/// One attribute value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// Binary.
    Bin4(i32),
    /// Text (padded to `width` when given).
    Text(String, usize),
    /// Packed 15,5 of a whole number.
    Packed(i64),
}

/// One attribute as read: its ID, type and raw value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Raw {
    /// The ID.
    pub id: u16,
    /// The type.
    pub kind: u16,
    /// The value bytes.
    pub data: Vec<u8>,
}

impl Raw {
    /// A text value without its null and trailing blanks.
    pub fn text(&self) -> String {
        crate::text::from_ebcdic(self.data.strip_suffix(&[0]).unwrap_or(&self.data))
    }

    /// A binary value.
    pub fn bin4(&self) -> i32 {
        i32::from_be_bytes(self.data.get(..4).and_then(|b| b.try_into().ok()).unwrap_or([0; 4]))
    }
}

fn packed(n: i64) -> Vec<u8> {
    // 15 digits: ten whole, five decimal, then the sign.
    let digits = format!("{:010}00000", n.unsigned_abs() % 10_000_000_000);
    let mut nibbles: Vec<u8> = digits.bytes().map(|b| b - b'0').collect();
    nibbles.push(if n < 0 { 0x0D } else { 0x0F });
    nibbles.chunks(2).map(|c| c[0] << 4 | c[1]).collect()
}

fn value_bytes(v: &Value) -> (u16, Vec<u8>) {
    match v {
        Value::Bin4(n) => (kind::BIN4, n.to_be_bytes().to_vec()),
        Value::Text(s, w) => {
            let mut b = crate::text::ebcdic(s);
            if *w > 0 {
                b.resize(*w, 0x40);
            }
            b.push(0);
            (kind::STRING, b)
        }
        Value::Packed(n) => (kind::PACKED, packed(*n)),
    }
}

/// An attribute list's data.
pub fn encode(attrs: &[(u16, Value)]) -> Vec<u8> {
    let n = attrs.len();
    let mut head = crate::reply::len16(n).to_be_bytes().to_vec();
    head.extend(12u16.to_be_bytes());
    let mut values = Vec::new();
    // Offsets count from the parameter's LL: six bytes before the data.
    let base = 6 + 4 + 12 * n;
    for (id, v) in attrs {
        let (k, b) = value_bytes(v);
        head.extend(id.to_be_bytes());
        head.extend(k.to_be_bytes());
        head.extend(crate::reply::len32(b.len()).to_be_bytes());
        head.extend(crate::reply::len32(base + values.len()).to_be_bytes());
        values.extend(b);
    }
    head.extend(values);
    head
}

/// An attribute list's entries.
pub fn decode(d: &[u8]) -> Vec<Raw> {
    let n = usize::from(crate::reply::be16(d, 0));
    let el = usize::from(crate::reply::be16(d, 2)).max(12);
    (0..n)
        .filter_map(|i| {
            let e = d.get(4 + i * el..4 + (i + 1) * el)?;
            let len = crate::reply::be32(e, 4) as usize;
            let off = (crate::reply::be32(e, 8) as usize).checked_sub(6)?;
            Some(Raw {
                id: crate::reply::be16(e, 0),
                kind: crate::reply::be16(e, 2),
                data: d.get(off..off + len)?.to_vec(),
            })
        })
        .collect()
}

/// An ID list's IDs.
pub fn ids_of(d: &[u8]) -> Vec<u16> {
    let n = usize::from(crate::reply::be16(d, 0));
    let el = usize::from(crate::reply::be16(d, 2)).max(2);
    (0..n)
        .map(|i| crate::reply::be16(d, 4 + i * el))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_oracle_shapes() {
        // The CCSID answer of the exchange attributes trace.
        let d = encode(&[(ids::NPSCCSID, Value::Bin4(37))]);
        assert_eq!(
            d,
            [0, 1, 0, 0x0c, 0, 0x8a, 0, 3, 0, 0, 0, 4, 0, 0, 0, 0x16, 0, 0, 0, 0x25]
        );
        let r = decode(&d);
        assert_eq!(r[0].bin4(), 37);
        assert_eq!(packed(6), vec![0, 0, 0, 0, 0x06, 0, 0, 0x0F]);
    }
}
