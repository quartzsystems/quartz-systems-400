// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The parameters of the change APIs, built from the model: what a
//! program passes, in [`crate::api`]'s order.

use crate::fmt::qsl::{self, Own};
use crate::fmt::text::{Rec, fixed};
use crate::model::attrs::Settings;
use crate::model::share::{Kind, Qualified, Share};

fn b4(v: i32) -> Vec<u8> {
    v.to_be_bytes().to_vec()
}

fn flag(v: bool) -> Vec<u8> {
    fixed(if v { "1" } else { "0" }, 1)
}

fn qualified(q: &Qualified) -> Vec<u8> {
    let mut b = fixed(&q.name, 10);
    b.extend(fixed(&q.library, 10));
    b
}

fn autl(a: &str) -> Vec<u8> {
    fixed(if a.is_empty() { "*NONE" } else { a }, 10)
}

/// The add (`add`) or change call of share `s`.
#[must_use]
pub fn share(s: &Share, add: bool) -> (&'static str, Vec<Vec<u8>>) {
    match &s.kind {
        Kind::File(f) => {
            let path = f.path.as_bytes().to_vec();
            let len = i32::try_from(path.len()).unwrap_or(i32::MAX);
            let mut table = Rec::zeros(0);
            for (i, e) in f.extensions.iter().enumerate() {
                table.size4(i * 50, e.chars().count().min(46));
                table.char(i * 50 + 4, 46, e);
            }
            let count = i32::try_from(f.extensions.len()).unwrap_or(i32::MAX);
            let parms = vec![
                fixed(&s.name, 12),
                path,
                b4(len),
                b4(1208),
                fixed(&s.text, 50),
                b4(f.permission.code()),
                b4(f.max_users),
                b4(f.text_ccsid),
                flag(f.text_conversion),
                // An empty table still counts as passed.
                if table.buf.is_empty() {
                    vec![0; 50]
                } else {
                    table.buf
                },
                b4(count),
                flag(s.encryption_required),
                autl(&s.autl),
            ];
            (if add { "QZLSADFS" } else { "QZLSCHFS" }, parms)
        }
        Kind::Print(p) => {
            let pf = if p.printer_file.is_empty() {
                qualified(&Qualified::new("*NONE", ""))
            } else {
                qualified(&p.printer_file)
            };
            let parms = vec![
                fixed(&s.name, 12),
                qualified(&p.outq),
                fixed(&s.text, 50),
                b4(p.spool_type.code()),
                fixed(&p.driver, 50),
                pf,
                flag(p.publish),
                flag(s.encryption_required),
                autl(&s.autl),
            ];
            (if add { "QZLSADPS" } else { "QZLSCHPS" }, parms)
        }
    }
}

/// `ZLSS0100` from `s`: every field, the 7.5 length.
#[must_use]
pub fn s0100(s: &Settings) -> Vec<u8> {
    let mut r = Rec::zeros(crate::fmt::zlss::S0100_SIZE);
    r.bin4(0, s.code_page);
    r.bin4(4, s.idle_timeout);
    r.bin4(8, s.oplock_timeout);
    r.bin4(12, s.browse_interval);
    r.char(16, 15, &s.wins_primary);
    r.char(31, 15, &s.wins_secondary);
    r.char(46, 224, &s.scope);
    r.flag(270, s.wins_proxy);
    r.bin4(272, 0);
    r.bin4(276, s.server_role);
    r.char(280, 1, &s.auth_method.to_string());
    r.bin4(284, s.message_auth);
    r.bin4(288, s.min_message_severity);
    r.bin4(292, s.lanman_auth);
    r.bin4(296, s.encryption);
    r.char(
        300,
        10,
        if s.server_autl.is_empty() {
            "*NONE"
        } else {
            &s.server_autl
        },
    );
    r.buf
}

/// `ZLSS0200`: the count and the names.
#[must_use]
pub fn s0200(users: &[String]) -> Vec<u8> {
    let mut r = Rec::zeros(4);
    r.size4(0, users.len());
    for (i, u) in users.iter().enumerate() {
        r.char(4 + 10 * i, 10, u);
    }
    r.buf
}

fn chsi(request: Vec<u8>, format: &str) -> Vec<Vec<u8>> {
    let len = i32::try_from(request.len()).unwrap_or(i32::MAX);
    vec![request, b4(len), fixed(format, 8)]
}

/// The calls that take pending attributes `from` to `to`.
#[must_use]
pub fn attributes(from: &Settings, to: &Settings) -> Vec<(&'static str, Vec<Vec<u8>>)> {
    let mut out = Vec::new();
    if (
        &from.server_name,
        &from.domain,
        &from.text,
        from.system_name_access,
    ) != (&to.server_name, &to.domain, &to.text, to.system_name_access)
    {
        out.push((
            "QZLSCHSN",
            vec![
                fixed(&to.server_name, 15),
                fixed(&to.domain, 15),
                fixed(&to.text, 50),
                flag(to.system_name_access),
            ],
        ));
    }
    if from.guest != to.guest {
        let g = if to.guest.is_empty() {
            "*NONE"
        } else {
            &to.guest
        };
        out.push(("QZLSCHSG", vec![fixed(g, 10)]));
    }
    if s0100(from) != s0100(to) {
        out.push(("QZLSCHSI", chsi(s0100(to), "ZLSS0100")));
    }
    if Own::of(from) != Own::of(to) {
        out.push(("QZLSCHSI", chsi(qsl::s0100(Own::of(to)), "QSLS0100")));
    }
    out
}
