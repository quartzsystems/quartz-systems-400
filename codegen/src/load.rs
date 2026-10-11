// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Placing a module's code in executable memory and resolving its
//! relocations. The runtime does this at program activation; the tests
//! do it to run generated code.

use memmap2::{Mmap, MmapMut};

use crate::error::{CodegenError, Result};
use crate::output::{ModuleCode, RelocKind, RelocTarget};

/// What a relocation resolves to: the value written at the site.
pub trait Resolve {
    /// The absolute value for a target: a function address for runtime
    /// and imported procedures, a frame offset for static data.
    fn resolve(&self, target: &RelocTarget) -> Result<u64>;
}

impl<F: Fn(&RelocTarget) -> Result<u64>> Resolve for F {
    fn resolve(&self, target: &RelocTarget) -> Result<u64> {
        self(target)
    }
}

/// A module's code in executable memory.
pub struct Loaded {
    map: Mmap,
    procs: Vec<(String, u32)>,
}

impl Loaded {
    /// Place the code and resolve every relocation.
    pub fn new(code: &ModuleCode, resolver: &dyn Resolve) -> Result<Self> {
        let len = code.code.len().max(1);
        let mut map = MmapMut::map_anon(len).map_err(|e| CodegenError::Load(e.to_string()))?;
        map[..code.code.len()].copy_from_slice(&code.code);
        let base = map.as_ptr() as u64;
        for r in &code.relocs {
            let value = resolver.resolve(&r.target)?;
            let at = r.offset as usize;
            match r.kind {
                RelocKind::Abs8 => {
                    let v = value.wrapping_add_signed(r.addend);
                    map[at..at + 8].copy_from_slice(&v.to_le_bytes());
                }
                RelocKind::PcRel4 => {
                    let site = base + u64::from(r.offset);
                    #[allow(clippy::cast_possible_wrap)]
                    let disp = (value as i64)
                        .wrapping_add(r.addend)
                        .wrapping_sub(site as i64);
                    let d = i32::try_from(disp).map_err(|_| {
                        CodegenError::Load(format!("{:?} is out of reach", r.target))
                    })?;
                    map[at..at + 4].copy_from_slice(&d.to_le_bytes());
                }
            }
        }
        let map = map
            .make_exec()
            .map_err(|e| CodegenError::Load(e.to_string()))?;
        Ok(Self {
            map,
            procs: code
                .procs
                .iter()
                .map(|p| (p.name.clone(), p.offset))
                .collect(),
        })
    }

    /// Address of the code.
    #[must_use]
    pub fn base(&self) -> usize {
        self.map.as_ptr() as usize
    }

    /// Address of a procedure's entry.
    #[must_use]
    pub fn proc_addr(&self, name: &str) -> Option<usize> {
        self.procs
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, off)| self.base() + *off as usize)
    }
}
