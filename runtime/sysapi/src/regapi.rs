// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The registration facility APIs: the programs `QUSRGPT`, `QUSDRGPT`,
//! `QUSADDEP`, `QUSRMVEP` and `QUSRTVEI`, and the same as the bound
//! procedures of `QUSRGFA1` (`QusRegisterExitPoint`,
//! `QusDeregisterExitPoint`, `QusAddExitProgram`,
//! `QusRemoveExitProgram`) and `QUSRGFA2`
//! (`QusRetrieveExitInformation`). Every parameter is by reference.
//! Controls and attributes are variable length records: a count, then
//! each record's length, key, data length and data.

use codegen::abi::Env;
use mi::Job;
use regfac::{Answer, Desc, ExitPoint, ExitProgram, MtAction, Prep, Query, ThreadSafe};

use crate::param::{Args, ErrCode};
use crate::provider::{ApiFail, Provider};

/// The program APIs.
pub const APIS: &[(&str, crate::ApiFn)] = &[
    ("QUSRGPT", qusrgpt),
    ("QUSDRGPT", qusdrgpt),
    ("QUSADDEP", qusaddep),
    ("QUSRMVEP", qusrmvep),
    ("QUSRTVEI", qusrtvei),
];

/// The bound procedures' names, `QUSRGFA1` then `QUSRGFA2`.
pub const PROCS: &[&str] = &[
    "QusRegisterExitPoint",
    "QusDeregisterExitPoint",
    "QusAddExitProgram",
    "QusRemoveExitProgram",
    "QusRetrieveExitInformation",
];

fn fail(f: regfac::RegFail) -> ApiFail {
    ApiFail {
        msgid: f.msgid,
        data: f.data,
    }
}

/// The records of a variable length record parameter: key and data.
fn records(a: &Args<'_>, i: usize) -> Vec<(i32, Vec<u8>)> {
    let n = usize::try_from(a.bin4(i)).unwrap_or(0).min(64);
    let mut out = Vec::new();
    let mut at = 4usize;
    for _ in 0..n {
        let Some(head) = a.raw(i, at + 12) else {
            break;
        };
        let h = &head[at..at + 12];
        let rec_len = usize::try_from(crate::param::bin4_at(h, 0)).unwrap_or(0);
        let key = crate::param::bin4_at(h, 4);
        let len = usize::try_from(crate::param::bin4_at(h, 8))
            .unwrap_or(0)
            .min(4096);
        let data = a
            .raw(i, at + 12 + len)
            .map(|b| b[at + 12..at + 12 + len].to_vec())
            .unwrap_or_default();
        out.push((key, data));
        at += rec_len.max(12 + len);
    }
    out
}

fn text(b: &[u8]) -> String {
    ccsid::decode_trimmed(b)
}

fn prep(b: &[u8]) -> Option<Prep> {
    let pgm = text(b.get(..10).unwrap_or(&[]));
    (!pgm.is_empty() && pgm != "*NONE").then(|| Prep {
        pgm,
        lib: text(b.get(10..20).unwrap_or(&[])),
        format: text(b.get(20..28).unwrap_or(&[])),
    })
}

fn msg_desc(b: &[u8]) -> Desc {
    Desc::Msg {
        msgf: text(b.get(..10).unwrap_or(&[])),
        lib: text(b.get(10..20).unwrap_or(&[])),
        msgid: text(b.get(20..27).unwrap_or(&[])),
    }
}

fn first(b: &[u8]) -> char {
    b.first().map_or(' ', |c| ccsid::char_of(*c))
}

/// An exit point from `QusRegisterExitPoint`'s controls: key 2 the
/// maximum (-1 none), 3 allow deregistration, 4 allow change, 5 to 7
/// the preprocessing programs (add, remove, retrieve), 8 a message
/// description, 10 a text description.
fn point_of(name: &str, format: &str, recs: &[(i32, Vec<u8>)]) -> ExitPoint {
    let mut p = ExitPoint::new(name, format);
    for (key, d) in recs {
        match key {
            2 => p.max = u32::try_from(crate::param::bin4_at(d, 0)).ok(),
            3 => p.allow_dereg = first(d) == '1',
            4 => p.allow_change = first(d) == '1',
            5 => p.prep_add = prep(d),
            6 => p.prep_rmv = prep(d),
            7 => p.prep_rtv = prep(d),
            8 => p.desc = msg_desc(d),
            10 => p.desc = Desc::text(&text(d)),
            _ => {}
        }
    }
    p
}

fn run(j: &mut Job, p: &mut dyn Provider, errc: ErrCode, q: Query) -> Option<Answer> {
    match p.regfac(q) {
        Ok(a) => {
            errc.ok();
            Some(a)
        }
        Err(f) => {
            errc.fail(j, &f);
            None
        }
    }
}

/// `QUSRGPT(name, format, controls, errc)`.
fn qusrgpt(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 3);
    let (name, format) = (a.char(0, 20), a.char(1, 8));
    let point = point_of(&name, &format, &records(&a, 2));
    run(j, p, errc, Query::Register { point });
}

/// `QUSDRGPT(name, format, errc)`.
fn qusdrgpt(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 2);
    let q = Query::Deregister {
        name: a.char(0, 20),
        format: a.char(1, 8),
    };
    run(j, p, errc, q);
}

