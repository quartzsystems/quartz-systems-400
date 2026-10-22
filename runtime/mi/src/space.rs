// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Fat pointers and the spaces they address. A pointer names a space by
//! number and generation; a deref checks the kind, that the space is
//! still live and that the offset is inside it, so a pointer into a
//! returned frame or a freed allocation fails instead of reaching
//! whatever is there now.

use codegen::Cond;

/// What a pointer points at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PtrKind {
    /// Null.
    Null = 0,
    /// A space: storage with an offset.
    Space = 1,
    /// A system object.
    System = 2,
    /// A procedure.
    Procedure = 3,
}

impl PtrKind {
    fn from_byte(b: u8) -> Option<Self> {
        Some(match b {
            0 => Self::Null,
            1 => Self::Space,
            2 => Self::System,
            3 => Self::Procedure,
            _ => return None,
        })
    }
}

/// The 16 bytes of a pointer in storage: kind, flags, generation,
/// space number, offset; little-endian fields.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FatPtr {
    /// Kind byte.
    pub kind: u8,
    /// Flags; unused so far.
    pub flags: u8,
    /// Generation of the space when the pointer was made.
    pub generation: u16,
    /// Space number, or the code address's low half for procedures.
    pub space: u32,
    /// Byte offset in the space, or a code address for procedures.
    pub offset: u64,
}

impl FatPtr {
    /// From the 16 bytes in storage.
    #[must_use]
    pub fn from_bytes(b: &[u8; 16]) -> Self {
        Self {
            kind: b[0],
            flags: b[1],
            generation: u16::from_le_bytes([b[2], b[3]]),
            space: u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
            offset: u64::from_le_bytes(b[8..16].try_into().expect("8 bytes")),
        }
    }

    /// From the two halves generated code passes.
    #[must_use]
    pub fn from_halves(lo: u64, hi: u64) -> Self {
        let mut b = [0u8; 16];
        b[..8].copy_from_slice(&lo.to_le_bytes());
        b[8..].copy_from_slice(&hi.to_le_bytes());
        Self::from_bytes(&b)
    }

    /// The 16 bytes in storage.
    #[must_use]
    pub fn to_bytes(self) -> [u8; 16] {
        let mut b = [0u8; 16];
        b[0] = self.kind;
        b[1] = self.flags;
        b[2..4].copy_from_slice(&self.generation.to_le_bytes());
        b[4..8].copy_from_slice(&self.space.to_le_bytes());
        b[8..16].copy_from_slice(&self.offset.to_le_bytes());
        b
    }

    /// The null pointer.
    #[must_use]
    pub fn null() -> Self {
        Self::default()
    }

    /// Whether it is null.
    #[must_use]
    pub fn is_null(self) -> bool {
        self.kind == PtrKind::Null as u8
    }

    /// A procedure pointer to native code.
    #[must_use]
    pub fn procedure(code: u64) -> Self {
        Self {
            kind: PtrKind::Procedure as u8,
            flags: 0,
            generation: 0,
            space: 0,
            offset: code,
        }
    }

    /// The kind, when valid.
    #[must_use]
    pub fn kind(self) -> Option<PtrKind> {
        PtrKind::from_byte(self.kind)
    }
}

/// A region of storage a pointer can address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Space {
    /// Native address of the first byte.
    pub base: u64,
    /// Bytes.
    pub len: u64,
    /// Generation; bumped when the space dies so stale pointers fail.
    pub generation: u16,
    /// Alive.
    pub live: bool,
    /// Call-stack depth that owns it; 0 for static storage.
    pub owner_depth: u32,
}

/// The job's space table.
#[derive(Clone, Debug, Default)]
#[allow(clippy::struct_field_names)]
pub struct Spaces {
    spaces: Vec<Space>,
    statics: Vec<(u64, u64)>,
    /// Static areas of system-state programs: the system domain.
    system: Vec<(u64, u64)>,
}

