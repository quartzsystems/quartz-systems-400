// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The native output of a module as bytes: code, relocations,
//! procedures with their statement maps, the static frame.

use std::collections::HashMap;

use codegen::abi::{StaticLayout, StaticSlot};
use codegen::{ModuleCode, ProcCode, RegionDesc, Reloc, RelocKind, RelocTarget, RtFn, StmtRange};
use qir::Scope;

use crate::bin::{R, W};
use crate::error::Result;

/// Encode a module's native output.
pub(crate) fn encode(w: &mut W, c: &ModuleCode) {
    w.str(&c.name);
    w.u32(c.version);
    w.bytes(&c.code);
    w.list(&c.relocs, |w, r| {
        w.u32(r.offset);
        w.u8(match r.kind {
            RelocKind::Abs8 => 0,
            RelocKind::PcRel4 => 1,
        });
        match &r.target {
            RelocTarget::Runtime(f) => {
                w.u8(0);
                w.u32(f.index());
            }
            RelocTarget::ImportProc(n) => {
                w.u8(1);
                w.str(n);
            }
            RelocTarget::StaticFrame => w.u8(2),
            RelocTarget::ImportData(n) => {
                w.u8(3);
                w.str(n);
            }
            RelocTarget::AttnFlag => w.u8(4),
        }
        w.i64(r.addend);
    });
    w.list(&c.procs, |w, p| {
        w.str(&p.name);
        w.u32(p.index);
        w.u32(p.offset);
        w.u32(p.size);
        w.bool(p.export);
        w.bool(p.entry);
        w.list(&p.stmts, |w, s| {
            w.u32(s.stmt);
            w.u32(s.start);
            w.u32(s.end);
        });
    });
    let mut slots: Vec<(&(Scope, u32, u32), &StaticSlot)> = c.statics.slots.iter().collect();
    slots.sort_by_key(|(k, _)| **k);
    w.list(&slots, |w, (k, s)| {
        w.u8(match k.0 {
            Scope::Module => 0,
            Scope::Proc => 1,
        });
        w.u32(k.1);
        w.u32(k.2);
        w.u64(s.offset);
        w.u64(s.size);
    });
    w.u64(c.statics.size);
    w.bytes(&c.static_init);
    w.list(&c.data_exports, |w, (n, o)| {
        w.str(n);
        w.u64(*o);
    });
    w.list(&c.regions, |w, r| {
        w.list(&r.handlers, |w, (k, a, b)| {
            w.i64(*k);
            w.i64(*a);
            w.i64(*b);
        });
        w.u32(r.parent);
    });
}

/// Decode a module's native output.
pub(crate) fn decode(r: &mut R<'_>) -> Result<ModuleCode> {
    let name = r.str()?;
    let version = r.u32()?;
    let code = r.bytes()?;
    let relocs = r.list(|r| {
        let offset = r.u32()?;
        let kind = match r.u8()? {
            0 => RelocKind::Abs8,
            1 => RelocKind::PcRel4,
            v => return Err(r.err(format!("{v} is not a relocation kind"))),
        };
        let target = match r.u8()? {
            0 => {
                let i = r.u32()?;
                RelocTarget::Runtime(
                    RtFn::from_index(i).ok_or_else(|| r.err(format!("runtime function {i}")))?,
                )
            }
            1 => RelocTarget::ImportProc(r.str()?),
            2 => RelocTarget::StaticFrame,
            3 => RelocTarget::ImportData(r.str()?),
            4 => RelocTarget::AttnFlag,
            v => return Err(r.err(format!("{v} is not a relocation target"))),
        };
        let addend = r.i64()?;
        Ok(Reloc {
            offset,
            kind,
            target,
            addend,
        })
    })?;
    let procs = r.list(|r| {
        Ok(ProcCode {
            name: r.str()?,
            index: r.u32()?,
            offset: r.u32()?,
            size: r.u32()?,
            export: r.bool()?,
            entry: r.bool()?,
            stmts: r.list(|r| {
                Ok(StmtRange {
                    stmt: r.u32()?,
                    start: r.u32()?,
                    end: r.u32()?,
                })
            })?,
        })
    })?;
    let mut slots = HashMap::new();
    for (k, s) in r.list(|r| {
        let scope = match r.u8()? {
            0 => Scope::Module,
            1 => Scope::Proc,
            v => return Err(r.err(format!("{v} is not a scope"))),
        };
        let proc = r.u32()?;
        let index = r.u32()?;
        let offset = r.u64()?;
        let size = r.u64()?;
        Ok(((scope, proc, index), StaticSlot { offset, size }))
    })? {
        slots.insert(k, s);
    }
    let size = r.u64()?;
    let static_init = r.bytes()?;
    let data_exports = r.list(|r| Ok((r.str()?, r.u64()?)))?;
    let regions = r.list(|r| {
        Ok(RegionDesc {
            handlers: r.list(|r| Ok((r.i64()?, r.i64()?, r.i64()?)))?,
            parent: r.u32()?,
        })
    })?;
    Ok(ModuleCode {
        name,
        version,
        code,
        relocs,
        procs,
        statics: StaticLayout { slots, size },
        static_init,
        data_exports,
        regions,
    })
}
