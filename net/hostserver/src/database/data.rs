// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Row data: result rows the server sends (`0x380E`) and the marker
//! rows a client sends (`0x381F`), both a 20-byte header, the null
//! indicators (rows × columns × 2, `0xFFFF` null) and fixed-size rows.

use sql::value::Value;

use super::types::{ColType, decode, encode};
use crate::reply;

/// Result data for `rows` of `cols`.
pub fn encode_rows(cols: &[ColType], rows: &[Vec<Value>], consistency: u32) -> Vec<u8> {
    let size: u32 = cols.iter().map(|c| c.len).sum();
    let mut v = consistency.to_be_bytes().to_vec();
    v.extend(reply::len32(rows.len()).to_be_bytes());
    v.extend(reply::len16(cols.len()).to_be_bytes());
    v.extend(2u16.to_be_bytes());
    v.extend(0u32.to_be_bytes());
    v.extend(size.to_be_bytes());
    for r in rows {
        for (i, _) in cols.iter().enumerate() {
            let null = r.get(i).is_none_or(Value::is_null);
            v.extend(if null { [0xFF, 0xFF] } else { [0, 0] });
        }
    }
    for r in rows {
        for (i, c) in cols.iter().enumerate() {
            match r.get(i) {
                Some(x) if !x.is_null() => v.extend(encode(x, c)),
                // A null's bytes are an empty value: blanks for text.
                _ => v.extend(encode(&super::types::empty(c), c)),
            }
        }
    }
    v
}

/// The marker rows a client sent with its format.
pub fn decode_rows(b: &[u8], cols: &[ColType]) -> Vec<Vec<Value>> {
    let rows = reply::be32(b, 4) as usize;
    let ncols = usize::from(reply::be16(b, 8));
    let ind = usize::from(reply::be16(b, 10)).max(2);
    let row_size = reply::be32(b, 16) as usize;
    let data_at = 20 + rows * ncols * ind;
    let mut out = Vec::with_capacity(rows);
    for r in 0..rows {
        let mut row = Vec::with_capacity(cols.len());
        let mut at = data_at + r * row_size;
        for (i, c) in cols.iter().enumerate() {
            let ia = 20 + (r * ncols + i) * ind;
            let null = b.get(ia).is_some_and(|x| x & 0x80 != 0);
            let n = c.len as usize;
            row.push(if null { Value::Null } else { decode(reply::slice(b, at, n), c) });
            at += n;
        }
        out.push(row);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_row_matches_the_oracle() {
        let c = ColType::integer("00001", false);
        let d = encode_rows(std::slice::from_ref(&c), &[vec![Value::int(1)]], 4);
        assert_eq!(d, [0, 0, 0, 4, 0, 0, 0, 1, 0, 1, 0, 2, 0, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0, 0, 0, 1]);
        assert_eq!(decode_rows(&d, &[c]), vec![vec![Value::int(1)]]);
    }
}
