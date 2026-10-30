// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `*EXITRG` payload: a header, then each exit point's record
//! followed by its programs' records, each program's data after its
//! record. Counts are little-endian as in every work management
//! payload; names and texts are CP037 (`docs/HOSTSVR.md`, § Exit
//! points).

use crate::error::{RegFail, RegResult};
use crate::model::{Desc, ExitPoint, ExitProgram, MtAction, Prep, ThreadSafe};
use crate::registry::Registry;
use crate::spec::{Layout, field, read};
use crate::text;

/// The payload's first four bytes, ASCII.
pub const MAGIC: &[u8; 4] = b"QREG";
/// The payload version.
pub const VERSION: u32 = 1;

/// The header.
pub static HEADER: Layout = Layout {
    name: "ExitrgHeader",
    size: 16,
    fields: &[
        field("RGMAGIC", 0, 4),
        field("RGVERS", 4, 4),
        field("RGCOUNT", 8, 4),
        field("RGRSV", 12, 4),
    ],
};

/// One exit point.
pub static POINT: Layout = Layout {
    name: "ExitrgPoint",
    size: 208,
    fields: &[
        field("EPNAME", 0, 20),
        field("EPFMT", 20, 8),
        field("EPMAX", 28, 4),
        field("EPFLAGS", 32, 1),
        field("EPDIND", 33, 1),
        field("EPPADD", 34, 28),
        field("EPPRMV", 62, 28),
        field("EPPRTV", 90, 28),
        field("EPMSGF", 118, 10),
        field("EPMSGL", 128, 10),
        field("EPMSGID", 138, 7),
        field("EPTEXT", 145, 50),
        field("EPRSV1", 195, 1),
        field("EPPGMS", 196, 4),
        field("EPRSV2", 200, 8),
    ],
};

/// One exit program; its data follows.
pub static PROGRAM: Layout = Layout {
    name: "ExitrgProgram",
    size: 116,
    fields: &[
        field("XPNBR", 0, 4),
        field("XPCCSID", 4, 4),
        field("XPDLEN", 8, 4),
        field("XPPGM", 12, 10),
        field("XPLIB", 22, 10),
        field("XPTHDS", 32, 1),
        field("XPMTA", 33, 1),
        field("XPDIND", 34, 1),
        field("XPRSV1", 35, 1),
        field("XPMSGF", 36, 10),
        field("XPMSGL", 46, 10),
        field("XPMSGID", 56, 7),
        field("XPTEXT", 63, 50),
        field("XPRSV2", 113, 3),
    ],
};

/// The payload layouts, for the spec test.
pub static LAYOUTS: &[&Layout] = &[&HEADER, &POINT, &PROGRAM];

/// Flag bits of `EPFLAGS`.
const REGISTERED: u8 = 1;
const ALLOW_DEREG: u8 = 2;
const ALLOW_CHANGE: u8 = 4;

fn prep_bytes(p: Option<&Prep>) -> Vec<u8> {
    match p {
        Some(p) => {
            let mut b = text::fixed(&p.pgm, 10);
            b.extend(text::fixed(&p.lib, 10));
            b.extend(text::fixed(&p.format, 8));
            b
        }
        None => vec![0x40; 28],
    }
}

fn prep_of(b: &[u8]) -> Option<Prep> {
    let pgm = text::field(&b[..10]);
    (!pgm.is_empty()).then(|| Prep {
        pgm,
        lib: text::field(&b[10..20]),
        format: text::field(&b[20..28]),
    })
}

fn desc_parts(d: &Desc) -> (char, &str, &str, &str, &str) {
    match d {
        Desc::Text { text } => ('T', "", "", "", text.as_str()),
        Desc::Msg { msgf, lib, msgid } => ('M', msgf.as_str(), lib.as_str(), msgid.as_str(), ""),
    }
}

fn desc_of(ind: u8, msgf: &[u8], lib: &[u8], id: &[u8], txt: &[u8]) -> Desc {
    if ccsid::char_of(ind) == 'M' {
        Desc::Msg {
            msgf: text::field(msgf),
            lib: text::field(lib),
            msgid: text::field(id),
        }
    } else {
        Desc::Text {
            text: text::field(txt),
        }
    }
}

fn le(i: i32) -> u32 {
    u32::from_ne_bytes(i.to_ne_bytes())
}