/// `QUSADDEP(name, format, number, program, data, data length,
/// attributes, errc)`; attribute keys: 1 replace, 2 a message
/// description, 3 the data's CCSID, 4 a text description, 5
/// threadsafe, 6 the multithreaded job action.
fn qusaddep(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 7);
    let (pgm, lib) = a.qual(3);
    let len = a.bin4(5);
    let Ok(n) = usize::try_from(len) else {
        errc.fail(j, &ApiFail::id("CPF3CD6").bin4(len));
        return;
    };
    let mut program = ExitProgram {
        number: a.bin4(2),
        pgm,
        lib: if lib == "*CURLIB" { "QGPL".into() } else { lib },
        data: a.bytes(4, n),
        ..ExitProgram::default()
    };
    let mut opts = regfac::AddOpts::default();
    for (key, d) in records(&a, 6) {
        match key {
            1 => opts.replace = first(&d) == '1',
            2 => program.desc = msg_desc(&d),
            3 => program.ccsid = crate::param::bin4_at(&d, 0),
            4 => program.desc = Desc::text(&text(&d)),
            5 => program.threadsafe = ThreadSafe::of_code(first(&d)),
            6 => program.mt_action = MtAction::of_code(first(&d)),
            _ => {}
        }
    }
    let q = Query::Add {
        name: a.char(0, 20),
        format: a.char(1, 8),
        program,
        opts,
    };
    run(j, p, errc, q);
}

/// `QUSRMVEP(name, format, number, errc)`.
fn qusrmvep(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 3);
    let q = Query::Remove {
        name: a.char(0, 20),
        format: a.char(1, 8),
        number: a.bin4(2),
    };
    run(j, p, errc, q);
}

/// The selection criteria parameter's bytes, read entry by entry so
/// nothing past the caller's storage is touched.
fn selection(a: &Args<'_>, i: usize) -> Vec<u8> {
    let n = a.bin4(i);
    let mut buf = a.bytes(i, 4);
    let mut at = 4usize;
    for _ in 0..n.clamp(0, 64) {
        let head = a.bytes(i, at + 16);
        let size = usize::try_from(crate::param::bin4_at(&head, at)).unwrap_or(0);
        let len = usize::try_from(crate::param::bin4_at(&head, at + 12))
            .unwrap_or(0)
            .min(regfac::registry::MAX_DATA);
        let whole = a.bytes(i, at + 16 + len);
        buf.extend_from_slice(&whole[at..]);
        at += size.max(16 + len);
        buf.resize(at, 0);
    }
    buf
}

/// `QUSRTVEI(handle, receiver, length, format, name, point format,
/// program number, selection, errc)`.
fn qusrtvei(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 8);
    let room = usize::try_from(a.bin4(2)).unwrap_or(0);
    let format = a.char(3, 8);
    if !matches!(format.as_str(), "EXTI0100" | "EXTI0200" | "EXTI0300") {
        errc.fail(j, &ApiFail::id("CPF3C21").text(&format, 8));
        return;
    }
    if room < 8 {
        errc.fail(
            j,
            &ApiFail::id("CPF3C24").bin4(i32::try_from(room).unwrap_or(0)),
        );
        return;
    }
    let start = match regfac::exti::resume_at(&a.bytes(0, 16)) {
        Ok(s) => s,
        Err(f) => {
            errc.fail(j, &fail(f));
            return;
        }
    };
    let sel = if format == "EXTI0100" || !a.present(7) || a.bin4(7) == 0 {
        Vec::new()
    } else {
        match regfac::exti::parse_selection(&selection(&a, 7)) {
            Ok(s) => s,
            Err(f) => {
                errc.fail(j, &fail(f));
                return;
            }
        }
    };
    let q = Query::List {
        name: a.char(4, 20),
        format: a.char(5, 8),
        which: 2,
    };
    let Some(Answer::Points { points }) = run(j, p, errc, q) else {
        return;
    };
    let refs: Vec<&ExitPoint> = points.iter().collect();
    let b = match format.as_str() {
        "EXTI0100" => regfac::exti::exti0100(&refs, start, room),
        f => regfac::exti::exti0200(&refs, a.bin4(6), &sel, f == "EXTI0300", start, room),
    };
    let n = room.min(b.len());
    a.put_bytes(1, &b[..n]);
}

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
            match crate::provider_of(j) {
                Some(p) => {
                    let mut p = p.borrow_mut();
                    $f(j, &args, &mut *p);
                }
                None => {
                    let e = ErrCode::at(&Args(&args), args.len().saturating_sub(1));
                    e.fail(j, &ApiFail::id("CPF3CDA"));
                }
            }
        }
    };
}

bound!(b_register, qusrgpt);
bound!(b_deregister, qusdrgpt);
bound!(b_add, qusaddep);
bound!(b_remove, qusrmvep);
bound!(b_retrieve, qusrtvei);

/// The bound procedures with their code addresses.
#[must_use]
pub fn procedures() -> Vec<(&'static str, usize)> {
    macro_rules! addr {
        ($f:ident) => {
            $f as *const () as usize
        };
    }
    vec![
        (PROCS[0], addr!(b_register)),
        (PROCS[1], addr!(b_deregister)),
        (PROCS[2], addr!(b_add)),
        (PROCS[3], addr!(b_remove)),
        (PROCS[4], addr!(b_retrieve)),
    ]
}
