// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The PTF header (`docs/PTF.md` §1).

use crate::spec::{Layout, field};
use crate::{Error, Result};

/// `PtfHeader` size.
pub const HEADER_LEN: usize = 128;
const REF_LEN: usize = 16;
const EXIT_LEN: usize = 32;
const MAGIC: &[u8; 8] = b"QS400PTF";

/// `PtfHeader`.
pub static HEADER_LAYOUT: Layout = Layout {
    name: "PtfHeader",
    size: Some(HEADER_LEN),
    fields: &[
        field("magic", 0, 8),
        field("version", 8, 2),
        field("kind", 10, 1),
        field("flags", 11, 1),
        field("ptf", 12, 8),
        field("product", 20, 7),
        field("option", 27, 4),
        field("release", 31, 6),
        field("load", 37, 4),
        field("target_release", 41, 6),
        field("reserved", 47, 1),
        field("created", 48, 8),
        field("prereq_count", 56, 2),
        field("coreq_count", 58, 2),
        field("supersedes_count", 60, 2),
        field("exit_count", 62, 2),
        field("object_count", 64, 4),
        field("store_format", 68, 4),
        field("reserved2", 72, 56),
    ],
};

/// `PtfRef`.
pub static REF_LAYOUT: Layout = Layout {
    name: "PtfRef",
    size: Some(REF_LEN),
    fields: &[
        field("ptf", 0, 8),
        field("product", 8, 7),
        field("reserved", 15, 1),
    ],
};

/// `ExitProgram`.
pub static EXIT_LAYOUT: Layout = Layout {
    name: "ExitProgram",
    size: Some(EXIT_LEN),
    fields: &[
        field("library", 0, 10),
        field("program", 10, 10),
        field("when", 20, 1),
        field("reserved", 21, 11),
    ],
};

/// Immediate or delayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PtfKind {
    /// Applied while the system runs.
    Immediate,
    /// Applied at the next IPL.
    Delayed,
}

/// Another PTF this one names.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PtfRef {
    /// `QSnnnnn`.
    pub ptf: String,
    /// Product ID.
    pub product: String,
}

/// When an exit program runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitWhen {
    /// Before the objects are replaced.
    BeforeApply,
    /// After they are.
    AfterApply,
    /// Before a remove.
    BeforeRemove,
    /// After a remove.
    AfterRemove,
    /// A data migration program.
    Migration,
}

impl ExitWhen {
    const ALL: [Self; 5] = [
        Self::BeforeApply,
        Self::AfterApply,
        Self::BeforeRemove,
        Self::AfterRemove,
        Self::Migration,
    ];
}

/// An exit program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitProgram {
    /// Library.
    pub library: String,
    /// Program.
    pub program: String,
    /// When it runs.
    pub when: ExitWhen,
}

/// The header.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct PtfHeader {
    /// `QSnnnnn`.
    pub ptf: String,
    /// Product ID.
    pub product: String,
    /// Option, `0000` for `*BASE`.
    pub option: String,
    /// `VxRyMz`.
    pub release: String,
    /// Load ID.
    pub load: String,
    /// The oldest release it applies to.
    pub target_release: String,
    /// Immediate or delayed.
    pub kind: PtfKind,
    /// It replaces the LIC.
    pub lic: bool,
    /// It has data migration programs.
    pub migration: bool,
    /// Only a cover letter.
    pub cover_only: bool,
    /// Unix seconds.
    pub created: u64,
    /// Prerequisites.
    pub prereqs: Vec<PtfRef>,
    /// Co-requisites.
    pub coreqs: Vec<PtfRef>,
    /// The PTFs this one supersedes.
    pub supersedes: Vec<PtfRef>,
    /// Exit programs.
    pub exits: Vec<ExitProgram>,
    /// Replacement objects in the stream.
    pub object_count: u32,
    /// The store format a migration brings the system to, 0 for none.
    pub store_format: u32,
}

fn ascii<const N: usize>(s: &str) -> [u8; N] {
    let mut b = [b' '; N];
    for (o, c) in b.iter_mut().zip(s.bytes()) {
        *o = c;
    }
    b
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).trim_end().to_string()
}

fn put_ref(out: &mut Vec<u8>, r: &PtfRef) {
    out.extend_from_slice(&ascii::<8>(&r.ptf));
    out.extend_from_slice(&ascii::<7>(&r.product));
    out.push(0);
}

