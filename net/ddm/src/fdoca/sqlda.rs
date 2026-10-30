// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `SQLDARD` at SQLAM level 7: the SQLCA group, the nullable
//! statement header (`SQLDHGRP`), the column count (`SQLNUM`), then per
//! column `SQLPRECISION`, `SQLSCALE`, `SQLLENGTH` (eight bytes),
//! `SQLTYPE`, `SQLCCSID` and the nullable `SQLDOPTGRP` (unnamed flag,
//! name, label and comments, each mixed then single-byte, then the
//! nullable user-defined type and extended describe groups).

// Short names for the reader, writer, field and value.
#![allow(clippy::many_single_char_names)]

use super::{Reader, Writer, sqlca, types};
use crate::Error;
use crate::sql::{Column, Sqlca};

/// Null indicator: null.
const NULL: u8 = 0xFF;
/// Null indicator: present.
const PRESENT: u8 = 0x00;

/// What an `SQLDARD` held.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sqldard {
    /// The SQLCA (none when the describe succeeded quietly).
    pub sqlca: Option<Sqlca>,
    /// The columns.
    pub columns: Vec<Column>,
}

/// Write an `SQLDARD`.
pub fn write(w: &mut Writer, d: &Sqldard, sqlam: u16) {
    sqlca::write(w, d.sqlca.as_ref(), sqlam);
    w.u8(NULL); // SQLDHGRP
    w.i16(i16::try_from(d.columns.len()).unwrap_or(i16::MAX));
    for c in &d.columns {
        let t = &c.ty;
        w.i16(i16::from(t.precision));
        w.i16(i16::from(t.scale));
        w.i64(i64::from(t.length));
        w.i16(types::sqltype_of(t));
        w.be16(t.ccsid);
        w.u8(PRESENT); // SQLDOPTGRP
        w.i16(0);
        w.vcs("");
        w.vcs(&c.name);
        w.vcs("");
        w.vcs(c.label.as_deref().unwrap_or(""));
        w.vcs("");
        w.vcs("");
        w.u8(NULL); // SQLUDTGRP
        w.u8(NULL); // SQLDXGRP
    }
}

/// Read an `SQLDARD`.
pub fn read(r: &mut Reader<'_>, sqlam: u16) -> Result<Sqldard, Error> {
    let ca = sqlca::read(r, sqlam)?;
    if r.u8()? != NULL {
        for _ in 0..6 {
            r.i16()?;
        }
        r.vcs()?;
        r.vcs()?;
        r.vcs()?;
    }
    let n = r.i16()?;
    let mut columns = Vec::new();
    for _ in 0..n.max(0) {
        let precision = r.i16()?;
        let scale = r.i16()?;
        let length = r.i64()?;
        let sqltype = r.i16()?;
        let ccsid = r.be16()?;
        let mut name = String::new();
        let mut label = None;
        if r.u8()? != NULL {
            r.i16()?;
            let m = r.vcs()?;
            let s = r.vcs()?;
            name = if m.is_empty() { s } else { m };
            let m = r.vcs()?;
            let s = r.vcs()?;
            let l = if m.is_empty() { s } else { m };
            if !l.trim().is_empty() {
                label = Some(l);
            }
            r.vcs()?;
            r.vcs()?;
            if r.u8()? != NULL {
                r.i32()?;
                for _ in 0..5 {
                    r.vcs()?;
                }
            }
            if r.u8()? != NULL {
                for _ in 0..4 {
                    r.i16()?;
                }
                for _ in 0..9 {
                    r.vcs()?;
                }
            }
        }
        let ty = types::col_of_sqltype(sqltype, precision, scale, length, ccsid)
            .ok_or_else(|| Error::Invalid(format!("SQLTYPE {sqltype}")))?;
        columns.push(Column { name, ty, label });
    }
    Ok(Sqldard { sqlca: ca, columns })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fdoca::Repr;
    use crate::sql::{ColType, Kind};

    #[test]
    fn round_trip() {
        let d = Sqldard {
            sqlca: None,
            columns: vec![
                Column {
                    name: "CUSNUM".into(),
                    ty: ColType::new(Kind::Numeric, 0, 6, 0, false),
                    label: None,
                },
                Column {
                    name: "LSTNAM".into(),
                    ty: ColType::new(Kind::Char, 8, 0, 0, true),
                    label: Some("Last name".into()),
                },
            ],
        };
        let mut w = Writer::new(Repr::default());
        write(&mut w, &d, 7);
        let mut r = Reader::new(&w.out, Repr::default());
        assert_eq!(read(&mut r, 7).unwrap(), d);
        assert!(r.done());
    }
}
