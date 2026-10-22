// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The rest of `QLEAWI`, as bound procedures: `CEE4ABN`, `CEETREC`,
//! `CEEDOD`, `CEEGPID`, `CEETSTA`, the heap APIs `CEECRHP`, `CEEDSHP`,
//! `CEEGTST`, `CEEFRST`, `CEECZST`, the date and time APIs `CEEDAYS`,
//! `CEEDATE`, `CEEDATM`, `CEESECS`, `CEEUTC`, `CEELOCT`, `CEEQCEN`,
//! `CEESCEN`, `CEEGMT`, `CEEGMTO`, `CEERAN0`, and `CEE4RAGE`. Every
//! parameter is by reference; a `VSTRING` is a two-byte length and the
//! characters; `CHAR80` outputs are blank padded.

use std::cell::Cell;

use codegen::abi::Env;
use mi::{Exception, FatPtr, Job};

use crate::job::Civil;
use crate::param::{Args, fixed};
use crate::picture;
use crate::provider::ApiFail;

/// The bound signature with room for nine by-reference parameters.
macro_rules! bound {
    ($name:ident, $f:ident) => {
        #[allow(clippy::too_many_arguments)]
        extern "sysv64" fn $name(
            env: *mut Env,
            argc: i64,
            _od: i64,
            p1: i64,
            p2: i64,
            p3: i64,
            p4: i64,
            p5: i64,
            p6: i64,
            p7: i64,
            p8: i64,
            p9: i64,
        ) {
            let all = [p1, p2, p3, p4, p5, p6, p7, p8, p9];
            let n = usize::try_from(argc).unwrap_or(0).min(9);
            let args: Vec<u64> = all[..n].iter().map(|v| v.cast_unsigned()).collect();
            let j = mi::storage::job_of(env);
            $f(j, &Args(&args));
        }
    };
}

bound!(b_cee4abn, cee4abn);
bound!(b_ceetrec, ceetrec);
bound!(b_ceedod, ceedod);
bound!(b_ceegpid, ceegpid);
bound!(b_ceetsta, ceetsta);
bound!(b_ceecrhp, ceecrhp);
bound!(b_ceedshp, ceedshp);
bound!(b_ceegtst, ceegtst);
bound!(b_ceefrst, ceefrst);
bound!(b_ceeczst, ceeczst);
bound!(b_ceedays, ceedays);
bound!(b_ceedate, ceedate);
bound!(b_ceedatm, ceedatm);
bound!(b_ceesecs, ceesecs);
bound!(b_ceeutc, ceeutc);
bound!(b_ceeloct, ceeloct);
bound!(b_ceeqcen, ceeqcen);
bound!(b_ceescen, ceescen);
bound!(b_ceegmt, ceegmt);
bound!(b_ceegmto, ceegmto);
bound!(b_ceeran0, ceeran0);
bound!(b_cee4rage, cee4rage);

/// Every procedure with its code address.
#[must_use]
pub fn procedures() -> Vec<(&'static str, usize)> {
    macro_rules! addr {
        ($f:ident) => {
            $f as *const () as usize
        };
    }
    vec![
        ("CEE4ABN", addr!(b_cee4abn)),
        ("CEETREC", addr!(b_ceetrec)),
        ("CEEDOD", addr!(b_ceedod)),
        ("CEEGPID", addr!(b_ceegpid)),
        ("CEETSTA", addr!(b_ceetsta)),
        ("CEECRHP", addr!(b_ceecrhp)),
        ("CEEDSHP", addr!(b_ceedshp)),
        ("CEEGTST", addr!(b_ceegtst)),
        ("CEEFRST", addr!(b_ceefrst)),
        ("CEECZST", addr!(b_ceeczst)),
        ("CEEDAYS", addr!(b_ceedays)),
        ("CEEDATE", addr!(b_ceedate)),
        ("CEEDATM", addr!(b_ceedatm)),
        ("CEESECS", addr!(b_ceesecs)),
        ("CEEUTC", addr!(b_ceeutc)),
        ("CEELOCT", addr!(b_ceeloct)),
        ("CEEQCEN", addr!(b_ceeqcen)),
        ("CEESCEN", addr!(b_ceescen)),
        ("CEEGMT", addr!(b_ceegmt)),
        ("CEEGMTO", addr!(b_ceegmto)),
        ("CEERAN0", addr!(b_ceeran0)),
        ("CEE4RAGE", addr!(b_cee4rage)),
    ]
}

