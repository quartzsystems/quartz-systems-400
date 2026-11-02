// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The volume's extents at the ASP: loaded and held at open, placed and
//! held when the volume grows, the map rewritten, and the parity sets
//! told which of their pages are raw. The caller checkpoints each change.

use std::collections::BTreeSet;

use tracing::info;

use crate::asp::Asp;
use crate::asp::ifs::volume::Volume;
use crate::asp::ifs::{IfsExtent, map};
use crate::asp::layout::RESERVED_HEAD;
use crate::asp::parity::{geometry_of, unit_of};
use crate::page::pager::PageNo;
use crate::{Refusal, Result, StoreError};

/// Shortest run an extent is made of (in a parity set, over all its
/// members).
pub const MIN_RUN: u64 = 256;
/// Smallest growth, and the smallest volume created: 64 MiB.
pub const MIN_GROW: u64 = 16_384;
/// Smallest volume asked for at creation: 1 GiB.
pub const CREATE_MIN: u64 = 262_144;
/// The ASP threshold when nothing else sets it, percent.
pub const DEFAULT_THRESHOLD: u8 = 90;

/// One extent to be: a span and its pages.
type Piece = (u16, u64, u64);

impl Asp {
    /// Read the map and hold its extents. At open, before anything
    /// allocates.
    pub(crate) fn ifs_load(&mut self) -> Result<()> {
        let Some(head) = self.sb.ifs_map else {
            return Ok(());
        };
        let (extents, pages) = map::read(&mut self.io, head)?;
        let volume = Volume::new(extents);
        if volume.pages() != self.sb.ifs_pages {
            return Err(StoreError::Corrupt {
                page: head,
                what: format!(
                    "IFS map lists {} pages, the superblock {}",
                    volume.pages(),
                    self.sb.ifs_pages
                ),
            });
        }
        for e in volume.extents() {
            let (span, lba) = self.layout().locate(e.start).ok_or_else(|| StoreError::Corrupt {
                page: head,
                what: format!("IFS extent at {} beyond the ASP", e.start),
            })?;
            let s = self.layout().span(span);
            if lba < RESERVED_HEAD || lba + e.pages > s.pages - 1 {
                return Err(StoreError::Corrupt {
                    page: head,
                    what: format!("IFS extent at {} leaves its span", e.start),
                });
            }
            self.alloc.hold(span, lba, e.pages);
        }
        self.ifs = volume;
        self.ifs_map_pages = pages;
        self.ifs_publish_raw();
        Ok(())
    }

    /// The volume.
    #[must_use]
    pub fn ifs_volume(&self) -> &Volume {
        &self.ifs
    }

    /// The map's pages, which RCLSTG keeps.
    #[must_use]
    pub fn ifs_map_pages(&self) -> &[PageNo] {
        &self.ifs_map_pages
    }

    /// Whether a span holds an IFS extent from `lba` on.
    #[must_use]
    pub fn ifs_in_span(&self, span: u16, lba: u64) -> bool {
        let s = self.layout().span(span);
        let from = s.start + lba;
        self.ifs
            .extents()
            .iter()
            .any(|e| e.end() > from && e.start < s.start + s.pages)
    }

    /// Pages the ASP can hold data on: every span less its reserved
    /// pages, the WAL and device parity's share.
    #[must_use]
    pub fn usable_pages(&self) -> u64 {
        let spans: u64 = self
            .layout()
            .spans()
            .iter()
            .map(|s| s.pages - 1 - RESERVED_HEAD)
            .sum();
        spans - self.sb.wal_pages - self.parity_share()
    }

    /// Of [`usable_pages`](Self::usable_pages), those in use: allocated,
    /// pending free and held for the IFS.
    #[must_use]
    pub fn used_pages(&self) -> u64 {
        self.usable_pages().saturating_sub(self.alloc.total_free())
    }

    /// Pages device parity holds over the whole ASP.
    fn parity_share(&self) -> u64 {
        (0..self.io.unit_count())
            .map(|u| self.parity_held(u))
            .sum()
    }

    /// Pages the volume may still grow by: what keeps the ASP at or
    /// below `threshold` percent used, and no more than the allocator
    /// can give.
    #[must_use]
    pub fn ifs_room(&self, threshold: u8) -> u64 {
        let limit = self.usable_pages() * u64::from(threshold.min(100)) / 100;
        limit
            .saturating_sub(self.used_pages())
            .min(self.alloc.available())
    }

    /// Grow the volume by about `want` pages (whole blocks in a parity
    /// set), at most `room`, at least `min` ([`MIN_GROW`] outside tests):
    /// the extents placed and held, a new map written and the old one's
    /// pages freed. Returns the pages added. The caller checkpoints.
    pub fn ifs_grow(&mut self, want: u64, room: u64, min: u64) -> Result<u64> {
        let want = want.min(room);
        if want < min {
            return Err(StoreError::Refused {
                unit: None,
                reason: Refusal::IfsNoRoom(room),
            });
        }
        let pieces = self.ifs_place(want, room);
        let added: u64 = pieces.iter().map(|p| p.2).sum();
        if added < min {
            return Err(StoreError::Refused {
                unit: None,
                reason: Refusal::IfsNoRoom(added),
            });
        }
        let mut extents = self.ifs.extents().to_vec();
        for &(span, lba, len) in &pieces {
            self.alloc.hold(span, lba, len);
            let start = self.layout().logical(span, lba);
            // Runs that continue the last extent extend it.
            match extents.last_mut() {
                Some(last) if last.end() == start => last.pages += len,
                _ => extents.push(IfsExtent { start, pages: len }),
            }
        }
        self.ifs_write_map(extents)?;
        info!(
            "asp {}: IFS volume grew by {added} pages to {}",
            self.sb.asp_number,
            self.ifs.pages()
        );
        Ok(added)
    }