impl Spaces {
    /// Record a static area, which pointers may address for ever.
    pub fn register_static(&mut self, base: u64, len: u64) {
        if !self.statics.contains(&(base, len)) {
            self.statics.push((base, len));
        }
    }

    /// Record a system-state program's static area as system domain.
    pub fn register_system(&mut self, base: u64, len: u64) {
        if !self.system.contains(&(base, len)) {
            self.system.push((base, len));
        }
    }

    /// Whether a native address lies in system-domain storage.
    #[must_use]
    pub fn in_system(&self, addr: u64) -> bool {
        self.system
            .iter()
            .any(|(b, l)| addr >= *b && addr < b.saturating_add(*l))
    }

    /// A space over storage the runtime owns (a heap block, a mapped
    /// user space): lives until [`Spaces::kill`].
    pub fn add_region(&mut self, base: u64, len: u64) -> FatPtr {
        self.spaces.push(Space {
            base,
            len,
            generation: 1,
            live: true,
            owner_depth: 0,
        });
        FatPtr {
            kind: PtrKind::Space as u8,
            flags: 0,
            generation: 1,
            space: u32::try_from(self.spaces.len() - 1).expect("few spaces"),
            offset: 0,
        }
    }

    /// Kill the runtime-owned space at a base address, if any.
    pub fn kill_at(&mut self, base: u64) {
        for s in &mut self.spaces {
            if s.live && s.owner_depth == 0 && s.base == base {
                s.live = false;
                s.generation = s.generation.wrapping_add(1);
            }
        }
    }

    /// Kill a space by number; pointers into it fail with `MCH3402`.
    pub fn kill(&mut self, space: u32) {
        if let Some(s) = self.spaces.get_mut(space as usize)
            && s.live
        {
            s.live = false;
            s.generation = s.generation.wrapping_add(1);
        }
    }

    /// The space a pointer names, when live: its number and length.
    pub fn space_info(&self, p: FatPtr) -> Result<(u32, u64), Cond> {
        let s = self.space_of(p)?;
        Ok((p.space, s.len))
    }

    /// Forget a static area: the spaces inside it die, so pointers into
    /// a reclaimed activation group fail with `MCH3402`.
    pub fn kill_static(&mut self, base: u64) {
        self.statics.retain(|(b, _)| *b != base);
        self.system.retain(|(b, _)| *b != base);
        for s in &mut self.spaces {
            if s.live && s.owner_depth == 0 && s.base == base {
                s.live = false;
                s.generation = s.generation.wrapping_add(1);
            }
        }
    }

    fn in_statics(&self, addr: u64, len: u64) -> bool {
        self.statics
            .iter()
            .any(|(b, l)| addr >= *b && addr.saturating_add(len) <= b.saturating_add(*l))
    }

    /// A space pointer to `len` bytes at a native address, owned by the
    /// call-stack entry at `depth` unless the storage is static.
    pub fn addr(&mut self, native: u64, len: u64, depth: u32) -> FatPtr {
        let owner = if self.in_statics(native, len) {
            0
        } else {
            depth
        };
        // Reuse a live space that already covers the bytes.
        for (i, s) in self.spaces.iter().enumerate() {
            if s.live
                && s.owner_depth == owner
                && native >= s.base
                && native + len <= s.base + s.len
            {
                return FatPtr {
                    kind: PtrKind::Space as u8,
                    flags: 0,
                    generation: s.generation,
                    space: u32::try_from(i).expect("few spaces"),
                    offset: native - s.base,
                };
            }
        }
        // Grow a static space to cover the whole area when possible.
        if owner == 0
            && let Some((b, l)) = self
                .statics
                .iter()
                .find(|(b, l)| native >= *b && native + len <= b + l)
        {
            let (b, l) = (*b, *l);
            self.spaces.push(Space {
                base: b,
                len: l,
                generation: 1,
                live: true,
                owner_depth: 0,
            });
            return FatPtr {
                kind: PtrKind::Space as u8,
                flags: 0,
                generation: 1,
                space: u32::try_from(self.spaces.len() - 1).expect("few spaces"),
                offset: native - b,
            };
        }
        self.spaces.push(Space {
            base: native,
            len,
            generation: 1,
            live: true,
            owner_depth: owner,
        });
        FatPtr {
            kind: PtrKind::Space as u8,
            flags: 0,
            generation: 1,
            space: u32::try_from(self.spaces.len() - 1).expect("few spaces"),
            offset: 0,
        }
    }

