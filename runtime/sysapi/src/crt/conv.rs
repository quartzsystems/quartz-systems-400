// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QtqIconvOpen`, `iconv_open`, `iconv`, `iconv_close` over the
//! `ccsid` crate's converters, and `_C_IFS_tmpnam`. An `iconv_t` is 52
//! bytes: the return value (0, or -1 when the CCSIDs cannot be
//! converted) and twelve words; QS/400 keeps a mark and the two CCSIDs
//! in the first three, so a descriptor is only data.

use ccsid::conv::Converter;
use codegen::abi::Env;
use ifs::Errno;
use mi::Job;

use crate::net::Stable;
use crate::net::abi::{bound_c, fail, fat, is_null, mem, ptr_at, put_ptr, raw};

/// The bytes of an `iconv_t`.
const CD_LEN: usize = 52;
/// The mark of a descriptor QS/400 opened: `QSCV`.
const MARK: u32 = 0xD8E2_C3E5;

/// A CCSID as a program names it: 0 is the job's.
fn ccsid_of(j: &mut Job, c: u32) -> u16 {
    match u16::try_from(c) {
        Ok(0) | Err(_) => crate::ifs::job_ccsid(j),
        Ok(c) => c,
    }
}

/// Write a descriptor into the caller's return buffer; `None`, or
/// CCSIDs that cannot be converted, is the failed one.
fn put_cd(retbuf: i64, pair: Option<(u16, u16)>) {
    let Ok(addr) = u64::try_from(retbuf) else { return };
    if addr == 0 {
        return;
    }
    let b = mi::storage::view(addr, CD_LEN);
    b.fill(0);
    let (from, to) = pair.unwrap_or((0, 0));
    let ok = pair.is_some() && Converter::new(from, to).is_ok();
    b[0..4].copy_from_slice(&(if ok { 0i32 } else { -1 }).to_be_bytes());
    if ok {
        b[4..8].copy_from_slice(&MARK.to_be_bytes());
        b[8..12].copy_from_slice(&u32::from(from).to_be_bytes());
        b[12..16].copy_from_slice(&u32::from(to).to_be_bytes());
    }
}

