// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QZNFRTVE` and `QZNFNFSO` in the service (`stg::Client::nfs_api`,
//! docs/SYSAPI.md): the job passes the parameters the answer depends
//! on, the service answers the bytes.
//!
//! - `QZNFRTVE`: receiver length (4), format (8, CP037), path name
//!   (bytes), its CCSID (4), the CCSID wanted back (4), handle (4). The
//!   answer is the feedback (16: bytes returned, available, entries,
//!   handle) then the receiver's bytes.
//! - `QZNFNFSO`: option (4), the input buffer (bytes), the output
//!   buffer's length (4), format (8). The answer is the output buffer.

use ifscmd::Msg;
use nfs::exports::api::{Resolved, expe0100, expe0200};
use nfs::exports::{DataCcsid, HostOpt, PathCcsid, file};
use sec::auth::principal::Principal;
use sec::profile::SpecialAuth;
use stg::nfs::NfsOptions;

use crate::ipl::system::System;

type ApiResult = Result<Vec<u8>, (String, Vec<u8>)>;

fn err(m: Msg) -> (String, Vec<u8>) {
    (m.id, m.data)
}

fn be(b: &[u8]) -> u32 {
    b.get(..4)
        .and_then(|x| x.try_into().ok())
        .map_or(0, u32::from_be_bytes)
}

fn text(ccsid: u16, b: &[u8]) -> String {
    ccsid::conv::to_string(ccsid, b).unwrap_or_else(|_| ccsid::from_ebcdic(b))
}

fn bytes(ccsid: u16, s: &str) -> Vec<u8> {
    ccsid::conv::from_str(ccsid, s).unwrap_or_else(|_| s.as_bytes().to_vec())
}

/// Run one API.
pub fn call(system: &mut System, me: &Principal, api: &str, parms: &[Vec<u8>]) -> ApiResult {
    let p = |i: usize| parms.get(i).map_or(&[][..], Vec::as_slice);
    match api {
        "QZNFRTVE" => rtve(
            be(p(0)) as usize,
            &ccsid::from_ebcdic(p(1)),
            p(2),
            be(p(3)),
            be(p(4)),
            be(p(5)) as usize,
        ),
        "QZNFNFSO" => nfso(
            system,
            me,
            be(p(0)),
            p(1),
            be(p(2)) as usize,
            &ccsid::from_ebcdic(p(3)),
        ),
        _ => Err(err(Msg::new("CPF3CF1"))),
    }
}

/// `QZNFRTVE`.
fn rtve(
    len: usize,
    format: &str,
    path: &[u8],
    path_ccsid: u32,
    want: u32,
    handle: usize,
) -> ApiResult {
    let format = format.trim();
    if format != "EXPE0100" && format != "EXPE0200" {
        return Err(err(Msg::new("CPF3C21").chars(format, 8)));
    }
    let want = u16::try_from(if want == 0 { 37 } else { want }).unwrap_or(37);
    let path_ccsid = u16::try_from(if path_ccsid == 0 { 37 } else { path_ccsid }).unwrap_or(37);
    let wanted = text(path_ccsid, path)
        .trim_end_matches(['\0', ' '])
        .to_string();
    let entries: Vec<super::table::Entry> = super::state()
        .table
        .entries()
        .iter()
        .filter(|e| wanted.is_empty() || file::same_path(&e.export.path, &wanted))
        .cloned()
        .collect();
    let enc = |s: &str| bytes(want, s);
    let host_ccsids = |h: &HostOpt| {
        let data = match h.data {
            DataCcsid::Ccsid(n) => u32::from(n),
            _ => 65535,
        };
        let path = match h.path {
            PathCcsid::Ccsid(n) => u32::from(n),
            _ => 819,
        };
        (data, path)
    };
    let mut out = Vec::new();
    let mut available = 0usize;
    let mut returned = 0u32;
    let mut next = 0usize;
    for (i, e) in entries.iter().enumerate().skip(handle) {
        let r = Resolved {
            export: &e.export,
            anon_uid: e.anon_uid.unwrap_or(u32::MAX),
            anon_profile: &e.anon_profile,
            ccsid: u32::from(want),
            encode: &enc,
            host_ccsids: &host_ccsids,
        };
        let b = if format == "EXPE0100" {
            expe0100(&r)
        } else {
            expe0200(&r)
        };
        available += b.len();
        if next == 0 && out.len() + b.len() <= len {
            out.extend_from_slice(&b);
            returned += 1;
        } else if next == 0 {
            next = i;
        }
    }
    let mut fb = Vec::with_capacity(16 + out.len());
    for v in [
        u32::try_from(out.len()).unwrap_or(u32::MAX),
        u32::try_from(available).unwrap_or(u32::MAX),
        returned,
        u32::try_from(next).unwrap_or(0),
    ] {
        fb.extend_from_slice(&v.to_be_bytes());
    }
    fb.extend_from_slice(&out);
    Ok(fb)
}

