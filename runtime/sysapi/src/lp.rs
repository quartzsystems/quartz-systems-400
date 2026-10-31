// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The product and license APIs (`docs/LICPGM.md` §4): `QSZRTVPR`,
//! `QSZCRTPD`, `QSZCRTPL`, `QSZPKGPO`, `QSZDLTPD`, `QSZDLTPL`,
//! `QSZSPTPR`, `QSZCHKTG`, `QLZAREQ`, `QLZARLS`. Each reads its
//! parameters' bytes, has the provider answer them (`licpgm::api`),
//! and writes the receiver and the error code.

use std::cell::RefCell;

use mi::Job;

use crate::param::{Args, ErrCode};
use crate::provider::{ApiFail, Provider};

/// The APIs.
pub const APIS: &[(&str, crate::ApiFn)] = &[
    ("QSZRTVPR", qszrtvpr),
    ("QSZCRTPD", qszcrtpd),
    ("QSZCRTPL", qszcrtpl),
    ("QSZPKGPO", qszpkgpo),
    ("QSZDLTPD", qszdltpd),
    ("QSZDLTPL", qszdltpl),
    ("QSZSPTPR", qszsptpr),
    ("QSZCHKTG", qszchktg),
    ("QLZAREQ", qlzareq),
    ("QLZARLS", qlzarls),
];

thread_local! {
    /// The shipped catalog as an installed system, for providers with
    /// no product records of their own.
    static CATALOG: RefCell<Option<licpgm::MemStore>> = const { RefCell::new(None) };
}

/// Answer an API from the shipped catalog (the provider default).
pub fn catalog_answer(api: &str, parms: &[Vec<u8>]) -> Result<Vec<u8>, ApiFail> {
    CATALOG.with(|c| {
        let mut c = c.borrow_mut();
        let s = c.get_or_insert_with(|| {
            let mut s = licpgm::MemStore::with_libs(&["QSYS", "QSYS2", "QUSRSYS", "QGPL"]);
            let _ = licpgm::catalog::seed(&mut s, 0);
            s
        });
        let objs = s.clone();
        let sys = licpgm::SystemInfo::current("QS40001", "P05");
        let cx = licpgm::api::dispatch::CallCtx {
            sys: &sys,
            today: licpgm::Date::new(2026, 1, 1).expect("a date"),
            job: "",
            curlib: "QGPL",
        };
        licpgm::api::dispatch::call(s, &objs, &cx, api, parms).map_err(|e| ApiFail {
            msgid: e.id.to_string(),
            data: e.data,
        })
    })
}

fn bytes(a: &Args<'_>, i: usize, n: usize) -> Vec<u8> {
    a.bytes(i, n)
}

fn count(a: &Args<'_>, i: usize) -> usize {
    usize::try_from(a.bin4(i)).unwrap_or(0).min(10_000)
}

fn bin(a: &Args<'_>, i: usize) -> Vec<u8> {
    a.bin4(i).to_be_bytes().to_vec()
}

/// Call, then the error code at `errc`; the answer when it worked.
fn run(j: &mut Job, p: &mut dyn Provider, a: &Args<'_>, errc: usize, api: &str, parms: Vec<Vec<u8>>) -> Option<Vec<u8>> {
    let e = ErrCode::at(a, errc);
    match p.licpgm(api, parms) {
        Ok(b) => {
            e.ok();
            Some(b)
        }
        Err(f) => {
            e.fail(j, &f);
            None
        }
    }
}

/// `QSZRTVPR(receiver, length, format, product information, errc
/// [, product information format])`.
fn qszrtvpr(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let fmt = if a.present(5) { a.char(5, 8) } else { String::new() };
    let info = if fmt == "PRDI0200" {
        let head = bytes(&a, 3, 36);
        let n = usize::try_from(crate::param::bin4_at(&head, 28)).unwrap_or(36).clamp(36, 4096);
        bytes(&a, 3, n)
    } else {
        bytes(&a, 3, 27)
    };
    let len = a.bin4(1);
    let parms = vec![len.to_be_bytes().to_vec(), bytes(&a, 2, 8), info, crate::param::fixed(&fmt, 8)];
    if let Some(out) = run(j, p, &a, 4, "QSZRTVPR", parms) {
        a.put_bytes(0, &out[..out.len().min(usize::try_from(len).unwrap_or(0))]);
    }
}