    fn space_of(&self, p: FatPtr) -> Result<&Space, Cond> {
        match p.kind() {
            Some(PtrKind::Null) => return Err(Cond::Mch3601),
            Some(PtrKind::Space) => {}
            _ => return Err(Cond::Mch3602),
        }
        let s = self.spaces.get(p.space as usize).ok_or(Cond::Mch3402)?;
        if !s.live || s.generation != p.generation {
            return Err(Cond::Mch3402);
        }
        Ok(s)
    }

    /// The native address a pointer addresses.
    pub fn deref(&self, p: FatPtr) -> Result<u64, Cond> {
        let s = self.space_of(p)?;
        if p.offset >= s.len {
            return Err(Cond::Mch0601);
        }
        Ok(s.base + p.offset)
    }

    /// The pointer `n` bytes on; one past the end is allowed, as the
    /// machine allows, but not beyond.
    pub fn add(&self, p: FatPtr, n: i64) -> Result<FatPtr, Cond> {
        let s = self.space_of(p)?;
        let off = i128::from(p.offset) + i128::from(n);
        if off < 0 || off > i128::from(s.len) {
            return Err(Cond::Mch0601);
        }
        Ok(FatPtr {
            offset: u64::try_from(off).expect("in range"),
            ..p
        })
    }

    /// `p - q` in bytes, within one space.
    pub fn diff(&self, p: FatPtr, q: FatPtr) -> Result<i64, Cond> {
        self.space_of(p)?;
        self.space_of(q)?;
        if p.space != q.space {
            return Err(Cond::Mch0601);
        }
        Ok(i64::try_from(i128::from(p.offset) - i128::from(q.offset)).expect("fits"))
    }

    /// Kill the spaces owned by call-stack entries deeper than `depth`.
    pub fn leave(&mut self, depth: u32) {
        for s in &mut self.spaces {
            if s.live && s.owner_depth > depth {
                s.live = false;
                s.generation = s.generation.wrapping_add(1);
            }
        }
    }

    /// Live spaces, for displays.
    #[must_use]
    pub fn live(&self) -> usize {
        self.spaces.iter().filter(|s| s.live).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_die_on_leave() {
        let mut sp = Spaces::default();
        sp.register_static(1000, 100);
        let st = sp.addr(1010, 4, 3);
        let fr = sp.addr(5000, 8, 3);
        assert_eq!(sp.deref(st).unwrap(), 1010);
        assert_eq!(sp.deref(fr).unwrap(), 5000);
        assert_eq!(sp.add(fr, 8).unwrap().offset, 8);
        assert_eq!(sp.add(fr, 9), Err(Cond::Mch0601));
        assert_eq!(sp.diff(sp.add(st, 20).unwrap(), st).unwrap(), 20);
        assert_eq!(sp.diff(st, fr), Err(Cond::Mch0601));
        sp.leave(2);
        assert_eq!(sp.deref(st).unwrap(), 1010);
        assert_eq!(sp.deref(fr), Err(Cond::Mch3402));
        assert_eq!(sp.deref(FatPtr::null()), Err(Cond::Mch3601));
        assert_eq!(sp.deref(FatPtr::procedure(7)), Err(Cond::Mch3602));
        let again = sp.addr(5000, 8, 3);
        assert_ne!((again.space, again.generation), (fr.space, fr.generation));
        assert_eq!(FatPtr::from_bytes(&again.to_bytes()), again);
    }
}
