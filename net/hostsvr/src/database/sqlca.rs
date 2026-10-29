// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SQLCA a reply carries in `0x3807`: the 136-byte DB2 layout (an
//! empty `0x3807` means none was produced).

use crate::template::template;

template! {
    base 0;
    /// The SQL communication area.
    pub struct Sqlca {
        /// `SQLCA   ` in EBCDIC.
        sqlcaid: [u8; 8],
        /// Its length (136).
        sqlcabc: u32,
        /// SQLCODE (signed).
        sqlcode: u32,
        /// Length of the message tokens.
        sqlerrml: u16,
        /// Message tokens.
        sqlerrmc: [u8; 70],
        /// The product and module that set it.
        sqlerrp: [u8; 8],
        /// Diagnostic word 1.
        sqlerrd1: u32,
        /// Diagnostic word 2.
        sqlerrd2: u32,
        /// Diagnostic word 3 (rows affected).
        sqlerrd3: u32,
        /// Diagnostic word 4.
        sqlerrd4: u32,
        /// Diagnostic word 5.
        sqlerrd5: u32,
        /// Diagnostic word 6.
        sqlerrd6: u32,
        /// Warning flags.
        sqlwarn: [u8; 11],
        /// SQLSTATE (EBCDIC).
        sqlstate: [u8; 5],
    }
}

impl Sqlca {
    /// The SQLCODE as a signed number.
    pub fn code(&self) -> i32 {
        i32::from_be_bytes(self.sqlcode.to_be_bytes())
    }

    /// The SQLSTATE as text.
    pub fn state(&self) -> String {
        ccsid::from_ebcdic(&self.sqlstate)
    }

    /// Rows affected (`sqlerrd3`).
    pub fn rows(&self) -> u32 {
        self.sqlerrd3
    }
}
