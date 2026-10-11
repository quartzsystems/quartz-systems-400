// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `monitor` regions: lexical ranges of a procedure's blocks that
//! `monitor NAME` opens and `endmon NAME` closes, properly nested.

use crate::inst::Inst;
use crate::module::{Label, Proc};

/// A monitor region.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Region {
    /// Name.
    pub name: String,
    /// Block and instruction of the `monitor`.
    pub start: (usize, usize),
    /// Block and instruction of the `endmon`.
    pub end: (usize, usize),
    /// Handler targets in order.
    pub handlers: Vec<Label>,
}

impl Region {
    /// Whether block `b`'s terminator runs inside the region.
    #[must_use]
    pub fn covers_end(&self, b: usize) -> bool {
        self.start.0 <= b && b < self.end.0
    }

    /// Whether entering block `b` at its start lands inside the region.
    #[must_use]
    pub fn interior(&self, b: usize) -> bool {
        self.start.0 < b && (b < self.end.0 || (b == self.end.0 && self.end.1 > 0))
    }

    /// Whether any instruction of block `b` runs inside the region.
    #[must_use]
    pub fn overlaps(&self, b: usize) -> bool {
        self.start.0 <= b && b <= self.end.0
    }
}

/// The regions of a procedure, outermost first, or why they do not nest.
pub fn regions(p: &Proc) -> Result<Vec<Region>, String> {
    let mut open: Vec<Region> = Vec::new();
    let mut done: Vec<Region> = Vec::new();
    for (bi, b) in p.blocks.iter().enumerate() {
        for (ii, i) in b.insts.iter().enumerate() {
            match i {
                Inst::Monitor { name, handlers } => {
                    if open.iter().chain(done.iter()).any(|r| r.name == *name) {
                        return Err(format!("monitor {name} is opened twice"));
                    }
                    if handlers.is_empty() {
                        return Err(format!("monitor {name} has no handlers"));
                    }
                    open.push(Region {
                        name: name.clone(),
                        start: (bi, ii),
                        end: (bi, ii),
                        handlers: handlers.iter().map(|h| h.target).collect(),
                    });
                }
                Inst::EndMon(name) => match open.pop() {
                    Some(mut r) if r.name == *name => {
                        r.end = (bi, ii);
                        done.push(r);
                    }
                    Some(r) => {
                        return Err(format!("endmon {name} closes monitor {}", r.name));
                    }
                    None => return Err(format!("endmon {name} has no monitor")),
                },
                _ => {}
            }
        }
    }
    if let Some(r) = open.pop() {
        return Err(format!("monitor {} is not closed", r.name));
    }
    done.sort_by_key(|r| r.start);
    Ok(done)
}

/// Check that no branch enters a region except through its `monitor`,
/// and that handlers lie outside their region.
pub fn check_entries(p: &Proc, regions: &[Region]) -> Result<(), String> {
    for r in regions {
        for (bi, b) in p.blocks.iter().enumerate() {
            let Some(last) = b.insts.last() else { continue };
            for t in last.targets() {
                let t = t.0 as usize;
                if r.interior(t) && !r.covers_end(bi) {
                    return Err(format!(
                        "block {} branches into monitor {} at {}",
                        b.label, r.name, p.blocks[t].label
                    ));
                }
            }
        }
        for h in &r.handlers {
            if r.interior(h.0 as usize) {
                return Err(format!(
                    "handler {} of monitor {} lies inside it",
                    p.blocks[h.0 as usize].label, r.name
                ));
            }
        }
    }
    Ok(())
}