thread_local! {
    /// The century window (`CEEQCEN`/`CEESCEN`), 80 by default.
    static CENTURY: Cell<i64> = const { Cell::new(80) };
}

/// Report through the feedback code at parameter `i`: `CEE0000` on
/// success, the condition token otherwise; an error with no feedback
/// code is signaled to the caller.
fn feedback(j: &mut Job, a: &Args<'_>, i: usize, r: Result<(), ApiFail>) {
    match r {
        Ok(()) => {
            if let Some(b) = a.raw(i, 12) {
                b.fill(0);
            }
        }
        Err(f) => {
            if let Some(b) = a.raw(i, 12) {
                let e = Exception::message(&f.msgid, "QCEEMSG", f.data, j.depth);
                let t = mi::except::encode_token(j, &e);
                b.copy_from_slice(&t);
            } else {
                let _ = j.send_message(
                    &f.msgid,
                    "QCEEMSG",
                    "QSYS",
                    f.data,
                    qir::MsgType::Escape,
                    mi::QueueRef::Same,
                );
            }
        }
    }
}

/// A `VSTRING` parameter.
fn vstring(a: &Args<'_>, i: usize) -> String {
    let n = usize::from(u16::from_be_bytes(
        a.raw(i, 2).map_or([0, 0], |b| [b[0], b[1]]),
    ));
    ccsid::decode_trimmed(a.raw(i, 2 + n).map_or(&[], |b| &b[2..]))
}

fn put_f8(a: &Args<'_>, i: usize, v: f64) {
    a.put_bytes(i, &v.to_be_bytes());
}

fn get_f8(a: &Args<'_>, i: usize) -> f64 {
    f64::from_be_bytes(
        a.raw(i, 8)
            .map_or([0; 8], |b| b.try_into().expect("8 bytes")),
    )
}

/// The clock, from the provider when installed.
fn now(j: &Job) -> crate::provider::LocalNow {
    match crate::provider_of(j) {
        Some(p) => p.borrow_mut().now(),
        None => crate::provider::MemProvider::default_now(),
    }
}

fn local_civil(j: &Job) -> Civil {
    let n = now(j);
    let local = i64::try_from(n.unix).unwrap_or(0) + i64::from(n.offset_min) * 60;
    Civil::of_unix(u64::try_from(local).unwrap_or(0))
}

/// `CEE4ABN()`: the request ends abnormally with `CEE9902` to the
/// control boundary's caller.
fn cee4abn(j: &mut Job, _a: &Args<'_>) {
    let pgm = j
        .act
        .frames
        .last()
        .and_then(|f| f.program.as_ref().map(|p| p.object.name.clone()))
        .unwrap_or_default();
    j.cancel_request("CEE9902", "QCEEMSG", fixed(&pgm, 10));
}

/// `CEETREC([return code], [user return code])`.
fn ceetrec(j: &mut Job, a: &Args<'_>) {
    j.end_request(a.bin4(1));
}

/// `CEEDOD(position, type, datatype, info1, info2, length, fc)`: no
/// operational descriptors exist, so `CEE0820`.
/// `CEEDOD(position, desc_type, datatype, info1, info2, length, fc)`:
/// the caller's operational descriptor of argument `position`;
/// `CEE0820` when it received none or the position is past them.
fn ceedod(j: &mut Job, a: &Args<'_>) {
    let pos = a.bin4(0);
    let list = j.act.frames.last().map_or(0, |f| f.opdesc);
    match mi::opdesc::entry(list, pos) {
        Some(d) => {
            a.put_bin4(1, d.desc_type);
            a.put_bin4(2, d.datatype);
            a.put_bin4(3, d.info1);
            a.put_bin4(4, d.info2);
            a.put_bin4(5, d.length);
            feedback(j, a, 6, Ok(()));
        }
        None => feedback(j, a, 6, Err(ApiFail::id("CEE0820").bin4(pos))),
    }
}