/// `QSZCRTPD(name, information, options, count, languages, count,
/// text, public authority, errc)`.
fn qszcrtpd(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let (no, nl) = (count(&a, 3), count(&a, 5));
    let parms = vec![
        bytes(&a, 0, 20),
        bytes(&a, 1, 106),
        bytes(&a, 2, 41 * no),
        bin(&a, 3),
        bytes(&a, 4, 20 * nl),
        bin(&a, 5),
        bytes(&a, 6, 50),
        bytes(&a, 7, 10),
    ];
    run(j, p, &a, 8, "QSZCRTPD", parms);
}

/// `QSZCRTPL(name, information, secondary language library, principal
/// library, libraries, count, exits, count, folders, count, text,
/// public authority, errc [, directories, count, format])`.
fn qszcrtpl(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let mut parms = vec![
        bytes(&a, 0, 10),
        bytes(&a, 1, 87),
        bytes(&a, 2, 10),
        bytes(&a, 3, 30),
        bytes(&a, 4, 30 * count(&a, 5)),
        bin(&a, 5),
        bytes(&a, 6, 20 * count(&a, 7)),
        bin(&a, 7),
        bytes(&a, 8, 126 * count(&a, 9)),
        bin(&a, 9),
        bytes(&a, 10, 50),
        bytes(&a, 11, 10),
    ];
    if a.present(14) {
        parms.push(bin(&a, 14));
    }
    run(j, p, &a, 12, "QSZCRTPL", parms);
}

/// `QSZPKGPO(option information, repackage, allow change, errc)`.
fn qszpkgpo(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let parms = vec![bytes(&a, 0, 35), bytes(&a, 1, 4), bytes(&a, 2, 5)];
    run(j, p, &a, 3, "QSZPKGPO", parms);
}

/// `QSZDLTPD(name, errc)`.
fn qszdltpd(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    run(j, p, &a, 1, "QSZDLTPD", vec![bytes(&a, 0, 20)]);
}

/// `QSZDLTPL(name, errc)`.
fn qszdltpl(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    run(j, p, &a, 1, "QSZDLTPL", vec![bytes(&a, 0, 20)]);
}

/// `QSZSPTPR(product information, length, format, action, errc)`.
fn qszsptpr(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let n = count(&a, 1).max(53);
    let parms = vec![bytes(&a, 0, n), bin(&a, 1), bytes(&a, 2, 8), bin(&a, 3)];
    run(j, p, &a, 4, "QSZSPTPR", parms);
}

/// `QSZCHKTG(target, supported releases, count, validated,
/// equivalent, errc)`.
fn qszchktg(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let parms = vec![bytes(&a, 0, 10), bytes(&a, 1, 6 * count(&a, 2)), bin(&a, 2)];
    match run(j, p, &a, 5, "QSZCHKTG", parms) {
        Some(out) if out.len() == 12 => {
            a.put_bytes(3, &out[..6]);
            a.put_bytes(4, &out[6..]);
        }
        _ => {
            a.put_char(3, 6, "");
            a.put_char(4, 6, "");
        }
    }
}

/// The license user parameter: `LICL0100` is 10 bytes, `LICL0200`
/// says its own length.
fn lic_user(a: &Args<'_>, fmt: &str) -> Vec<u8> {
    if fmt == "LICL0200" {
        let head = bytes(a, 2, 28);
        bytes(a, 2, licpgm::api::lic::licl0200_extent(&head).min(4096))
    } else {
        bytes(a, 2, 10)
    }
}

/// `QLZAREQ(product, format, user, format, errc)`.
fn qlzareq(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let ufmt = a.char(3, 8);
    let parms = vec![bytes(&a, 0, 17), bytes(&a, 1, 8), lic_user(&a, &ufmt), bytes(&a, 3, 8)];
    run(j, p, &a, 4, "QLZAREQ", parms);
}

/// `QLZARLS(product, format, user, format, errc)`.
fn qlzarls(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let ufmt = a.char(3, 8);
    let parms = vec![bytes(&a, 0, 17), bytes(&a, 1, 8), lic_user(&a, &ufmt), bytes(&a, 3, 8)];
    run(j, p, &a, 4, "QLZARLS", parms);
}
