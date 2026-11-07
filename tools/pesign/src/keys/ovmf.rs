// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A test aid: an OVMF variable store (`OVMF_VARS*.fd`) with a signature
//! list appended to its `db`, so a VM boots with the QS/400 key already
//! enrolled (the check `INSTALL.md` §6 records). The store's layout is
//! EDK2's authenticated variable store: a firmware volume header, the
//! variable store header, then variables of a 60-byte header, the
//! UTF-16 name and the data, each 4-byte aligned.

use crate::Error;

/// A variable that is in use.
const VAR_ADDED: u8 = 0x3F;
/// Its state once replaced.
const VAR_DELETED: u8 = 0x3D;
/// `StartId` of each variable.
const START: u16 = 0x55AA;
/// The authenticated variable header's size.
const HEADER: usize = 60;

fn le16(b: &[u8], o: usize) -> Result<usize, Error> {
    b.get(o..o + 2)
        .map(|s| usize::from(u16::from_le_bytes([s[0], s[1]])))
        .ok_or(Error::Pe("truncated variable store"))
}

fn le32(b: &[u8], o: usize) -> Result<usize, Error> {
    b.get(o..o + 4)
        .and_then(|s| usize::try_from(u32::from_le_bytes([s[0], s[1], s[2], s[3]])).ok())
        .ok_or(Error::Pe("truncated variable store"))
}

/// `vars` with `esl` appended to the live `db` variable: a new copy of
/// it in the free space, the old one marked deleted.
pub fn add_db(vars: &[u8], esl: &[u8]) -> Result<Vec<u8>, Error> {
    let mut d = vars.to_vec();
    let store = le16(&d, 0x30)?;
    let end = store + le32(&d, store + 16)?;
    let mut off = store + 28;
    let mut db = None;
    let name: Vec<u8> = "db\0".encode_utf16().flat_map(u16::to_le_bytes).collect();
    while off + HEADER <= end.min(d.len()) && le16(&d, off)? == usize::from(START) {
        let ns = le32(&d, off + 36)?;
        let ds = le32(&d, off + 40)?;
        if d[off + 2] == VAR_ADDED && d.get(off + HEADER..off + HEADER + ns) == Some(&name[..]) {
            db = Some((off, ns, ds));
        }
        off = (off + HEADER + ns + ds + 3) & !3;
    }
    let (at, ns, ds) = db.ok_or(Error::Pe(
        "the variable store has no db: start from a store with keys enrolled",
    ))?;
    let mut new = d[at..at + HEADER + ns + ds].to_vec();
    new.extend_from_slice(esl);
    let size = u32::try_from(ds + esl.len()).map_err(|_| Error::Pe("db too large"))?;
    new[40..44].copy_from_slice(&size.to_le_bytes());
    if off + new.len() > end.min(d.len()) {
        return Err(Error::Pe("no room left in the variable store"));
    }
    d[off..off + new.len()].copy_from_slice(&new);
    d[at + 2] = VAR_DELETED;
    Ok(d)
}
