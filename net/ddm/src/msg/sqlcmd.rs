// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SQL commands and their statement object. `SQLSTT` holds the
//! statement as a nullable mixed string then a nullable single-byte
//! string, each with a four-byte length: QS/400 sends the single-byte
//! part in the `CCSIDSBC` of `ACCRDB`.

use crate::cp::{cmd, obj, param};
use crate::msg::pkg::Pkgnamcsn;
use crate::wire::Object;
use crate::{Error, be16};

/// The default query block size.
pub const QRYBLKSZ: u32 = 32_767;

/// `SQLSTT` of a statement.
pub fn sqlstt(text: &str) -> Object {
    Object::data(obj::SQLSTT, nocs(&crate::wire::ebcdic(text)))
}

/// `SQLATTR` of attribute text (`FOR READ ONLY`).
pub fn sqlattr(text: &str) -> Object {
    Object::data(obj::SQLATTR, nocs(&crate::wire::ebcdic(text)))
}

/// A null mixed string then a single-byte one.
fn nocs(b: &[u8]) -> Vec<u8> {
    let mut d = vec![0xFF, 0x00];
    d.extend(u32::try_from(b.len()).unwrap_or(0).to_be_bytes());
    d.extend_from_slice(b);
    d
}

/// The statement bytes of an `SQLSTT` and whether they are the mixed
/// part; two- and four-byte lengths are both read.
pub fn sqlstt_bytes(b: &[u8]) -> Result<(Vec<u8>, bool), Error> {
    let bad = || Error::Invalid("SQLSTT does not read".into());
    let mut at = 0;
    for mixed in [true, false] {
        let ind = *b.get(at).ok_or_else(bad)?;
        at += 1;
        if ind == 0xFF {
            continue;
        }
        // Four-byte length when the two-byte reading would not fit.
        let rest = &b[at..];
        let (len, head) = if rest.len() >= 4 {
            let l4 = u32::from_be_bytes([rest[0], rest[1], rest[2], rest[3]]) as usize;
            if 4 + l4 <= rest.len() {
                (l4, 4)
            } else {
                (usize::from(be16(rest, 0)), 2)
            }
        } else if rest.len() >= 2 {
            (usize::from(be16(rest, 0)), 2)
        } else {
            return Err(bad());
        };
        let start = at + head;
        let data = b.get(start..start + len).ok_or_else(bad)?;
        return Ok((data.to_vec(), mixed));
    }
    Err(bad())
}

/// `PRPSQLSTT`, asking for the SQLDA when `describe`.
pub fn prpsqlstt(pkg: &Pkgnamcsn, describe: bool) -> Object {
    let mut v = vec![pkg.to_object()];
    if describe {
        v.push(Object::byte(param::RTNSQLDA, 0xF1));
    }
    Object::coll(cmd::PRPSQLSTT, v)
}

/// `DSCSQLSTT`: describe a prepared statement's output.
pub fn dscsqlstt(pkg: &Pkgnamcsn) -> Object {
    Object::coll(cmd::DSCSQLSTT, vec![pkg.to_object()])
}

/// `OPNQRY`.
pub fn opnqry(pkg: &Pkgnamcsn, qryblksz: u32) -> Object {
    Object::coll(
        cmd::OPNQRY,
        vec![pkg.to_object(), Object::u32(param::QRYBLKSZ, qryblksz)],
    )
}

/// `CNTQRY`.
pub fn cntqry(pkg: &Pkgnamcsn, qryblksz: u32, qryinsid: Option<&[u8]>) -> Object {
    let mut v = vec![pkg.to_object(), Object::u32(param::QRYBLKSZ, qryblksz)];
    if let Some(id) = qryinsid {
        v.push(Object::data(param::QRYINSID, id.to_vec()));
    }
    Object::coll(cmd::CNTQRY, v)
}

/// `CLSQRY`.
pub fn clsqry(pkg: &Pkgnamcsn, qryinsid: Option<&[u8]>) -> Object {
    let mut v = vec![pkg.to_object()];
    if let Some(id) = qryinsid {
        v.push(Object::data(param::QRYINSID, id.to_vec()));
    }
    Object::coll(cmd::CLSQRY, v)
}

/// `EXCSQLIMM`.
pub fn excsqlimm(pkg: &Pkgnamcsn) -> Object {
    Object::coll(cmd::EXCSQLIMM, vec![pkg.to_object()])
}

/// `EXCSQLSTT`.
pub fn excsqlstt(pkg: &Pkgnamcsn) -> Object {
    Object::coll(cmd::EXCSQLSTT, vec![pkg.to_object()])
}

/// `BGNBND`: start binding a package.
pub fn bgnbnd(pkg: &Pkgnamcsn) -> Object {
    Object::coll(cmd::BGNBND, vec![pkg.to_pkgnamct()])
}

/// `BNDSQLSTT`: bind one statement at the package's section; the
/// statement follows in `SQLSTT`.
pub fn bndsqlstt(pkg: &Pkgnamcsn) -> Object {
    Object::coll(cmd::BNDSQLSTT, vec![pkg.to_object()])
}

/// `ENDBND`: end the bind.
pub fn endbnd(pkg: &Pkgnamcsn) -> Object {
    Object::coll(cmd::ENDBND, vec![pkg.to_pkgnamct()])
}

/// `RDBCMM`.
pub fn rdbcmm() -> Object {
    Object::coll(cmd::RDBCMM, Vec::new())
}

/// `RDBRLLBCK`.
pub fn rdbrllbck() -> Object {
    Object::coll(cmd::RDBRLLBCK, Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statement_round_trip() {
        let o = sqlstt("SELECT 1 FROM SYSIBM.SYSDUMMY1");
        let (b, mixed) = sqlstt_bytes(o.bytes()).unwrap();
        assert!(!mixed);
        assert_eq!(crate::wire::text(&b), "SELECT 1 FROM SYSIBM.SYSDUMMY1");
    }
}