/// `CEEGPID(version, platform, fc)`: ILE version 1 on platform 3.
fn ceegpid(j: &mut Job, a: &Args<'_>) {
    a.put_bin4(0, 1);
    a.put_bin4(1, 3);
    feedback(j, a, 2, Ok(()));
}

/// `CEETSTA(presence, argument number, fc)`: whether the caller
/// received that many arguments.
fn ceetsta(j: &mut Job, a: &Args<'_>) {
    let n = a.bin4(1);
    let argc = j
        .act
        .frames
        .last()
        .map_or(0, |f| i32::try_from(f.argc).unwrap_or(0));
    a.put_bin4(0, i32::from(n >= 1 && n <= argc));
    feedback(j, a, 2, Ok(()));
}

fn heap_fail(c: mi::Cond) -> ApiFail {
    let _ = c;
    ApiFail::id("CEE0802")
}

/// `CEECRHP(heap id, initial size, increment, strategy, fc)`.
fn ceecrhp(j: &mut Job, a: &Args<'_>) {
    let id = j.create_heap();
    a.put_bin4(0, i32::try_from(id).unwrap_or(0));
    feedback(j, a, 4, Ok(()));
}

/// `CEEDSHP(heap id, fc)`.
fn ceedshp(j: &mut Job, a: &Args<'_>) {
    let id = u32::try_from(a.bin4(0)).unwrap_or(u32::MAX);
    let r = j
        .discard_heap(id)
        .map_err(|_| ApiFail::id("CEE0803").bin4(a.bin4(0)));
    feedback(j, a, 1, r);
}

/// `CEEGTST(heap id, size, address, fc)`.
fn ceegtst(j: &mut Job, a: &Args<'_>) {
    let id = u32::try_from(a.bin4(0)).unwrap_or(u32::MAX);
    let n = u64::try_from(a.bin4(1)).unwrap_or(0);
    let r = match j.alloc_from(id, n) {
        Ok(p) => {
            a.put_bytes(2, &p.to_bytes());
            Ok(())
        }
        Err(_) => Err(ApiFail::id("CEE0803").bin4(a.bin4(0))),
    };
    feedback(j, a, 3, r);
}

fn ptr_at(a: &Args<'_>, i: usize) -> FatPtr {
    FatPtr::from_bytes(
        &a.raw(i, 16)
            .map_or([0; 16], |b| b.try_into().expect("16 bytes")),
    )
}

/// `CEEFRST(address, fc)`.
fn ceefrst(j: &mut Job, a: &Args<'_>) {
    let r = j.dealloc(ptr_at(a, 0)).map_err(heap_fail);
    feedback(j, a, 1, r);
}

/// `CEECZST(address, new size, fc)`.
fn ceeczst(j: &mut Job, a: &Args<'_>) {
    let n = u64::try_from(a.bin4(1)).unwrap_or(0);
    let r = match j.realloc(ptr_at(a, 0), n) {
        Ok(p) => {
            a.put_bytes(0, &p.to_bytes());
            Ok(())
        }
        Err(c) => Err(heap_fail(c)),
    };
    feedback(j, a, 2, r);
}

fn parse_with(j: &Job, text: &str, pic: &str) -> Result<Civil, ApiFail> {
    if !picture::valid(pic) {
        return Err(ApiFail::id("CEE2518"));
    }
    let century = CENTURY.with(Cell::get);
    picture::parse(text, pic, century, local_civil(j).year).ok_or_else(|| ApiFail::id("CEE2501"))
}

/// `CEEDAYS(input date, picture, lilian, fc)`.
fn ceedays(j: &mut Job, a: &Args<'_>) {
    let r =
        parse_with(j, &vstring(a, 0), &vstring(a, 1)).map(|c| a.put_bin4(2, picture::lilian_of(c)));
    feedback(j, a, 3, r);
}

/// `CEEDATE(lilian, picture, output, fc)`.
fn ceedate(j: &mut Job, a: &Args<'_>) {
    let c = picture::civil_of_lilian(a.bin4(0));
    let r = picture::render(c, &vstring(a, 1))
        .map(|s| a.put_char(2, 80, &s))
        .ok_or_else(|| ApiFail::id("CEE2518"));
    feedback(j, a, 3, r);
}

