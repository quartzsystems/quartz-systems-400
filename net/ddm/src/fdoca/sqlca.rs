// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SQLCA group (`SQLCAGRP`) of `SQLCARD`, `SQLDARD` and `QRYDTA`:
//! a null indicator, `SQLCODE`, `SQLSTATE`, `SQLERRPROC`, then the
//! nullable `SQLCAXGRP` (`SQLERRD1`-`6`, `SQLWARN0`-`A`, `SQLRDBNAME`,
//! the message tokens mixed and single-byte) and, from SQLAM level 7,
//! the nullable `SQLDIAGGRP`. Below level 7 `SQLRDBNAME` is 18 fixed
//! characters.

// Short names for the reader, writer, field and value.
#![allow(clippy::many_single_char_names)]

use super::{Reader, Writer};
use crate::Error;
use crate::sql::Sqlca;

/// A null indicator: present.
pub const PRESENT: u8 = 0x00;
/// A null indicator: null.
pub const NULL: u8 = 0xFF;

/// Write an SQLCA group; `None` writes the null group (success with
/// nothing to report).
pub fn write(w: &mut Writer, ca: Option<&Sqlca>, sqlam: u16) {
    let Some(ca) = ca else {
        w.u8(NULL);
        return;
    };
    w.u8(PRESENT);
    w.i32(ca.sqlcode);
    w.fixed(&ca.sqlstate, 5);
    w.fixed(&ca.sqlerrproc, 8);
    w.u8(PRESENT);
    for d in ca.sqlerrd {
        w.i32(d);
    }
    w.bytes(&warn_bytes(w, &ca.sqlwarn));
    if sqlam >= 7 {
        w.vcs(&ca.rdbname);
    } else {
        w.fixed(&ca.rdbname, 18);
    }
    w.vcs("");
    w.vcs(&ca.sqlerrmc);
    if sqlam >= 7 {
        w.u8(NULL);
    }
}

fn warn_bytes(w: &Writer, warn: &[u8; 11]) -> Vec<u8> {
    let text: String = warn.iter().map(|&b| char::from(b)).collect();
    let mut b = w.chars(&text);
    b.resize(11, 0x40);
    b
}

/// Read an SQLCA group; `None` for the null group.
pub fn read(r: &mut Reader<'_>, sqlam: u16) -> Result<Option<Sqlca>, Error> {
    if r.u8()? == NULL {
        return Ok(None);
    }
    let mut ca = Sqlca {
        sqlcode: r.i32()?,
        sqlstate: r.fixed(5)?,
        sqlerrproc: r.fixed(8)?,
        ..Sqlca::default()
    };
    if r.u8()? != NULL {
        for d in &mut ca.sqlerrd {
            *d = r.i32()?;
        }
        let w = r.take(11)?;
        let t = r.text(w);
        for (d, c) in ca.sqlwarn.iter_mut().zip(t.chars()) {
            *d = u8::try_from(c).unwrap_or(b' ');
        }
        ca.rdbname = if sqlam >= 7 {
            r.vcs()?.trim_end().to_string()
        } else {
            r.fixed(18)?
        };
        let m = r.vcs()?;
        let s = r.vcs()?;
        ca.sqlerrmc = if m.is_empty() { s } else { m };
    }
    if sqlam >= 7 && !r.done() && r.u8()? != NULL {
        skip_diag(r)?;
    }
    Ok(Some(ca))
}

/// Skip a present `SQLDIAGGRP`: its status then condition and
/// connection lists, which QS/400 never sends.
fn skip_diag(r: &mut Reader<'_>) -> Result<(), Error> {
    // SQLDIAGSTT is nullable; the lists that follow are counted.
    if r.u8()? != NULL {
        r.take(4 * 4)?;
        r.vcs()?;
        r.vcs()?;
    }
    for _ in 0..2 {
        let n = r.i16()?;
        if n > 0 {
            return Err(Error::Invalid("SQLDIAGGRP lists are not read".into()));
        }
    }
    Ok(())
}

/// The bytes of `SQLCARD` for `ca`.
pub fn sqlcard(ca: Option<&Sqlca>, repr: super::Repr, sqlam: u16) -> Vec<u8> {
    let mut w = Writer::new(repr);
    write(&mut w, ca, sqlam);
    w.out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fdoca::Repr;

    #[test]
    fn round_trip() {
        let mut ca = Sqlca::error(-204, "42704", "NOSUCH");
        ca.rdbname = "QS400".into();
        ca.sqlerrd[2] = 7;
        for sqlam in [6, 7] {
            let b = sqlcard(Some(&ca), Repr::default(), sqlam);
            let mut r = Reader::new(&b, Repr::default());
            assert_eq!(read(&mut r, sqlam).unwrap(), Some(ca.clone()));
            assert!(r.done());
        }
        assert_eq!(sqlcard(None, Repr::default(), 7), vec![NULL]);
    }
}