/// `QtqIconvOpen(to, from)`: two `QtqCode_T`, the CCSID first.
#[allow(clippy::too_many_arguments)]
extern "sysv64" fn b_qtqiconvopen(env: *mut Env, _argc: i64, _od: i64, retbuf: i64, to: u128, from: u128) {
    let j = mi::storage::job_of(env);
    let word = |j: &Job, p: u128| mem(j, p, 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    match (word(j, to), word(j, from)) {
        (Ok(t), Ok(f)) => {
            let (t, f) = (ccsid_of(j, t), ccsid_of(j, f));
            put_cd(retbuf, Some((f, t)));
        }
        _ => {
            j.errno.set(Errno::EFAULT.code());
            put_cd(retbuf, None);
        }
    }
}

/// The CCSID of an `iconv_open` code: `IBMCCSID` and five digits.
fn code_ccsid(j: &mut Job, p: u128) -> Option<u16> {
    let b = mem(j, p, 13).ok()?;
    let s = ccsid::from_ebcdic(b);
    let digits = s.strip_prefix("IBMCCSID")?.get(..5)?;
    let c: u32 = digits.parse().ok()?;
    Some(ccsid_of(j, c))
}

/// `iconv_open(tocode, fromcode)`.
#[allow(clippy::too_many_arguments)]
extern "sysv64" fn b_iconv_open(env: *mut Env, _argc: i64, _od: i64, retbuf: i64, to: u128, from: u128) {
    let j = mi::storage::job_of(env);
    match (code_ccsid(j, to), code_ccsid(j, from)) {
        (Some(t), Some(f)) => put_cd(retbuf, Some((f, t))),
        _ => {
            j.errno.set(Errno::EINVAL.code());
            put_cd(retbuf, None);
        }
    }
}

/// The CCSIDs of a descriptor at a native address.
fn cd_of(cd: i64) -> Option<(u16, u16)> {
    let addr = u64::try_from(cd).ok().filter(|a| *a != 0)?;
    let b = mi::storage::view(addr, CD_LEN);
    let w = |o: usize| u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
    if w(4) != MARK {
        return None;
    }
    Some((u16::try_from(w(8)).ok()?, u16::try_from(w(12)).ok()?))
}

/// `iconv(cd, &in, &inleft, &out, &outleft)`: as much of the input as
/// fits; the pointers and counts move on.
fn iconv(j: &mut Job, cd: i64, inp: u128, inleft: u128, outp: u128, outleft: u128) -> i64 {
    let r = (|| -> Result<i64, Errno> {
        let (from, to) = cd_of(cd).ok_or(Errno::EBADF)?;
        if is_null(inp) {
            return Ok(0);
        }
        let in_slot = mem(j, inp, 16)?;
        let out_slot = mem(j, outp, 16)?;
        let in_ptr = ptr_at(in_slot, 0);
        let out_ptr = ptr_at(out_slot, 0);
        let in_n = mem(j, inleft, 4)?;
        let out_n = mem(j, outleft, 4)?;
        let n_in = u32::from_be_bytes([in_n[0], in_n[1], in_n[2], in_n[3]]) as usize;
        let n_out = u32::from_be_bytes([out_n[0], out_n[1], out_n[2], out_n[3]]) as usize;
        let input = mem(j, in_ptr, n_in)?.to_vec();
        let mut conv = Converter::new(from, to).map_err(|_| Errno::EINVAL)?;
        let (mut out, mut done, mut out_done) = (Vec::new(), 0usize, 0usize);
        let mut big = false;
        for (i, &b) in input.iter().enumerate() {
            let mut piece = Vec::new();
            conv.feed(&[b], &mut piece);
            if out.len() + piece.len() > n_out {
                big = true;
                break;
            }
            out.extend_from_slice(&piece);
            if conv.pending() == 0 {
                done = i + 1;
                out_done = out.len();
            }
        }
        mem(j, out_ptr, out_done)?.copy_from_slice(&out[..out_done]);
        let new_in = j.spaces.add(fat(in_ptr), i64::try_from(done).unwrap_or(0)).map_err(|_| Errno::EFAULT)?;
        let new_out = j.spaces.add(fat(out_ptr), i64::try_from(out_done).unwrap_or(0)).map_err(|_| Errno::EFAULT)?;
        put_ptr(mem(j, inp, 16)?, 0, new_in);
        put_ptr(mem(j, outp, 16)?, 0, new_out);
        let left_in = u32::try_from(n_in - done).unwrap_or(0);
        let left_out = u32::try_from(n_out - out_done).unwrap_or(0);
        mem(j, inleft, 4)?.copy_from_slice(&left_in.to_be_bytes());
        mem(j, outleft, 4)?.copy_from_slice(&left_out.to_be_bytes());
        if big {
            Err(Errno::E2BIG)
        } else if done < n_in {
            Err(Errno::EINVAL)
        } else {
            Ok(0)
        }
    })();
    match r {
        Ok(v) => v,
        Err(e) => fail(j, e),
    }
}

/// `iconv_close(cd)`.
fn iconv_close(j: &mut Job, cd: i64) -> i64 {
    if cd_of(cd).is_some() { 0 } else { fail(j, Errno::EBADF) }
}

/// `_C_IFS_tmpnam(buf)`: a name in `/tmp` no other call gave, in `buf`
/// when one is passed, else in storage the job keeps.
fn tmpnam(j: &mut Job, buf: u128) -> u128 {
    let n = super::with_state(j, |s| {
        s.tmpcount += 1;
        s.tmpcount
    });
    let name = format!("/tmp/QACXTMP{:06}{n:06}", j.ident.number.parse::<u32>().unwrap_or(0) % 1_000_000);
    let mut bytes = ccsid::to_ebcdic(&name).unwrap_or_default();
    bytes.push(0);
    if !is_null(buf) {
        return match mem(j, buf, bytes.len()) {
            Ok(b) => {
                b.copy_from_slice(&bytes);
                buf
            }
            Err(e) => {
                fail(j, e);
                raw(mi::FatPtr::null())
            }
        };
    }
    let mut st = Stable::new(j, bytes.len());
    st.bytes().copy_from_slice(&bytes);
    let p = st.ptr();
    super::with_state(j, |s| s.tmpnames.push(st));
    raw(p)
}

bound_c!(b_iconv(cd: i64, i: u128, il: u128, o: u128, ol: u128) -> i64 = iconv);
bound_c!(b_iconv_close(cd: i64) -> i64 = iconv_close);
bound_c!(b_tmpnam(buf: u128) -> u128 = tmpnam);

pub(super) fn procedures() -> Vec<(&'static str, usize)> {
    macro_rules! at {
        ($f:ident) => {
            $f as *const () as usize
        };
    }
    vec![
        ("QtqIconvOpen", at!(b_qtqiconvopen)),
        ("iconv_open", at!(b_iconv_open)),
        ("iconv", at!(b_iconv)),
        ("iconv_close", at!(b_iconv_close)),
        ("_C_IFS_tmpnam", at!(b_tmpnam)),
    ]
}
