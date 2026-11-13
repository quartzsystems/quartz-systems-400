// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The NFS APIs (docs/NFS.md section 8, docs/SYSAPI.md): `QZNFRTVE`,
//! Retrieve NFS Export Information, and `QZNFNFSO`, Perform NFS Options.
//! Each gathers the parameters the answer depends on, the provider
//! answers (the storage service over the link, `stgsvc/nfs/api.rs`),
//! and the receiver, the feedback and the error code are filled here.

use mi::Job;

use crate::param::{Args, ErrCode};
use crate::provider::Provider;

/// The APIs.
pub const APIS: &[(&str, crate::ApiFn)] = &[("QZNFNFSO", qznfnfso), ("QZNFRTVE", qznfrtve)];

fn bin(a: &Args<'_>, i: usize) -> Vec<u8> {
    a.bin4(i).to_be_bytes().to_vec()
}

fn len_of(a: &Args<'_>, i: usize, max: usize) -> usize {
    usize::try_from(a.bin4(i)).unwrap_or(0).min(max)
}

/// `QZNFRTVE`: receiver, its length, feedback, format, path name, its
/// length, its CCSID, the CCSID wanted, handle, error code.
fn qznfrtve(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let e = ErrCode::at(&a, 9);
    let len = len_of(&a, 1, 16 << 20);
    let path_len = len_of(&a, 5, 16 << 20);
    let parms = vec![
        bin(&a, 1),
        a.bytes(3, 8),
        if path_len > 0 {
            a.bytes(4, path_len)
        } else {
            Vec::new()
        },
        bin(&a, 6),
        bin(&a, 7),
        bin(&a, 8),
    ];
    match p.nfs("QZNFRTVE", parms) {
        Ok(b) => {
            e.ok();
            let (fb, data) = b.split_at(b.len().min(16));
            a.put_bytes(2, fb);
            a.put_bytes(0, &data[..data.len().min(len)]);
        }
        Err(f) => e.fail(j, &f),
    }
}

/// `QZNFNFSO`: option, input buffer, its length, output buffer, its
/// length, format, error code.
fn qznfnfso(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let e = ErrCode::at(&a, 6);
    let in_len = len_of(&a, 2, 1 << 20);
    let out_len = len_of(&a, 4, 16 << 20);
    let parms = vec![
        bin(&a, 0),
        if in_len > 0 {
            a.bytes(1, in_len)
        } else {
            Vec::new()
        },
        bin(&a, 4),
        a.bytes(5, 8),
    ];
    match p.nfs("QZNFNFSO", parms) {
        Ok(b) => {
            e.ok();
            if !b.is_empty() {
                a.put_bytes(3, &b[..b.len().min(out_len)]);
            }
        }
        Err(f) => e.fail(j, &f),
    }
}
