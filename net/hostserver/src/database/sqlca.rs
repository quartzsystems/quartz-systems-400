// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SQL engine's SQLCA as the 136-byte DB2 layout of `0x3807`.

use hostsvr::database::sqlca::Sqlca as Wire;
use hostsvr::template::Template as _;
use sql::sqlca::Sqlca;

use crate::text;

fn fixed<const N: usize>(s: &str, fill: u8) -> [u8; N] {
    let mut a = [fill; N];
    let b = text::ebcdic(s);
    let n = b.len().min(N);
    a[..n].copy_from_slice(&b[..n]);
    a
}

fn u(i: i32) -> u32 {
    u32::from_be_bytes(i.to_be_bytes())
}

/// The wire SQLCA. The message tokens are not sent (`SQLERRML` 0): the
/// text travels in the message parameters.
pub fn encode(c: &Sqlca) -> Vec<u8> {
    let warn: String = c.sqlwarn.iter().map(|w| if *w == '\0' { ' ' } else { *w }).collect();
    let state = if c.sqlstate.is_empty() { "00000" } else { c.sqlstate.as_str() };
    Wire {
        sqlcaid: fixed("SQLCA", 0x40),
        sqlcabc: 136,
        sqlcode: u(c.sqlcode),
        sqlerrml: 0,
        sqlerrmc: [0; 70],
        sqlerrp: fixed(if c.sqlerrp.is_empty() { "QSQ00000" } else { c.sqlerrp.as_str() }, 0x40),
        sqlerrd1: u(c.sqlerrd[0]),
        sqlerrd2: u(c.sqlerrd[1]),
        sqlerrd3: u(c.sqlerrd[2]),
        sqlerrd4: u(c.sqlerrd[3]),
        sqlerrd5: u(c.sqlerrd[4]),
        sqlerrd6: u(c.sqlerrd[5]),
        sqlwarn: fixed(&warn, 0x40),
        sqlstate: fixed(state, 0xF0),
    }
    .encode()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_136_bytes() {
        let b = encode(&Sqlca::rows(3));
        assert_eq!(b.len(), 136);
        assert_eq!(&b[..5], &text::ebcdic("SQLCA")[..]);
    }
}