impl PtfHeader {
    /// Encode, with the lists after the fixed part.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut b = vec![0u8; HEADER_LEN];
        b[0..8].copy_from_slice(MAGIC);
        b[8..10].copy_from_slice(&1u16.to_be_bytes());
        b[10] = u8::from(self.kind == PtfKind::Delayed);
        b[11] = u8::from(self.lic) | u8::from(self.migration) << 1 | u8::from(self.cover_only) << 2;
        b[12..20].copy_from_slice(&ascii::<8>(&self.ptf));
        b[20..27].copy_from_slice(&ascii::<7>(&self.product));
        b[27..31].copy_from_slice(&ascii::<4>(&self.option));
        b[31..37].copy_from_slice(&ascii::<6>(&self.release));
        b[37..41].copy_from_slice(&ascii::<4>(&self.load));
        b[41..47].copy_from_slice(&ascii::<6>(&self.target_release));
        b[48..56].copy_from_slice(&self.created.to_be_bytes());
        let n = |v: usize| u16::try_from(v).expect("list").to_be_bytes();
        b[56..58].copy_from_slice(&n(self.prereqs.len()));
        b[58..60].copy_from_slice(&n(self.coreqs.len()));
        b[60..62].copy_from_slice(&n(self.supersedes.len()));
        b[62..64].copy_from_slice(&n(self.exits.len()));
        b[64..68].copy_from_slice(&self.object_count.to_be_bytes());
        b[68..72].copy_from_slice(&self.store_format.to_be_bytes());
        for r in self.prereqs.iter().chain(&self.coreqs).chain(&self.supersedes) {
            put_ref(&mut b, r);
        }
        for e in &self.exits {
            let mut x = [0u8; EXIT_LEN];
            ccsid::encode_fixed(&e.library, &mut x[0..10]).ok();
            ccsid::encode_fixed(&e.program, &mut x[10..20]).ok();
            x[20] = u8::try_from(ExitWhen::ALL.iter().position(|w| *w == e.when).expect("listed"))
                .expect("small");
            b.extend_from_slice(&x);
        }
        b
    }

    /// Decode.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let bad = |w: &str| Error::NotPtf(w.into());
        if b.len() < HEADER_LEN || &b[0..8] != MAGIC {
            return Err(bad("no PTF header"));
        }
        if u16::from_be_bytes([b[8], b[9]]) != 1 {
            return Err(bad("header version"));
        }
        let n = |at: usize| usize::from(u16::from_be_bytes([b[at], b[at + 1]]));
        let (np, nc, ns, ne) = (n(56), n(58), n(60), n(62));
        if b.len() != HEADER_LEN + (np + nc + ns) * REF_LEN + ne * EXIT_LEN {
            return Err(bad("header lists"));
        }
        let mut at = HEADER_LEN;
        let mut refs = |k: usize| {
            let v: Vec<PtfRef> = (0..k)
                .map(|i| {
                    let r = &b[at + i * REF_LEN..at + (i + 1) * REF_LEN];
                    PtfRef {
                        ptf: text(&r[0..8]),
                        product: text(&r[8..15]),
                    }
                })
                .collect();
            at += k * REF_LEN;
            v
        };
        let prereqs = refs(np);
        let coreqs = refs(nc);
        let supersedes = refs(ns);
        let mut exits = Vec::new();
        for i in 0..ne {
            let x = &b[at + i * EXIT_LEN..at + (i + 1) * EXIT_LEN];
            exits.push(ExitProgram {
                library: ccsid::decode_trimmed(&x[0..10]),
                program: ccsid::decode_trimmed(&x[10..20]),
                when: *ExitWhen::ALL.get(usize::from(x[20])).ok_or_else(|| bad("exit"))?,
            });
        }
        Ok(Self {
            kind: if b[10] == 1 {
                PtfKind::Delayed
            } else {
                PtfKind::Immediate
            },
            lic: b[11] & 1 != 0,
            migration: b[11] & 2 != 0,
            cover_only: b[11] & 4 != 0,
            ptf: text(&b[12..20]),
            product: text(&b[20..27]),
            option: text(&b[27..31]),
            release: text(&b[31..37]),
            load: text(&b[37..41]),
            target_release: text(&b[41..47]),
            created: u64::from_be_bytes(b[48..56].try_into().expect("8")),
            prereqs,
            coreqs,
            supersedes,
            exits,
            object_count: u32::from_be_bytes(b[64..68].try_into().expect("4")),
            store_format: u32::from_be_bytes(b[68..72].try_into().expect("4")),
        })
    }
}
