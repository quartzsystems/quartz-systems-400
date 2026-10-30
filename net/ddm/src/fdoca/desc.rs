// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! FD:OCA descriptors (`QRYDSC`, `FDODSC`): a run of triplets, each a
//! length byte, a type and an ID.
//!
//! - A meta data definition (`MDD`, `0x78`) says what the next triplet
//!   stands for: `05 01 01 T` a scalar of DRDA type `T`, `05 02 01 D0`
//!   the column group, `05 03 01 E0` the row, `05 04 01 F0` the set.
//! - A scalar data array (`SDA`, `0x70`) defines local type `ID` as an
//!   FD:OCA type, a CCSID, a character size, a mode and a length.
//! - A group data array (`GDA`, `0x76`, continued by `0x7F`) lists the
//!   columns as a local ID and a length. An ID below `0x80` is DRDA's
//!   early descriptor of that DRDA type (`0x02` an integer, `0x0F` a
//!   nullable decimal); `0x80` and up are the SDAs defined before.
//! - A row layout (`RLO`, `0x71`) repeats groups by their IDs.
//!
//! IBM i describes character columns (dates and times among them)
//! through an MDD and SDA pair and numbers by their early descriptors
//! (the pub400 traces); QS/400 writes the same form.

use crate::Error;
use crate::fdoca::types::{self, drda};
use crate::sql::ColType;

/// Triplet type: meta data definition.
pub const MDD: u8 = 0x78;
/// Triplet type: scalar data array.
pub const SDA: u8 = 0x70;
/// Triplet type: group data array.
pub const GDA: u8 = 0x76;
/// Triplet type: continued group data array.
pub const CPT: u8 = 0x7F;
/// Triplet type: row layout.
pub const RLO: u8 = 0x71;
/// Local ID: the column group.
pub const SQLDTAGRP: u8 = 0xD0;
/// Local ID: the SQLCA group.
pub const SQLCAGRP: u8 = 0x54;
/// Local ID: the row of SQLCA and columns.
pub const SQLCADTA: u8 = 0xE0;
/// Local ID: the answer set.
pub const SQLDTARD: u8 = 0xF0;
/// The first local ID an SDA may take.
pub const FIRST_LID: u8 = 0x80;

/// The most columns one GDA triplet holds (3 + 3 × 84 = 255).
pub const GDA_MAX: usize = 84;

/// FD:OCA data types (the high bit makes them nullable).
pub mod fd {
    consts! {
        /// Fixed bytes.
        FIXEDBYTES = 0x01,
        /// Varying bytes.
        VARBYTES = 0x02,
        /// Fixed single-byte characters.
        FIXEDCHAR = 0x10,
        /// Varying single-byte characters.
        VARCHAR = 0x11,
        /// Varying mixed characters.
        VARMIX = 0x19,
        /// Big-endian integer.
        INTEGER_BE = 0x23,
        /// Little-endian integer.
        INTEGER_LE = 0x24,
        /// Packed decimal.
        DECIMAL = 0x30,
        /// Zoned decimal.
        ZONED = 0x33,
        /// IEEE float.
        FLOAT_IEEE = 0x48,
    }
}

/// One triplet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Triplet {
    /// Its type.
    pub ttype: u8,
    /// Its local ID.
    pub id: u8,
    /// What follows.
    pub data: Vec<u8>,
}

/// Every triplet of `b`.
pub fn triplets(b: &[u8]) -> Result<Vec<Triplet>, Error> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < b.len() {
        let len = usize::from(b[at]);
        if len < 3 || at + len > b.len() {
            return Err(Error::Invalid(format!("FD:OCA triplet length {len} at {at}")));
        }
        out.push(Triplet {
            ttype: b[at + 1],
            id: b[at + 2],
            data: b[at + 3..at + len].to_vec(),
        });
        at += len;
    }
    Ok(out)
}

/// The encoding of triplets.
pub fn encode(t: &[Triplet]) -> Vec<u8> {
    let mut v = Vec::new();
    for x in t {
        v.push(u8::try_from(3 + x.data.len()).unwrap_or(u8::MAX));
        v.push(x.ttype);
        v.push(x.id);
        v.extend_from_slice(&x.data);
    }
    v
}

/// One described column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Field {
    /// The DRDA type (odd: nullable).
    pub drda: u8,
    /// The CCSID of character data (0 for others).
    pub ccsid: u16,
    /// The length (precision and scale for decimals; the most bytes for
    /// varying strings).
    pub len: u16,
}

