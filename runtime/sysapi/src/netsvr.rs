// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The NetServer APIs (`docs/NETSERVER.md`, "APIs"): `QZLSADFS`,
//! `QZLSADPS`, `QZLSCHFS`, `QZLSCHPS`, `QZLSCHSG`, `QZLSCHSI`,
//! `QZLSCHSN`, `QZLSENDS`, `QZLSENSS`, `QZLSLSTI`, `QZLSOLST`,
//! `QZLSRMS`, `QZLSSTRS`. Each gathers its parameters' bytes in the
//! order `netsvrcfg::api` reads them, the provider answers (the storage
//! service over the link), and the receiver, user space and error code
//! are filled here.

#![allow(clippy::doc_markdown)] // NetServer, NetBIOS

use mi::Job;

use crate::param::{Args, ErrCode};
use crate::provider::{ApiFail, Provider};

/// The APIs.
pub const APIS: &[(&str, crate::ApiFn)] = &[
    ("QZLSADFS", qzlsadfs),
    ("QZLSADPS", qzlsadps),
    ("QZLSCHFS", qzlschfs),
    ("QZLSCHPS", qzlschps),
    ("QZLSCHSG", qzlschsg),
    ("QZLSCHSI", qzlschsi),
    ("QZLSCHSN", qzlschsn),
    ("QZLSENDS", qzlsends),
    ("QZLSENSS", qzlsenss),
    ("QZLSLSTI", qzlslsti),
    ("QZLSOLST", qzlsolst),
    ("QZLSRMS", qzlsrms),
    ("QZLSSTRS", qzlsstrs),
];

/// Parameter `i`'s `n` bytes, or nothing when it was not passed.
fn opt(a: &Args<'_>, i: usize, n: usize) -> Vec<u8> {
    if a.present(i) {
        a.bytes(i, n)
    } else {
        Vec::new()
    }
}

fn bin(a: &Args<'_>, i: usize) -> Vec<u8> {
    a.bin4(i).to_be_bytes().to_vec()
}

fn len_of(a: &Args<'_>, i: usize, max: usize) -> usize {
    usize::try_from(a.bin4(i)).unwrap_or(0).min(max)
}