/// `CEEDATM(seconds, picture, output, fc)`.
fn ceedatm(j: &mut Job, a: &Args<'_>) {
    let c = picture::civil_of_seconds(get_f8(a, 0));
    let r = picture::render(c, &vstring(a, 1))
        .map(|s| a.put_char(2, 80, &s))
        .ok_or_else(|| ApiFail::id("CEE2518"));
    feedback(j, a, 3, r);
}

/// `CEESECS(timestamp, picture, seconds, fc)`.
fn ceesecs(j: &mut Job, a: &Args<'_>) {
    let r =
        parse_with(j, &vstring(a, 0), &vstring(a, 1)).map(|c| put_f8(a, 2, picture::seconds_of(c)));
    feedback(j, a, 3, r);
}

/// `CEEUTC(lilian, seconds)`: the current time, UTC.
fn ceeutc(j: &mut Job, a: &Args<'_>) {
    let c = Civil::of_unix(now(j).unix);
    a.put_bin4(0, picture::lilian_of(c));
    put_f8(a, 1, picture::seconds_of(c));
}

/// `CEEGMT(lilian, seconds, fc)`.
fn ceegmt(j: &mut Job, a: &Args<'_>) {
    ceeutc(j, a);
    feedback(j, a, 2, Ok(()));
}

/// `CEELOCT(lilian, seconds, gregorian, fc)`: local time, the
/// Gregorian form `YYYYMMDDHHMISS999` in 23 characters.
fn ceeloct(j: &mut Job, a: &Args<'_>) {
    let c = local_civil(j);
    a.put_bin4(0, picture::lilian_of(c));
    put_f8(a, 1, picture::seconds_of(c));
    let g = picture::render(c, "YYYYMMDDHHMISS999").unwrap_or_default();
    a.put_char(2, 23, &g);
    feedback(j, a, 3, Ok(()));
}

/// `CEEGMTO(hours, minutes, seconds, fc)`: the local offset from UTC.
fn ceegmto(j: &mut Job, a: &Args<'_>) {
    let off = now(j).offset_min;
    a.put_bin4(0, off / 60);
    a.put_bin4(1, off % 60);
    put_f8(a, 2, f64::from(off) * 60.0);
    feedback(j, a, 3, Ok(()));
}

/// `CEEQCEN(century, fc)`.
fn ceeqcen(j: &mut Job, a: &Args<'_>) {
    a.put_bin4(0, i32::try_from(CENTURY.with(Cell::get)).unwrap_or(80));
    feedback(j, a, 1, Ok(()));
}

/// `CEESCEN(century, fc)`: 0 to 100.
fn ceescen(j: &mut Job, a: &Args<'_>) {
    let c = a.bin4(0);
    let r = if (0..=100).contains(&c) {
        CENTURY.with(|w| w.set(i64::from(c)));
        Ok(())
    } else {
        Err(ApiFail::id("CEE2501"))
    };
    feedback(j, a, 1, r);
}

/// `CEERAN0(seed, random, fc)`: a Lehmer generator, the seed updated.
fn ceeran0(j: &mut Job, a: &Args<'_>) {
    let mut seed = i64::from(a.bin4(0));
    if seed <= 0 {
        seed = i64::try_from(now(j).unix % 2_147_483_646)
            .unwrap_or(1)
            .max(1);
    }
    seed = seed * 16_807 % 2_147_483_647;
    a.put_bin4(0, i32::try_from(seed).unwrap_or(1));
    #[allow(clippy::cast_precision_loss)]
    put_f8(a, 1, seed as f64 / 2_147_483_647.0);
    feedback(j, a, 2, Ok(()));
}

/// `CEE4RAGE(procedure, fc)`: an exit procedure for the current
/// activation group.
fn cee4rage(j: &mut Job, a: &Args<'_>) {
    let p = ptr_at(a, 0);
    let r = if p.kind() == Some(mi::PtrKind::Procedure) {
        let ag = j.act.current as usize;
        j.act.groups[ag].exits.push(p.offset);
        Ok(())
    } else {
        Err(ApiFail::id("CEE0801").text("CEE4RAG", 7))
    };
    feedback(j, a, 1, r);
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_procedure_has_an_address() {
        let p = super::procedures();
        assert_eq!(p.len(), 22);
        assert!(p.iter().all(|(_, a)| *a != 0));
    }
}