    /// Write `extents` as the new map, free the old one's pages and
    /// point the superblock at it.
    fn ifs_write_map(&mut self, extents: Vec<IfsExtent>) -> Result<()> {
        let mut at = Vec::with_capacity(map::pages_for(extents.len()));
        for _ in 0..map::pages_for(extents.len()) {
            at.push(self.alloc.alloc_page()?);
        }
        map::write(&mut self.io, &at, &extents, self.sb.generation + 1)?;
        for pno in std::mem::take(&mut self.ifs_map_pages) {
            self.alloc.free(pno, 1)?;
        }
        self.ifs = Volume::new(extents);
        self.sb.ifs_map = at.first().copied();
        self.sb.ifs_pages = self.ifs.pages();
        self.ifs_map_pages = at;
        self.ifs_publish_raw();
        Ok(())
    }

    /// Where `want` pages go: per parity set whole blocks free on every
    /// member, elsewhere free runs, lowest first, never more than
    /// `room`.
    fn ifs_place(&self, want: u64, room: u64) -> Vec<Piece> {
        let mut out = Vec::new();
        let mut got = 0u64;
        let mut sets_done = BTreeSet::new();
        for span in 0..self.layout().unit_count() {
            if got >= want {
                break;
            }
            if self.alloc.is_excluded(span) {
                continue;
            }
            let unit = self.layout().span(span).primary;
            match self.parity_set_of(unit) {
                Some(set) => {
                    if sets_done.insert(set) {
                        got += self.place_in_set(set, want - got, room - got, &mut out);
                    }
                }
                None => {
                    for (lba, len) in self.alloc.span_runs(span) {
                        if got >= want {
                            break;
                        }
                        if len < MIN_RUN {
                            continue;
                        }
                        let take = len.min(want - got);
                        out.push((span, lba, take));
                        got += take;
                    }
                }
            }
        }
        out
    }

    /// Whole blocks of `set` free on every member, up to `want` pages
    /// over the members (rounded up to a block, never past `room`).
    fn place_in_set(&self, set: u8, want: u64, room: u64, out: &mut Vec<Piece>) -> u64 {
        let Ok(geo) = geometry_of(&self.sb, set) else {
            return 0;
        };
        let spans: Vec<u16> = self
            .sb
            .parity
            .members(set)
            .iter()
            .filter_map(|t| unit_of(&self.sb, &t.unit_uuid))
            .filter_map(|u| self.layout().span_of_unit(u))
            .collect();
        if spans.len() != geo.n || spans.iter().any(|&s| self.alloc.is_excluded(s)) {
            return 0;
        }
        let w = geo.w() as u64;
        let per_block = w * geo.n as u64;
        // Rows free on every member: the runs intersected.
        let mut common = vec![(RESERVED_HEAD, geo.rows_end() - RESERVED_HEAD)];
        for &s in &spans {
            common = intersect(&common, &self.alloc.span_runs(s));
        }
        let mut got = 0;
        for (lba, len) in common {
            if got >= want {
                break;
            }
            let b0 = (lba - RESERVED_HEAD).div_ceil(w);
            let b1 = (lba + len - RESERVED_HEAD) / w;
            if b1 <= b0 || (b1 - b0) * per_block < MIN_RUN {
                continue;
            }
            let need = (want - got).div_ceil(per_block);
            let fit = (room - got) / per_block;
            let k = (b1 - b0).min(need).min(fit);
            if k == 0 {
                break;
            }
            for &s in &spans {
                out.push((s, RESERVED_HEAD + b0 * w, k * w));
            }
            got += k * per_block;
        }
        got
    }

    /// Tell each parity set which of its members' pages are volume pages.
    pub(crate) fn ifs_publish_raw(&self) {
        for h in &self.parity.sets {
            let Ok(members) = self.set_spans(h.set) else {
                continue;
            };
            let mut e = h.lock();
            for (m, span) in members.into_iter().enumerate() {
                let s = self.layout().span(span);
                let mut runs: Vec<(u64, u64)> = self
                    .ifs
                    .extents()
                    .iter()
                    .filter(|x| x.start >= s.start && x.start < s.start + s.pages)
                    .map(|x| (x.start - s.start, x.pages))
                    .collect();
                runs.sort_unstable();
                e.set_raw(m, runs);
            }
        }
    }

    /// The spans of a set's members, in member order.
    fn set_spans(&self, set: u8) -> Result<Vec<u16>> {
        self.sb
            .parity
            .members(set)
            .iter()
            .map(|t| {
                unit_of(&self.sb, &t.unit_uuid)
                    .and_then(|u| self.layout().span_of_unit(u))
                    .ok_or_else(|| StoreError::Corrupt {
                        page: 1,
                        what: format!("parity set {set}: a member has no span"),
                    })
            })
            .collect()
    }
}

/// The intersection of two ascending lists of runs.
fn intersect(a: &[(u64, u64)], b: &[(u64, u64)]) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        let lo = a[i].0.max(b[j].0);
        let hi = (a[i].0 + a[i].1).min(b[j].0 + b[j].1);
        if lo < hi {
            out.push((lo, hi - lo));
        }
        if a[i].0 + a[i].1 < b[j].0 + b[j].1 {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::intersect;

    #[test]
    fn runs_intersect() {
        assert_eq!(
            intersect(&[(0, 10), (20, 10)], &[(5, 20), (28, 5)]),
            vec![(5, 5), (20, 5), (28, 2)]
        );
        assert!(intersect(&[(0, 5)], &[(5, 5)]).is_empty());
    }
}