/// The output of a retrieve: bytes returned and available, then `body`,
/// cut to `len`.
fn retrieved(body: &[u8], len: usize) -> Vec<u8> {
    let total = 8 + body.len();
    let mut out = Vec::with_capacity(total);
    let returned = total.min(len);
    out.extend_from_slice(&u32::try_from(returned).unwrap_or(0).to_be_bytes());
    out.extend_from_slice(&u32::try_from(total).unwrap_or(0).to_be_bytes());
    out.extend_from_slice(body);
    out.truncate(len.max(8).min(total));
    out
}

/// `QZNFNFSO`.
fn nfso(
    system: &mut System,
    me: &Principal,
    option: u32,
    input: &[u8],
    len: usize,
    format: &str,
) -> ApiResult {
    let format = format.trim();
    let want = match option {
        1 => "NFOP0101",
        2 => "NFOP0100",
        3 => "NFOP0201",
        4 => "NFOP0200",
        5 => "NFOP0301",
        6 => "NFOP0300",
        _ => return Err(err(Msg::new("CPF3C90").chars("NFS option", 10))),
    };
    if format != want {
        return Err(err(Msg::new("CPF3C21").chars(format, 8)));
    }
    let mut o = super::options(system);
    if option.is_multiple_of(2) && !me.has(SpecialAuth::IOSYSCFG) {
        return Err(err(Msg::new("CPFA1B8").chars("QZNFNFSO", 20)));
    }
    match option {
        1 => {
            let mut b = u32::try_from(o.flavors.len())
                .unwrap_or(0)
                .to_be_bytes()
                .to_vec();
            for f in &o.flavors {
                b.extend_from_slice(&f.to_be_bytes());
            }
            Ok(retrieved(&b, len))
        }
        3 => {
            let d = if o.domain.is_empty() {
                system_domain(system)
            } else {
                o.domain.clone()
            };
            let mut b = 1208u32.to_be_bytes().to_vec();
            b.extend_from_slice(&u32::try_from(d.len()).unwrap_or(0).to_be_bytes());
            b.extend_from_slice(d.as_bytes());
            Ok(retrieved(&b, len))
        }
        5 => {
            let mut b = o.max_threads.to_be_bytes().to_vec();
            b.extend_from_slice(&o.handle_cache.to_be_bytes());
            b.extend_from_slice(&u32::from(o.utf8_validation).to_be_bytes());
            Ok(retrieved(&b, len))
        }
        2 => {
            let n = be(input) as usize;
            let flavors: Vec<u32> = (0..n)
                .map(|i| be(input.get(4 + 4 * i..).unwrap_or(&[])))
                .collect();
            // AUTH_SYS only (docs/NFS.md section 10).
            if flavors.is_empty() || flavors.iter().any(|f| *f != 1) {
                return Err(err(Msg::new("CPFA1BB").vary("sec").vary("flavor")));
            }
            o.flavors = vec![1];
            save(system, &o)
        }
        4 => {
            let ccsid = u16::try_from(be(input)).unwrap_or(1208);
            let n = be(input.get(4..).unwrap_or(&[])) as usize;
            let raw = input.get(8..8 + n).unwrap_or(&[]);
            o.domain = text(if ccsid == 0 { 1208 } else { ccsid }, raw)
                .trim()
                .to_string();
            save(system, &o)
        }
        _ => {
            o.max_threads = be(input);
            o.handle_cache = be(input.get(4..).unwrap_or(&[]));
            o.utf8_validation = be(input.get(8..).unwrap_or(&[])) != 0;
            save(system, &o)
        }
    }
}

fn save(system: &mut System, o: &NfsOptions) -> ApiResult {
    super::save_options(system, o)
        .map(|()| Vec::new())
        .map_err(|e| {
            eprintln!("slic: nfs: options: {e}");
            err(Msg::new("CPFA1B4").bin4(0))
        })
}

/// The TCP/IP domain name, the NFSv4 domain's default.
fn system_domain(system: &mut System) -> String {
    crate::stgsvc::tcp::config(system)
        .map(|c| c.domain.domain_name.trim().to_string())
        .unwrap_or_default()
}
