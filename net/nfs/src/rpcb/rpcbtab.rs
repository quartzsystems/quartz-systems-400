// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/etc/RPCBTAB` as the oracle writes it: 357-byte records, each the
//! CCSID 37 text `program version netid uaddr owner`, a new line
//! (`X'15'`), then zeros. `RTVRPCREG(*YES)` reads it back.

use crate::types::rpcb::rpcb;

/// The length of a record.
pub const RECORD: usize = 357;

const NEWLINE: u8 = 0x15;

/// Why a table cannot be read or written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RpcbtabError {
    /// The file is not whole records.
    #[error("{0} bytes is not a number of 357-byte records")]
    Length(usize),
    /// A record without its new line.
    #[error("record {0} has no new line")]
    NoNewline(usize),
    /// A record whose text is not five fields of the right kinds.
    #[error("record {0} is not `program version netid uaddr owner`")]
    Fields(usize),
    /// A registration whose text does not fit a record or CCSID 37.
    #[error("registration {0} cannot be written")]
    Unwritable(usize),
}

/// The text of a registration.
fn text(r: &rpcb) -> Option<String> {
    let s = |b: &[u8]| {
        std::str::from_utf8(b)
            .ok()
            .filter(|s| !s.is_empty() && !s.contains(char::is_whitespace))
            .map(str::to_string)
    };
    Some(format!(
        "{} {} {} {} {}",
        r.r_prog,
        r.r_vers,
        s(&r.r_netid)?,
        s(&r.r_addr)?,
        s(&r.r_owner)?
    ))
}

/// The table holding `entries`.
pub fn encode(entries: &[rpcb]) -> Result<Vec<u8>, RpcbtabError> {
    let mut out = Vec::with_capacity(entries.len() * RECORD);
    for (i, r) in entries.iter().enumerate() {
        let t = text(r).ok_or(RpcbtabError::Unwritable(i))?;
        let mut rec = ccsid::to_ebcdic(&t).map_err(|_| RpcbtabError::Unwritable(i))?;
        rec.push(NEWLINE);
        if rec.len() > RECORD {
            return Err(RpcbtabError::Unwritable(i));
        }
        rec.resize(RECORD, 0);
        out.extend_from_slice(&rec);
    }
    Ok(out)
}

/// The registrations in a table.
pub fn decode(b: &[u8]) -> Result<Vec<rpcb>, RpcbtabError> {
    if !b.len().is_multiple_of(RECORD) {
        return Err(RpcbtabError::Length(b.len()));
    }
    let mut out = Vec::with_capacity(b.len() / RECORD);
    for (i, rec) in b.chunks(RECORD).enumerate() {
        let end = rec
            .iter()
            .position(|x| *x == NEWLINE)
            .ok_or(RpcbtabError::NoNewline(i))?;
        let t = ccsid::from_ebcdic(rec.get(..end).unwrap_or_default());
        let f: Vec<&str> = t.split(' ').collect();
        let [prog, vers, netid, addr, owner] = f.as_slice() else {
            return Err(RpcbtabError::Fields(i));
        };
        let num = |s: &str| s.parse::<u32>().map_err(|_| RpcbtabError::Fields(i));
        if [netid, addr, owner]
            .iter()
            .any(|s| s.is_empty() || !s.is_ascii() || s.contains(char::is_whitespace))
        {
            return Err(RpcbtabError::Fields(i));
        }
        out.push(rpcb {
            r_prog: num(prog)?,
            r_vers: num(vers)?,
            r_netid: netid.as_bytes().to_vec(),
            r_addr: addr.as_bytes().to_vec(),
            r_owner: owner.as_bytes().to_vec(),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg(prog: u32, vers: u32, netid: &str, addr: &str, owner: &str) -> rpcb {
        rpcb {
            r_prog: prog,
            r_vers: vers,
            r_netid: netid.into(),
            r_addr: addr.into(),
            r_owner: owner.into(),
        }
    }

    #[test]
    fn a_record_as_the_oracle_writes_it() {
        let b = encode(&[reg(100_000, 2, "tcp", "::.0.111", "0")]).unwrap();
        assert_eq!(b.len(), RECORD);
        // "100000 2 tcp ::.0.111 0" in CCSID 37, X'15', zeros.
        let head = [
            0xf1, 0xf0, 0xf0, 0xf0, 0xf0, 0xf0, 0x40, 0xf2, 0x40, 0xa3, 0x83, 0x97, 0x40, 0x7a,
            0x7a, 0x4b, 0xf0, 0x4b, 0xf1, 0xf1, 0xf1, 0x40, 0xf0, 0x15,
        ];
        assert_eq!(&b[..head.len()], &head);
        assert!(b[head.len()..].iter().all(|x| *x == 0));
        assert_eq!(
            decode(&b).unwrap(),
            vec![reg(100_000, 2, "tcp", "::.0.111", "0")]
        );
    }

    #[test]
    fn bad_tables() {
        assert_eq!(decode(&[0; 10]), Err(RpcbtabError::Length(10)));
        assert_eq!(decode(&[0; RECORD]), Err(RpcbtabError::NoNewline(0)));
        let mut r = ccsid::to_ebcdic("100000 x tcp ::.0.111 0").unwrap();
        r.push(NEWLINE);
        r.resize(RECORD, 0);
        assert_eq!(decode(&r), Err(RpcbtabError::Fields(0)));
        assert_eq!(
            encode(&[reg(1, 1, "tcp", "a b", "0")]),
            Err(RpcbtabError::Unwritable(0))
        );
        assert_eq!(
            encode(&[reg(1, 1, "tcp", &"9".repeat(400), "0")]),
            Err(RpcbtabError::Unwritable(0))
        );
    }
}