fn signed(u: u32) -> i32 {
    i32::from_ne_bytes(u.to_ne_bytes())
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The payload of `reg`.
#[must_use]
pub fn encode(reg: &Registry) -> Vec<u8> {
    let mut out = Vec::new();
    let mut h = HEADER.zeroed();
    h.bytes("RGMAGIC", MAGIC)
        .le4("RGVERS", VERSION)
        .le4("RGCOUNT", count(reg.points.len()));
    out.extend(h.into_bytes());
    for p in &reg.points {
        let (ind, msgf, lib, id, txt) = desc_parts(&p.desc);
        let mut flags = 0;
        if p.registered {
            flags |= REGISTERED;
        }
        if p.allow_dereg {
            flags |= ALLOW_DEREG;
        }
        if p.allow_change {
            flags |= ALLOW_CHANGE;
        }
        let mut r = POINT.blank();
        r.text("EPNAME", &p.name)
            .text("EPFMT", &p.format)
            .le4("EPMAX", le(p.max_code()))
            .bytes("EPFLAGS", &[flags])
            .ch("EPDIND", ind)
            .bytes("EPPADD", &prep_bytes(p.prep_add.as_ref()))
            .bytes("EPPRMV", &prep_bytes(p.prep_rmv.as_ref()))
            .bytes("EPPRTV", &prep_bytes(p.prep_rtv.as_ref()))
            .text("EPMSGF", msgf)
            .text("EPMSGL", lib)
            .text("EPMSGID", id)
            .text("EPTEXT", txt)
            .bytes("EPRSV1", &[0])
            .le4("EPPGMS", count(p.programs.len()))
            .bytes("EPRSV2", &[]);
        out.extend(r.into_bytes());
        for g in &p.programs {
            let (ind, msgf, lib, id, txt) = desc_parts(&g.desc);
            let mut r = PROGRAM.blank();
            r.le4("XPNBR", le(g.number))
                .le4("XPCCSID", le(g.ccsid))
                .le4("XPDLEN", count(g.data.len()))
                .text("XPPGM", &g.pgm)
                .text("XPLIB", &g.lib)
                .ch("XPTHDS", g.threadsafe.code())
                .ch("XPMTA", g.mt_action.code())
                .ch("XPDIND", ind)
                .bytes("XPRSV1", &[0])
                .text("XPMSGF", msgf)
                .text("XPMSGL", lib)
                .text("XPMSGID", id)
                .text("XPTEXT", txt)
                .bytes("XPRSV2", &[]);
            out.extend(r.into_bytes());
            out.extend(&g.data);
        }
    }
    out
}

fn corrupt(what: &str) -> RegFail {
    RegFail::unavailable(&format!(
        "the registration facility repository is damaged: {what}"
    ))
}

fn take<'a>(b: &'a [u8], at: &mut usize, n: usize, what: &str) -> RegResult<&'a [u8]> {
    let s = b.get(*at..*at + n).ok_or_else(|| corrupt(what))?;
    *at += n;
    Ok(s)
}

/// The registry a payload holds.
pub fn decode(b: &[u8]) -> RegResult<Registry> {
    let mut at = 0;
    let h = take(b, &mut at, HEADER.size, "header")?;
    if read(&HEADER, h, "RGMAGIC") != MAGIC {
        return Err(corrupt("not a registration payload"));
    }
    let version = text::le4(h, HEADER.get("RGVERS").offset);
    if version != VERSION {
        return Err(corrupt(&format!("version {version}")));
    }
    let n = text::le4(h, HEADER.get("RGCOUNT").offset);
    let mut reg = Registry::default();
    for _ in 0..n {
        let r = take(b, &mut at, POINT.size, "exit point")?;
        let f = |name: &str| read(&POINT, r, name);
        let flags = f("EPFLAGS")[0];
        let max = signed(text::le4(r, POINT.get("EPMAX").offset));
        let mut p = ExitPoint {
            name: text::field(f("EPNAME")),
            format: text::field(f("EPFMT")),
            registered: flags & REGISTERED != 0,
            allow_dereg: flags & ALLOW_DEREG != 0,
            allow_change: flags & ALLOW_CHANGE != 0,
            max: u32::try_from(max).ok(),
            prep_add: prep_of(f("EPPADD")),
            prep_rmv: prep_of(f("EPPRMV")),
            prep_rtv: prep_of(f("EPPRTV")),
            desc: desc_of(
                f("EPDIND")[0],
                f("EPMSGF"),
                f("EPMSGL"),
                f("EPMSGID"),
                f("EPTEXT"),
            ),
            programs: Vec::new(),
        };
        let pgms = text::le4(r, POINT.get("EPPGMS").offset);
        for _ in 0..pgms {
            let g = take(b, &mut at, PROGRAM.size, "exit program")?;
            let f = |name: &str| read(&PROGRAM, g, name);
            let len = text::le4(g, PROGRAM.get("XPDLEN").offset) as usize;
            let data = take(b, &mut at, len, "exit program data")?.to_vec();
            p.programs.push(ExitProgram {
                number: signed(text::le4(g, PROGRAM.get("XPNBR").offset)),
                pgm: text::field(f("XPPGM")),
                lib: text::field(f("XPLIB")),
                data,
                ccsid: signed(text::le4(g, PROGRAM.get("XPCCSID").offset)),
                threadsafe: ThreadSafe::of_code(ccsid::char_of(f("XPTHDS")[0])),
                mt_action: MtAction::of_code(ccsid::char_of(f("XPMTA")[0])),
                desc: desc_of(
                    f("XPDIND")[0],
                    f("XPMSGF"),
                    f("XPMSGL"),
                    f("XPMSGID"),
                    f("XPTEXT"),
                ),
            });
        }
        reg.points.push(p);
    }
    if at != b.len() {
        return Err(corrupt("bytes after the last exit point"));
    }
    Ok(reg)
}