/// Call, then the error code at `errc`; the answer when it worked.
fn run(
    j: &mut Job,
    p: &mut dyn Provider,
    a: &Args<'_>,
    errc: usize,
    api: &str,
    parms: Vec<Vec<u8>>,
) -> Option<Vec<u8>> {
    let e = ErrCode::at(a, errc);
    match p.netsvr(api, parms) {
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

/// A file share's parameters: share, path, path length, CCSID, text,
/// permission, maximum users, error code, then the optional text CCSID,
/// text conversion, extensions, extension count, encryption,
/// authorization list.
fn file_parms(a: &Args<'_>) -> Vec<Vec<u8>> {
    let ext_count = if a.present(11) {
        len_of(a, 11, 1000)
    } else {
        0
    };
    vec![
        a.bytes(0, 12),
        a.bytes(1, len_of(a, 2, 4096)),
        bin(a, 2),
        bin(a, 3),
        a.bytes(4, 50),
        bin(a, 5),
        bin(a, 6),
        if a.present(8) { bin(a, 8) } else { Vec::new() },
        opt(a, 9, 1),
        opt(a, 10, 50 * ext_count),
        if a.present(11) {
            bin(a, 11)
        } else {
            Vec::new()
        },
        opt(a, 12, 1),
        opt(a, 13, 10),
    ]
}

/// `QZLSADFS`.
fn qzlsadfs(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    run(j, p, &a, 7, "QZLSADFS", file_parms(&a));
}

/// `QZLSCHFS`.
fn qzlschfs(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    run(j, p, &a, 7, "QZLSCHFS", file_parms(&a));
}

/// A print share's parameters: share, output queue, text, spooled file
/// type, driver, error code, then the optional printer file, publish,
/// encryption, authorization list.
fn print_parms(a: &Args<'_>) -> Vec<Vec<u8>> {
    vec![
        a.bytes(0, 12),
        a.bytes(1, 20),
        a.bytes(2, 50),
        bin(a, 3),
        a.bytes(4, 50),
        opt(a, 6, 20),
        opt(a, 7, 1),
        opt(a, 8, 1),
        opt(a, 9, 10),
    ]
}

/// `QZLSADPS`.
fn qzlsadps(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    run(j, p, &a, 5, "QZLSADPS", print_parms(&a));
}

/// `QZLSCHPS`.
fn qzlschps(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    run(j, p, &a, 5, "QZLSCHPS", print_parms(&a));
}

/// `QZLSRMS(share, errc)`.
fn qzlsrms(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    run(j, p, &a, 1, "QZLSRMS", vec![a.bytes(0, 12)]);
}

/// `QZLSCHSG(guest, errc)`.
fn qzlschsg(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    run(j, p, &a, 1, "QZLSCHSG", vec![a.bytes(0, 10)]);
}

/// `QZLSCHSI(request, length, format, errc)`.
fn qzlschsi(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let n = len_of(&a, 1, 1 << 20);
    run(
        j,
        p,
        &a,
        3,
        "QZLSCHSI",
        vec![a.bytes(0, n), bin(&a, 1), a.bytes(2, 8)],
    );
}

/// `QZLSCHSN(server, domain, text, errc [, allow system name])`.
fn qzlschsn(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let parms = vec![
        a.bytes(0, 15),
        a.bytes(1, 15),
        a.bytes(2, 50),
        opt(&a, 4, 1),
    ];
    run(j, p, &a, 3, "QZLSCHSN", parms);
}

/// `QZLSENDS(errc)`.
fn qzlsends(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    run(j, p, &a, 0, "QZLSENDS", Vec::new());
}

/// `QZLSENSS(workstation, errc [, session identifier])`.
fn qzlsenss(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    run(j, p, &a, 1, "QZLSENSS", vec![a.bytes(0, 15), opt(&a, 2, 8)]);
}

/// `QZLSSTRS(reset, errc)`.
fn qzlsstrs(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    run(j, p, &a, 1, "QZLSSTRS", vec![a.bytes(0, 1)]);
}

/// `QZLSOLST(receiver, length, list information, format, qualifier,
/// errc [, session user, session identifier])`.
fn qzlsolst(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let parms = vec![
        bin(&a, 1),
        a.bytes(3, 8),
        a.bytes(4, 15),
        opt(&a, 6, 10),
        opt(&a, 7, 8),
    ];
    if let Some(out) = run(j, p, &a, 5, "QZLSOLST", parms) {
        let (info, data) = out.split_at(out.len().min(64));
        a.put_bytes(2, info);
        let room = len_of(&a, 1, usize::MAX);
        a.put_bytes(0, &data[..data.len().min(room)]);
    }
}

/// `QZLSLSTI(user space, format, qualifier, errc [, session user,
/// session identifier])`: the service builds the space's contents from
/// offset 64 (the user area is left alone).
fn qzlslsti(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let space = a.bytes(0, 20);
    let format = a.bytes(1, 8);
    let (name, lib) = a.qual(0);
    let missing = || {
        let mut f = ApiFail::id("CPFB68F");
        f.data.extend(crate::param::fixed("QZLSLSTI", 10));
        f.data.extend_from_slice(&space);
        f.data.extend(3i32.to_be_bytes());
        f.data.extend_from_slice(&format);
        f.data.extend(0i32.to_be_bytes());
        f
    };
    let Some(lib) = j.usrspcs.library_of(&name, &lib) else {
        ErrCode::at(&a, 3).fail(j, &missing());
        return;
    };
    if j.map_user_space(&name, &lib).is_err() {
        ErrCode::at(&a, 3).fail(j, &missing());
        return;
    }
    let parms = vec![
        space.clone(),
        format.clone(),
        a.bytes(2, 15),
        opt(&a, 4, 10),
        opt(&a, 5, 8),
    ];
    let Some(image) = run(j, p, &a, 3, "QZLSLSTI", parms) else {
        return;
    };
    let end = 64 + image.len();
    j.usrspcs.grow(&name, &lib, end, &mut j.spaces);
    match j.usrspcs.mapped_mut(&name, &lib) {
        Some(s) if s.len() >= end => {
            s[64..end].copy_from_slice(&image);
            j.sync_user_spaces();
        }
        _ => ErrCode::at(&a, 3).fail(j, &missing()),
    }
}