impl Field {
    /// Of a column type.
    pub fn of(t: &ColType) -> Self {
        let (drda, len) = types::drda_of(t);
        Self {
            drda,
            ccsid: t.ccsid,
            len,
        }
    }

    /// The column type.
    pub fn col(&self) -> Option<ColType> {
        let mut t = types::col_of(self.drda, self.len)?;
        if self.ccsid != 0 {
            t.ccsid = self.ccsid;
        }
        Some(t)
    }

    /// Whether the value carries a null indicator.
    pub fn nullable(&self) -> bool {
        self.drda & 1 == 1
    }

    /// The FD:OCA type that carries it, without the null bit.
    fn fdoca(self) -> u8 {
        let t = match u16::from(self.drda & !1) {
            drda::SMALL | drda::INTEGER | drda::INTEGER8 => fd::INTEGER_BE,
            drda::FLOAT8 | drda::FLOAT4 => fd::FLOAT_IEEE,
            drda::DECIMAL => fd::DECIMAL,
            drda::ZDECIMAL => fd::ZONED,
            drda::VARCHAR | drda::LONG => fd::VARCHAR,
            drda::VARMIX | drda::LONGMIX => fd::VARMIX,
            drda::VARBYTE => fd::VARBYTES,
            drda::FIXBYTE => fd::FIXEDBYTES,
            _ => fd::FIXEDCHAR,
        };
        u8::try_from(t).unwrap_or(0)
    }
}

/// What an SDA defines.
#[derive(Clone, Copy)]
struct Local {
    drda: u8,
    ccsid: u16,
}

/// The columns of a descriptor's column group, in order.
pub fn fields(b: &[u8]) -> Result<Vec<Field>, Error> {
    let mut out = Vec::new();
    let mut locals: Vec<(u8, Local)> = Vec::new();
    let mut pending_drda: Option<u8> = None;
    let mut in_group = false;
    for t in triplets(b)? {
        match t.ttype {
            MDD => {
                pending_drda = match t.data.as_slice() {
                    [_, 1, _, d] => Some(*d),
                    _ => None,
                };
                in_group = false;
            }
            SDA => {
                let fdt = t.data.first().copied().unwrap_or(0);
                let ccsid = t
                    .data
                    .get(1..5)
                    .map_or(0, |c| u16::try_from(u32::from_be_bytes([c[0], c[1], c[2], c[3]])).unwrap_or(0));
                let drda = pending_drda
                    .take()
                    .ok_or_else(|| Error::Invalid(format!("SDA of FD:OCA type {fdt:#04x} without an MDD")))?;
                locals.retain(|(id, _)| *id != t.id);
                locals.push((t.id, Local { drda, ccsid }));
                in_group = false;
            }
            GDA => in_group = t.id == SQLDTAGRP,
            CPT => {}
            _ => in_group = false,
        }
        if matches!(t.ttype, GDA | CPT) && in_group {
            for c in t.data.as_chunks::<3>().0 {
                let len = u16::from_be_bytes([c[1], c[2]]);
                let f = if let Some((_, l)) = locals.iter().find(|(id, _)| *id == c[0]) {
                    Field {
                        drda: l.drda,
                        ccsid: l.ccsid,
                        len,
                    }
                } else {
                    let probe = Field { drda: c[0], ccsid: 0, len };
                    let ccsid = probe.col().map_or(0, |t| t.ccsid);
                    Field { drda: c[0], ccsid, len }
                };
                out.push(f);
            }
        }
    }
    Ok(out)
}

