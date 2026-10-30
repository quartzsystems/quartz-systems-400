// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Path names at the API boundary: NUL-terminated strings in the job's
//! CCSID, and `Qlg_Path_Name_T` structures that name their CCSID, give
//! the name in place or by pointer, and their own delimiter.

use ifs::Errno;
use mi::Job;

use super::consts::{QLG_CHAR_DOUBLE, QLG_CHAR_SINGLE, QLG_PTR_DOUBLE, QLG_PTR_SINGLE};
use super::layout::{self as lay, QLG_PATH_NAME};
use super::{R, job_ccsid};
use crate::net::abi::{fat, mem, ptr_at};

/// The longest path read from storage.
const MAX_PATH: usize = 64 * 1024;

/// Bytes in a CCSID as text; `ECONVERT` when the CCSID is unknown.
pub(crate) fn decode(ccsid: u16, b: &[u8]) -> R<String> {
    ccsid::conv::to_string(ccsid, b).map_err(|_| Errno::ECONVERT)
}

/// Text in a CCSID.
pub(crate) fn encode(ccsid: u16, s: &str) -> Vec<u8> {
    ccsid::conv::from_str(ccsid, s).unwrap_or_else(|_| ccsid::to_ebcdic(s).unwrap_or_default())
}

/// A NUL-terminated path at a pointer, in the job's CCSID; `EFAULT`
/// for a bad pointer, `ENOENT` for an empty path.
pub(crate) fn cpath(j: &mut Job, p: u128) -> R<String> {
    let (at, avail) = j.spaces.extent(fat(p)).map_err(|_| Errno::EFAULT)?;
    let n = usize::try_from(avail).unwrap_or(usize::MAX).min(MAX_PATH);
    let b = mi::storage::view(at, n);
    let end = b.iter().position(|&c| c == 0).ok_or(Errno::ENAMETOOLONG)?;
    if end == 0 {
        return Err(Errno::ENOENT);
    }
    let ccsid = job_ccsid(j);
    decode(ccsid, &b[..end])
}

/// A `Qlg_Path_Name_T` at a pointer as a path.
pub(crate) fn qlg_path(j: &mut Job, p: u128) -> R<String> {
    let jc = job_ccsid(j);
    let j = &*j;
    qlg_decode(j, jc, &|n| mem(j, p, n).map(|b| b.to_vec()))
}

/// A `Qlg_Path_Name_T` at a native address (a program API's parameter).
pub(crate) fn qlg_path_at(j: &mut Job, addr: u64) -> R<String> {
    if addr == 0 {
        return Err(Errno::EFAULT);
    }
    let jc = job_ccsid(j);
    qlg_decode(j, jc, &|n| Ok(mi::storage::view(addr, n).to_vec()))
}

/// Decode a structure whose first `n` bytes `read` gives.
fn qlg_decode(j: &Job, job_ccsid: u16, read: &dyn Fn(usize) -> R<Vec<u8>>) -> R<String> {
    let head = read(QLG_PATH_NAME.size)?;
    let ccsid = match u16::try_from(lay::get_n(&head, &QLG_PATH_NAME, "CCSID")) {
        Ok(0) => job_ccsid,
        Ok(c) => c,
        Err(_) => return Err(Errno::EINVAL),
    };
    let kind = u32::try_from(lay::get_n(&head, &QLG_PATH_NAME, "Path_Type")).unwrap_or(u32::MAX);
    let len = usize::try_from(lay::get_i(&head, &QLG_PATH_NAME, "Path_Length")).map_err(|_| Errno::EINVAL)?;
    let (delim_at, _) = lay::at(&QLG_PATH_NAME, "Path_Name_Delimiter");
    let delim_len = match kind {
        QLG_CHAR_SINGLE | QLG_PTR_SINGLE => 1,
        QLG_CHAR_DOUBLE | QLG_PTR_DOUBLE => 2,
        _ => return Err(Errno::EINVAL),
    };
    let delim = head[delim_at..delim_at + delim_len].to_vec();
    if len == 0 {
        return Err(Errno::ENOENT);
    }
    if len > MAX_PATH {
        return Err(Errno::ENAMETOOLONG);
    }
    let bytes: Vec<u8> = match kind {
        QLG_CHAR_SINGLE | QLG_CHAR_DOUBLE => read(QLG_PATH_NAME.size + len)?[QLG_PATH_NAME.size..].to_vec(),
        _ => {
            let with_ptr = read(QLG_PATH_NAME.size + 16)?;
            let name = ptr_at(&with_ptr, QLG_PATH_NAME.size);
            mem(j, name, len)?.to_vec()
        }
    };
    let text = decode(ccsid, &bytes)?;
    let d = decode(ccsid, &delim)?;
    Ok(match d.chars().next() {
        Some(c) if c != '/' && c != '\0' => text.replace(c, "/"),
        _ => text,
    })
}

/// A `Qlg_Path_Name_T` of a path with the name in place, in `ccsid`:
/// the structure and the name.
pub(crate) fn qlg_bytes(ccsid: u16, path: &str) -> Vec<u8> {
    let name = encode(ccsid, path);
    let mut b = vec![0u8; QLG_PATH_NAME.size];
    lay::put_n(&mut b, &QLG_PATH_NAME, "CCSID", u64::from(ccsid));
    lay::put_n(&mut b, &QLG_PATH_NAME, "Path_Type", u64::from(QLG_CHAR_SINGLE));
    lay::put_n(&mut b, &QLG_PATH_NAME, "Path_Length", name.len() as u64);
    let slash = encode(ccsid, "/");
    lay::put(&mut b, &QLG_PATH_NAME, "Path_Name_Delimiter", &slash);
    b.extend_from_slice(&name);
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_qlg_name_round_trips_in_any_ccsid() {
        let b = qlg_bytes(1208, "/home/QSTEST/é");
        assert_eq!(lay::get_n(&b, &QLG_PATH_NAME, "Path_Length"), 15);
        assert_eq!(b[20], b'/');
        assert_eq!(decode(1208, &b[32..]).unwrap(), "/home/QSTEST/é");
        let e = qlg_bytes(37, "/a");
        assert_eq!(&e[32..], &[0x61, 0x81]);
        assert_eq!(e[20], 0x61);
    }
}
