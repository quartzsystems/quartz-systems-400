// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The receivers of `QusRetrieveExitInformation` (`QUSRTVEI`):
//! `EXTI0100` (exit points), `EXTI0200` (exit programs) and `EXTI0300`
//! (exit programs with their descriptions), the continuation handle
//! and the exit program selection criteria. Layouts from `QUSREG`.

use crate::error::{RegFail, RegResult};
use crate::model::{Desc, ExitPoint, ExitProgram, Prep};
use crate::registry::MAX_DATA;
use crate::spec::{Layout, Rec, field};
use crate::text;

/// The header every format starts with.
pub static HEADER: Layout = Layout {
    name: "EXTIHeader",
    size: 36,
    fields: &[
        field("BYTRTN", 0, 4),
        field("BYTAVL", 4, 4),
        field("CONTH", 8, 16),
        field("OFFENT", 24, 4),
        field("NBRENT", 28, 4),
        field("LENENT", 32, 4),
    ],
};

/// One exit point of `EXTI0100`.
pub static EXTI0100: Layout = Layout {
    name: "EXTI0100",
    size: 204,
    fields: &[
        field("EPNAME", 0, 20),
        field("EPFMT", 20, 8),
        field("MAXPGM", 28, 4),
        field("NBRPGM", 32, 4),
        field("ALWDRG", 36, 1),
        field("ALWCHG", 37, 1),
        field("REGD", 38, 1),
        field("PADDN", 39, 10),
        field("PADDL", 49, 10),
        field("PADDF", 59, 8),
        field("PRMVN", 67, 10),
        field("PRMVL", 77, 10),
        field("PRMVF", 87, 8),
        field("PRTVN", 95, 10),
        field("PRTVL", 105, 10),
        field("PRTVF", 115, 8),
        field("DIND", 123, 1),
        field("DMSGF", 124, 10),
        field("DMSGL", 134, 10),
        field("DMSGID", 144, 7),
        field("TEXT", 151, 50),
        field("RSV", 201, 3),
    ],
};

/// One exit program of `EXTI0200`; its data follows.
pub static EXTI0200: Layout = Layout {
    name: "EXTI0200",
    size: 76,
    fields: &[
        field("NEXT", 0, 4),
        field("EPNAME", 4, 20),
        field("EPFMT", 24, 8),
        field("REGD", 32, 1),
        field("COMPL", 33, 1),
        field("RSV", 34, 2),
        field("PGMNBR", 36, 4),
        field("PGM", 40, 10),
        field("PGMLIB", 50, 10),
        field("CCSID", 60, 4),
        field("OFFDTA", 64, 4),
        field("LENDTA", 68, 4),
        field("THDS", 72, 1),
        field("MTA", 73, 1),
        field("SVU", 74, 1),
        field("RSV2", 75, 1),
    ],
};

/// One exit program of `EXTI0300`; its data follows.
pub static EXTI0300: Layout = Layout {
    name: "EXTI0300",
    size: 156,
    fields: &[
        field("NEXT", 0, 4),
        field("EPNAME", 4, 20),
        field("EPFMT", 24, 8),
        field("REGD", 32, 1),
        field("COMPL", 33, 1),
        field("RSV", 34, 2),
        field("PGMNBR", 36, 4),
        field("PGM", 40, 10),
        field("PGMLIB", 50, 10),
        field("DIND", 60, 1),
        field("MSGF", 61, 10),
        field("MSGL", 71, 10),
        field("MSGID", 81, 7),
        field("TEXT", 88, 50),
        field("RSV2", 138, 2),
        field("CCSID", 140, 4),
        field("OFFDTA", 144, 4),
        field("LENDTA", 148, 4),
        field("THDS", 152, 1),
        field("MTA", 153, 1),
        field("SVU", 154, 1),
        field("RSV3", 155, 1),
    ],
};

/// One selection criterion's fixed part (`Qus_Select_Entry`).
pub static SELECT: Layout = Layout {
    name: "SelectEntry",
    size: 16,
    fields: &[
        field("SIZE", 0, 4),
        field("COMPOP", 4, 4),
        field("START", 8, 4),
        field("LENCMP", 12, 4),
    ],
};

/// The receiver layouts, for the spec test.
pub static LAYOUTS: &[&Layout] = &[&HEADER, &EXTI0100, &EXTI0200, &EXTI0300, &SELECT];

/// One exit program selection criterion: the program data at `start`
/// (1-based) equals `data`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Select {
    /// Comparison operator: 1 is equal, the only one IBM defines.
    pub op: i32,
    /// Start in the program data, 1-based.
    pub start: usize,
    /// The comparison data.
    pub data: Vec<u8>,
}

fn n32(n: usize) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