/// The `QRYDSC` (or `FDODSC`) of `fields`: an MDD and SDA per distinct
/// type, the column group, the row and the set.
pub fn qrydsc(fields: &[Field]) -> Vec<u8> {
    let mut t = Vec::new();
    let mut lids: Vec<(u8, u8, u16)> = Vec::new();
    let mut refs = Vec::with_capacity(fields.len());
    for f in fields {
        if f.ccsid == 0 {
            refs.push((f.drda, f.len));
            continue;
        }
        let fdt = f.fdoca() | if f.nullable() { 0x80 } else { 0 };
        let lid = if let Some(&(lid, _, _)) = lids.iter().find(|(_, d, c)| *d == f.drda && *c == f.ccsid) {
            lid
        } else {
            let lid = FIRST_LID + u8::try_from(lids.len()).unwrap_or(0);
            lids.push((lid, f.drda, f.ccsid));
            t.push(Triplet {
                ttype: MDD,
                id: 0x00,
                data: vec![0x05, 0x01, 0x01, f.drda],
            });
            let mut d = vec![fdt];
            d.extend(u32::from(f.ccsid).to_be_bytes());
            d.push(1);
            d.push(u8::from(matches!(f.fdoca(), 0x11 | 0x19)));
            d.extend(0x7FFFu16.to_be_bytes());
            t.push(Triplet {
                ttype: SDA,
                id: lid,
                data: d,
            });
            lid
        };
        refs.push((lid, f.len));
    }
    t.push(Triplet {
        ttype: MDD,
        id: 0x00,
        data: vec![0x05, 0x02, 0x01, SQLDTAGRP],
    });
    let chunks: Vec<&[(u8, u16)]> = if refs.is_empty() {
        vec![&[]]
    } else {
        refs.chunks(GDA_MAX).collect()
    };
    for (i, chunk) in chunks.into_iter().enumerate() {
        let mut d = Vec::with_capacity(chunk.len() * 3);
        for (lid, len) in chunk {
            d.push(*lid);
            d.extend(len.to_be_bytes());
        }
        t.push(Triplet {
            ttype: if i == 0 { GDA } else { CPT },
            id: if i == 0 { SQLDTAGRP } else { 0x00 },
            data: d,
        });
    }
    t.push(Triplet {
        ttype: MDD,
        id: 0x00,
        data: vec![0x05, 0x03, 0x01, SQLCADTA],
    });
    t.push(Triplet {
        ttype: RLO,
        id: SQLCADTA,
        data: vec![SQLCAGRP, 0x00, 0x01, SQLDTAGRP, 0x00, 0x01],
    });
    t.push(Triplet {
        ttype: MDD,
        id: 0x00,
        data: vec![0x05, 0x04, 0x01, SQLDTARD],
    });
    t.push(Triplet {
        ttype: RLO,
        id: SQLDTARD,
        data: vec![SQLCADTA, 0x00, 0x00],
    });
    encode(&t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sql::Kind;

    #[test]
    fn qrydsc_lists_its_fields() {
        let f: Vec<Field> = [
            ColType::new(Kind::Integer, 0, 10, 0, false),
            ColType::new(Kind::Decimal, 0, 7, 2, true),
            ColType::new(Kind::VarChar, 40, 0, 0, true),
            ColType::new(Kind::VarChar, 10, 0, 0, true),
        ]
        .iter()
        .map(Field::of)
        .collect();
        let d = qrydsc(&f);
        assert_eq!(fields(&d).unwrap(), f);
        assert_eq!(f[1].len, 0x0702);
        let many = vec![f[0]; 200];
        assert_eq!(fields(&qrydsc(&many)).unwrap().len(), 200);
    }

    #[test]
    fn reads_the_pub400_form() {
        // QRYDSC of VARCHAR(128), VARCHAR(128), CHAR(1) from pub400.
        let b = [
            0x07, 0x78, 0x00, 0x05, 0x01, 0x01, 0x32, 0x0c, 0x70, 0x80, 0x11, 0x00, 0x00, 0x00, 0x25,
            0x01, 0x01, 0x7f, 0xff, 0x07, 0x78, 0x00, 0x05, 0x01, 0x01, 0x30, 0x0c, 0x70, 0x81, 0x10,
            0x00, 0x00, 0x00, 0x25, 0x01, 0x00, 0x7f, 0xff, 0x07, 0x78, 0x00, 0x05, 0x02, 0x01, 0xd0,
            0x0c, 0x76, 0xd0, 0x80, 0x00, 0x80, 0x80, 0x00, 0x80, 0x81, 0x00, 0x01, 0x07, 0x78, 0x00,
            0x05, 0x03, 0x01, 0xe0, 0x09, 0x71, 0xe0, 0x54, 0x00, 0x01, 0xd0, 0x00, 0x01, 0x07, 0x78,
            0x00, 0x05, 0x04, 0x01, 0xf0, 0x06, 0x71, 0xf0, 0xe0, 0x00, 0x00,
        ];
        let f = fields(&b).unwrap();
        assert_eq!(
            f,
            vec![
                Field { drda: 0x32, ccsid: 37, len: 128 },
                Field { drda: 0x32, ccsid: 37, len: 128 },
                Field { drda: 0x30, ccsid: 37, len: 1 },
            ]
        );
    }
}
