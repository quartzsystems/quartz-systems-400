// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Data formats: the super extended format the server describes
//! columns and markers with (`0x3812`, `0x3813`), and the client's
//! format of the marker data it sends (`0x381E`, 64-byte records).

use super::types::ColType;
use crate::{reply, text};

/// A field record of the server's format.
const RECORD: usize = 48;

/// Its size as the record's length field holds it.
const RECORD_LL: u16 = 48;

/// The super extended format of `cols`; `result` marks a result
/// format (header byte 8), which also names the columns.
pub fn encode(cols: &[ColType], result: bool) -> Vec<u8> {
    let n = cols.len();
    let size: u32 = cols.iter().map(|c| c.len).sum();
    let mut v = 0u32.to_be_bytes().to_vec();
    v.extend(reply::len32(n).to_be_bytes());
    v.extend(if result { [1, 0, 0xFF, 0] } else { [0, 0, 0, 0] });
    v.extend(size.to_be_bytes());
    let names: Vec<Vec<u8>> = cols
        .iter()
        .map(|c| {
            let (ccsid, name) = if result { (37u16, text::ebcdic(&c.name)) } else { (0, Vec::new()) };
            let mut item = reply::len32(8 + name.len()).to_be_bytes().to_vec();
            item.extend(0x3840u16.to_be_bytes());
            item.extend(ccsid.to_be_bytes());
            item.extend(name);
            item
        })
        .collect();
    let mut var_at = RECORD * n;
    for (i, c) in cols.iter().enumerate() {
        let mut r = RECORD_LL.to_be_bytes().to_vec();
        r.extend(c.sqltype().to_be_bytes());
        r.extend(c.len.to_be_bytes());
        r.extend(c.scale.to_be_bytes());
        r.extend(c.precision.to_be_bytes());
        r.extend(c.ccsid.to_be_bytes());
        r.push(0xF0);
        r.extend(u16::from(c.column).to_be_bytes());
        r.extend([0u8; 4 + 1 + 4 + 4 + 2]);
        // From this record's start to its name.
        let off = var_at - RECORD * i;
        r.extend(reply::len32(off).to_be_bytes());
        r.extend(reply::len32(names[i].len()).to_be_bytes());
        r.extend([0u8; 8]);
        v.extend(r);
        var_at += names[i].len();
    }
    for item in names {
        v.extend(item);
    }
    v
}

/// The client's format of its marker data.
pub fn decode_client(b: &[u8]) -> Vec<ColType> {
    let n = reply::be32(b, 4) as usize;
    let mut out = Vec::with_capacity(n);
    let mut at = 16;
    for _ in 0..n {
        let ll = usize::from(reply::be16(b, at)).max(48);
        let r = reply::slice(b, at, ll);
        let t = reply::be16(r, 2);
        let base = t & !1;
        out.push(ColType {
            base,
            null: t & 1 == 1,
            len: reply::be32(r, 4),
            scale: reply::be16(r, 8),
            precision: reply::be16(r, 10),
            ccsid: reply::be16(r, 12),
            name: String::new(),
            column: false,
        });
        at += ll;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_oracle_count_format() {
        // `SELECT COUNT(*) ...`: the 77-byte result format of the trace.
        let c = ColType::integer("00001", false);
        let want = "00000000000000010100ff0000000004003001f000000004000000090000f00000000000000000000000000000000000000000300000000d00000000000000000000000d38400025f0f0f0f0f1";
        let got: String = encode(&[c], true).iter().fold(String::new(), |mut s, b| {
            use std::fmt::Write as _;
            let _ = write!(s, "{b:02x}");
            s
        });
        assert_eq!(got, want);
    }
}