/// The criteria in a selection parameter: a count, then each entry.
pub fn parse_selection(b: &[u8]) -> RegResult<Vec<Select>> {
    let n = text::bin4(b, 0);
    if n < 0 {
        return Err(RegFail::id("CPF3CE7", format!("{n} selection criteria")));
    }
    let mut out = Vec::new();
    let mut at = 4;
    for _ in 0..n {
        let size = usize::try_from(text::bin4(b, at)).unwrap_or(0);
        let op = text::bin4(b, at + 4);
        let start = text::bin4(b, at + 8);
        let len = text::bin4(b, at + 12);
        if op != 1 {
            return Err(RegFail::id("CPF3CE4", format!("comparison operator {op}")).bin4(op));
        }
        let Ok(start) = usize::try_from(start) else {
            return Err(RegFail::id("CPF3CE8", format!("start position {start}")));
        };
        if start < 1 {
            return Err(RegFail::id("CPF3CE8", format!("start position {start}")));
        }
        let Ok(len) = usize::try_from(len) else {
            return Err(RegFail::id("CPF3CE9", format!("comparison length {len}")));
        };
        if len == 0 || len > MAX_DATA {
            return Err(RegFail::id("CPF3CE9", format!("comparison length {len}")));
        }
        if start - 1 + len > MAX_DATA {
            return Err(RegFail::id(
                "CPF3CE6",
                "the comparison runs past the program data".to_string(),
            ));
        }
        let data = b
            .get(at + SELECT.size..at + SELECT.size + len)
            .ok_or_else(|| RegFail::id("CPF3CE9", format!("comparison length {len}")))?
            .to_vec();
        out.push(Select { op, start, data });
        at += size.max(SELECT.size + len);
    }
    Ok(out)
}

/// Whether a program passes every criterion.
#[must_use]
pub fn selected(p: &ExitProgram, sel: &[Select]) -> bool {
    sel.iter().all(|s| {
        p.data
            .get(s.start - 1..s.start - 1 + s.data.len())
            .is_some_and(|d| d == s.data.as_slice())
    })
}

/// A continuation handle naming entry `index`.
#[must_use]
pub fn handle(index: usize) -> Vec<u8> {
    text::fixed(&format!("{index:016}"), 16)
}

/// The entry a continuation handle names: 0 for blanks.
pub fn resume_at(h: &[u8]) -> RegResult<usize> {
    if h.iter().all(|&b| b == 0x40 || b == 0) {
        return Ok(0);
    }
    text::field(h).trim().parse().map_err(|_| {
        RegFail::id(
            "CPF3CE2",
            "the continuation handle is not one this API gave",
        )
    })
}

fn prep(r: &mut Rec, n: &str, l: &str, f: &str, p: Option<&Prep>) {
    match p {
        Some(p) => {
            r.text(n, &p.pgm).text(l, &p.lib).text(f, &p.format);
        }
        None => {
            r.text(n, "*NONE").text(l, "").text(f, "");
        }
    }
}

fn desc(r: &mut Rec, names: [&str; 5], d: &Desc) {
    let [ind, msgf, lib, id, txt] = names;
    r.ch(ind, d.indicator());
    match d {
        Desc::Text { text } => {
            r.text(msgf, "").text(lib, "").text(id, "").text(txt, text);
        }
        Desc::Msg {
            msgf: f,
            lib: l,
            msgid,
        } => {
            r.text(msgf, f).text(lib, l).text(id, msgid).text(txt, "");
        }
    }
}

fn yes(b: bool) -> char {
    if b { '1' } else { '0' }
}

/// One `EXTI0100` entry.
#[must_use]
pub fn point_entry(p: &ExitPoint) -> Vec<u8> {
    let mut r = EXTI0100.blank();
    r.text("EPNAME", &p.name)
        .text("EPFMT", &p.format)
        .bin4("MAXPGM", p.max_code())
        .bin4("NBRPGM", n32(p.programs.len()))
        .ch("ALWDRG", yes(p.allow_dereg))
        .ch("ALWCHG", yes(p.allow_change))
        .ch("REGD", yes(p.registered));
    prep(&mut r, "PADDN", "PADDL", "PADDF", p.prep_add.as_ref());
    prep(&mut r, "PRMVN", "PRMVL", "PRMVF", p.prep_rmv.as_ref());
    prep(&mut r, "PRTVN", "PRTVL", "PRTVF", p.prep_rtv.as_ref());
    desc(
        &mut r,
        ["DIND", "DMSGF", "DMSGL", "DMSGID", "TEXT"],
        &p.desc,
    );
    r.bytes("RSV", &[]);
    r.into_bytes()
}

/// How a receiver's entries chain: the program formats give each
/// entry the offset of the next and of its data, from the receiver's
/// start.
#[derive(Clone, Copy)]
enum Chain {
    /// `EXTI0100`: fixed entries one after another.
    None,
    /// `EXTI0200`/`EXTI0300`: `NEXT` at 0 and the data offset at this
    /// offset of the entry.
    Programs(usize),
}

/// A receiver: header, then entries from `start` while they fit in
/// `room`, the handle naming the first left out.
fn receiver(
    entries: &[Vec<u8>],
    start: usize,
    room: usize,
    fixed_len: usize,
    chain: Chain,
) -> Vec<u8> {
    let mut out = HEADER.zeroed().into_bytes();
    let mut returned = 0;
    let mut next = None;
    let mut total = HEADER.size;
    for (i, e) in entries.iter().enumerate().skip(start) {
        total += e.len();
        if next.is_none() && out.len() + e.len() <= room {
            let at = out.len();
            out.extend(e);
            if let Chain::Programs(offdta) = chain {
                let end = n32(out.len());
                out[at..at + 4].copy_from_slice(&end.to_be_bytes());
                let off = text::bin4(&out, at + offdta);
                if off > 0 {
                    let abs = n32(at) + off;
                    out[at + offdta..at + offdta + 4].copy_from_slice(&abs.to_be_bytes());
                }
            }
            returned += 1;
        } else if next.is_none() {
            next = Some(i);
        }
    }
    let mut h = HEADER.zeroed();
    h.bin4("BYTRTN", n32(out.len().min(room)))
        .bin4("BYTAVL", n32(total))
        .bytes_blank("CONTH", &next.map_or_else(|| vec![0x40; 16], handle))
        .bin4("OFFENT", if returned > 0 { n32(HEADER.size) } else { 0 })
        .bin4("NBRENT", returned)
        .bin4("LENENT", n32(fixed_len));
    out[..HEADER.size].copy_from_slice(&h.into_bytes());
    out
}

/// `EXTI0100` for `points`, from entry `start`, for a receiver of
/// `room` bytes.
#[must_use]
pub fn exti0100(points: &[&ExitPoint], start: usize, room: usize) -> Vec<u8> {
    let entries: Vec<Vec<u8>> = points.iter().map(|p| point_entry(p)).collect();
    receiver(&entries, start, room, EXTI0100.size, Chain::None)
}

/// One `EXTI0200` or `EXTI0300` entry; the data offset is from the
/// entry's start until the receiver makes it absolute.
fn program_entry(p: &ExitPoint, g: &ExitProgram, with_desc: bool) -> Vec<u8> {
    let layout = if with_desc { &EXTI0300 } else { &EXTI0200 };
    let mut r = layout.blank();
    r.bin4("NEXT", 0)
        .text("EPNAME", &p.name)
        .text("EPFMT", &p.format)
        .ch("REGD", yes(p.registered))
        .ch("COMPL", '1')
        .bytes("RSV", &[])
        .bin4("PGMNBR", g.number)
        .text("PGM", &g.pgm)
        .text("PGMLIB", &g.lib)
        .bin4("CCSID", g.ccsid)
        .bin4(
            "OFFDTA",
            if g.data.is_empty() {
                0
            } else {
                n32(layout.size)
            },
        )
        .bin4("LENDTA", n32(g.data.len()))
        .ch("THDS", g.threadsafe.code())
        .ch("MTA", g.mt_action.code())
        .ch(
            "SVU",
            if g.mt_action == crate::model::MtAction::SysVal {
                '1'
            } else {
                '0'
            },
        );
    if with_desc {
        desc(&mut r, ["DIND", "MSGF", "MSGL", "MSGID", "TEXT"], &g.desc);
        r.bytes("RSV2", &[]).bytes("RSV3", &[]);
    } else {
        r.bytes("RSV2", &[]);
    }
    let mut b = r.into_bytes();
    b.extend(&g.data);
    // Entries start on four-byte boundaries.
    while b.len() % 4 != 0 {
        b.push(0);
    }
    b
}

/// `EXTI0200` (`with_desc` false) or `EXTI0300` for the programs of
/// `points` numbered `number` (-1 for every one) that pass `sel`.
#[must_use]
pub fn exti0200(
    points: &[&ExitPoint],
    number: i32,
    sel: &[Select],
    with_desc: bool,
    start: usize,
    room: usize,
) -> Vec<u8> {
    let entries: Vec<Vec<u8>> = points
        .iter()
        .flat_map(|p| {
            p.programs
                .iter()
                .filter(move |g| number == -1 || g.number == number)
                .filter(|g| selected(g, sel))
                .map(move |g| program_entry(p, g, with_desc))
        })
        .collect();
    let fixed = if with_desc {
        EXTI0300.size
    } else {
        EXTI0200.size
    };
    let offdta = if with_desc {
        EXTI0300.get("OFFDTA")
    } else {
        EXTI0200.get("OFFDTA")
    };
    receiver(&entries, start, room, fixed, Chain::Programs(offdta.offset))
}
